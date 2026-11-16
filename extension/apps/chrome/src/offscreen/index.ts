// Offscreen document: owns the microphone, the speech worker and the model files.
import { CLOUD_ERROR_TEXT, CloudError, classifyMicError, cloudById, isSilent, startLiveCloud, transcribeCloud, type LiveCloud, type CloudErrorKind, type CloudJob, type ErrorKind, type ModelStatus, type ToBackground, type ToOffscreen } from "@voicio/core";
import * as dl from "./download";
import workletUrl from "./worklet.ts?worker&url";
import { CATALOG, byId, cacheName, fileUrl, totalBytes } from "../shared/models";
import { micAudio } from "../shared/mic";

const SR = 16000;
const CHUNK = SR / 10; // 100 ms
const MAX_SECS = 15 * 60;
const IDLE_UNLOAD_MS = 10 * 60 * 1000; // the model is released 10 minutes after the last dictation (a warm model makes the next one start at once)
const PARTIAL_WINDOW_CHUNKS = 80; // live text decodes only the last 8 s of the open segment; the final decode always sees everything
const PAUSE_END_MS = 700; // silence that closes a segment
const PARTIAL_EVERY_MS = 600;
const MAX_SEGMENT_SECS = 25;

const send = (m: ToBackground) => void chrome.runtime.sendMessage(m).catch(() => {});
/** Diagnostics only: never contains audio or recognised text. */
const log = (msg: string) => send({ t: "off-log", msg });

/* ---------- speech worker ---------- */
let worker: Worker | null = null;
let seq = 0;
const pending = new Map<number, { ok: (v: { text?: string }) => void; fail: (e: Error) => void }>();
let loaded: Promise<void> | null = null;
let loadedId = "";
let activeId = CATALOG[0]!.id; // the model used for the current dictation
let idleTimer: number | undefined;

function rpc(msg: object): Promise<{ text?: string }> {
  if (!worker) {
    worker = new Worker(new URL("./asr.worker.ts", import.meta.url), { type: "module" });
    worker.onmessage = (e) => {
      const p = pending.get(e.data.id);
      if (!p) return;
      pending.delete(e.data.id);
      e.data.ok ? p.ok(e.data) : p.fail(new Error(e.data.error));
    };
  }
  const id = ++seq;
  return new Promise((ok, fail) => {
    pending.set(id, { ok, fail });
    worker!.postMessage({ id, ...msg });
  });
}
const ensureLoaded = () => {
  if (loaded && loadedId !== activeId) unload(true); // another model was chosen: free the old one first
  loadedId = activeId;
  return (loaded ??= rpc({ t: "load", model: activeId, cache: cacheName(activeId), url: fileUrl(byId(activeId), byId(activeId).files[0]!.path) }).then(() => {}).catch((e) => { loaded = null; throw e; }));
};
/** A quiet microphone (low input volume) hurts recognition. Lift speech to a healthy level; silence and loud audio are left alone. */
function levelUp(a: Float32Array): Float32Array {
  let peak = 0;
  for (let i = 0; i < a.length; i++) { const v = Math.abs(a[i]!); if (v > peak) peak = v; }
  if (peak < 0.01 || peak > 0.3) return a;
  const g = Math.min(8, 0.5 / peak), out = new Float32Array(a.length);
  for (let i = 0; i < a.length; i++) out[i] = a[i]! * g;
  return out;
}

const armIdle = () => { clearTimeout(idleTimer); idleTimer = window.setTimeout(() => unload(), IDLE_UNLOAD_MS); };
const transcribe = async (raw: Float32Array) => {
  const audio = levelUp(raw);
  await ensureLoaded();
  armIdle();
  const out = ((await rpc({ t: "run", audio })).text ?? "").trim();
  log(`recognised ${(audio.length / SR).toFixed(1)} s with ${activeId}: ${out.length} chars`);
  return out;
};
function unload(force = false) {
  if (capture && !force) { armIdle(); return; }
  clearTimeout(idleTimer);
  worker?.terminate(); // frees the model memory
  worker = null; loaded = null; pending.clear(); warmed = "";
}
/** Load the model and run one short throwaway decode so the first real one doesn't pay for kernel set-up. */
let warmed = "";
async function warm(model: string) {
  if (capture) return;
  activeId = byId(model).id;
  await ensureLoaded();
  armIdle();
  if (warmed !== activeId) { warmed = activeId; await rpc({ t: "run", audio: new Float32Array(SR / 2) }).catch(() => {}); }
}

/** Audio levels go straight to the page that shows the bar and to open popups over ports, never through the service worker. */
const levelPorts = new Set<chrome.runtime.Port>();
chrome.runtime.onConnect.addListener((port) => {
  if (port.name !== "voicio-level") return;
  levelPorts.add(port);
  port.onDisconnect.addListener(() => levelPorts.delete(port));
});

/* ---------- capture session ---------- */
interface Capture {
  ctx: AudioContext;
  stream: MediaStream;
  buf: Float32Array; // all audio, grown by doubling (no per-chunk copies)
  n: number; // chunks received, 100 ms each
  lastDecodeMs: number; // how long the last live decode took; the cadence backs off with it
  lastPartialText: string;
  openFrom: number; // chunk index where the open (unfinalised) segment starts
  committed: string[]; // finalised segments' text
  jobs: Promise<unknown>; // sequential ASR queue
  partialBusy: boolean;
  lastPartialAt: number;
  speechChunks: number; // chunks of speech heard in the open segment
  silentChunks: number; // trailing silent chunks
  everSpoke: boolean;
  floorDb: number;
  autostopSecs: number;
  stopped: boolean;
  tickets: number;
  maxDb: number; // loudest 100 ms chunk, for diagnostics
  startedAt: number;
  label: string; // microphone in use
  lost: boolean; // the device disappeared while recording
  cloud?: CloudJob; // optional cloud speech-to-text; the on-device result is the fallback unless `only` (no model installed)
  live?: LiveCloud; // the provider's live stream (cloud.live): its words replace the on-device partials
}
let capture: Capture | null = null;
let lastErr = ""; // most recent recognition failure, reported instead of "no speech" when it explains an empty result

const rmsDb = (a: Float32Array) => {
  let s = 0;
  for (let i = 0; i < a.length; i++) s += a[i]! * a[i]!;
  return 10 * Math.log10(s / a.length + 1e-12);
};
const join = (c: Capture, from: number, to = c.n) => c.buf.slice(from * CHUNK, to * CHUNK);
const words = (t: string) => t.split(/\s+/).filter(Boolean);

/** Open the microphone, trying the chosen device first, then the system default, then the simplest request. */
async function openMic(deviceId: string, retried = false): Promise<{ stream: MediaStream } | { kind: ErrorKind; detail: string }> {
  const attempts: MediaStreamConstraints[] = [];
  if (deviceId) attempts.push({ audio: micAudio(deviceId) });
  attempts.push({ audio: micAudio() }, { audio: true });
  let name = "";
  for (const audio of attempts) {
    try { return { stream: await navigator.mediaDevices.getUserMedia(audio) }; }
    catch (e) {
      name = (e as DOMException).name;
      log(`getUserMedia failed: ${name}`);
      if (name === "NotAllowedError" || name === "SecurityError") break; // asking again will not help
    }
  }
  if (name === "NotFoundError" && !retried) { await new Promise((r) => setTimeout(r, 900)); return openMic(deviceId, true); } // devices can be mid-refresh right after a sound change
  return { kind: classifyMicError(name), detail: name };
}

const abort = (c: Capture, kind: ErrorKind, detail?: string) => { c.live?.cancel(); release(c); send({ t: "off-error", kind, detail }); };

async function start(autostopSecs: number, model: string, micId = "", cloud?: Capture["cloud"]) {
  if (capture) { capture.live?.cancel(); release(capture); } // a recording that was never closed must not leak its text into the new one
  activeId = byId(model).id;
  // The extension origin has its own microphone permission, and only a visible page can show Chrome's prompt.
  const perm = await navigator.permissions.query({ name: "microphone" as PermissionName }).catch(() => null);
  if (perm?.state === "prompt") { send({ t: "off-error", kind: "mic-prompt" }); return; }
  if (perm?.state === "denied") { send({ t: "off-error", kind: "mic-blocked" }); return; }
  const opened = await openMic(micId);
  if ("kind" in opened) { send({ t: "off-error", kind: opened.kind, detail: opened.detail }); return; }
  const stream = opened.stream;
  const track = stream.getAudioTracks()[0];
  const label = track?.label || "Microphone";
  send({ t: "off-device", label });
  const ctx = new AudioContext({ sampleRate: SR });
  try {
    await Promise.race([ctx.resume().catch(() => {}), new Promise((r) => setTimeout(r, 1000))]); // a suspended context delivers no audio; never wait on it forever
    await ctx.audioWorklet.addModule(workletUrl);
  } catch (e) {
    stream.getTracks().forEach((t) => t.stop()); void ctx.close();
    send({ t: "off-error", kind: "engine", detail: `audio engine: ${String((e as Error).message ?? e)}` });
    return;
  }
  const node = new AudioWorkletNode(ctx, "voicio-capture", { numberOfInputs: 1, numberOfOutputs: 0 });
  ctx.createMediaStreamSource(stream).connect(node);
  const c: Capture = {
    ctx, stream, buf: new Float32Array(SR * 60), n: 0, lastDecodeMs: 0, lastPartialText: "", openFrom: 0, committed: [], jobs: Promise.resolve(), partialBusy: false,
    lastPartialAt: 0, speechChunks: 0, silentChunks: 0, everSpoke: false, floorDb: -50, autostopSecs, stopped: false, tickets: 0, maxDb: -120,
    startedAt: performance.now(), label, lost: false, cloud,
  };
  capture = c;
  const lp = cloud?.live ? cloudById(cloud.provider) : undefined;
  if (lp && cloud) {
    try {
      c.live = startLiveCloud(lp, cloud.key, { model: cloud.model, region: cloud.region, onText: (t) => { if (capture === c && !c.stopped) send({ t: "off-partial", words: words(t) }); } });
    } catch { log(`cloud ${lp.id}: live unavailable, sending the recording whole`); }
  }
  node.port.onmessage = (e: MessageEvent<Float32Array>) => onChunk(c, e.data);
  // Unplugged or revoked while dictating: keep what was already recognised.
  track?.addEventListener("ended", () => { if (capture === c) { c.lost = true; void finish(false); } });
  // The operating system muted the microphone: say so instead of listening to nothing.
  let muteTimer: number | undefined;
  const checkMuted = () => { clearTimeout(muteTimer); if (track?.muted) muteTimer = window.setTimeout(() => { if (capture === c && track.muted) abort(c, "mic-silent", `${label} is muted`); }, 1200); };
  track?.addEventListener("mute", checkMuted); track?.addEventListener("unmute", () => clearTimeout(muteTimer));
  checkMuted();
  // Audio must start flowing: a context that never delivers a chunk is an engine problem, not "no speech".
  window.setTimeout(() => { if (capture === c && c.n === 0) abort(c, "engine", `no audio from ${label} (audio context ${ctx.state})`); }, 2500);
  if (!cloud?.only) void ensureLoaded().catch(() => {}); // warm up while the user starts talking; audio is already buffered
}

function onChunk(c: Capture, a: Float32Array) {
  if (c.stopped) return;
  if ((c.n + 1) * CHUNK > c.buf.length) { const g = new Float32Array(c.buf.length * 2); g.set(c.buf); c.buf = g; }
  c.buf.set(a, c.n * CHUNK); c.n++;
  c.live?.feed(a);
  const db = rmsDb(a);
  c.maxDb = Math.max(c.maxDb, db);
  // A device that only ever delivers zeros is muted or the wrong input: say so now instead of at the end.
  if (isSilent(c.maxDb, performance.now() - c.startedAt)) { abort(c, "mic-silent", c.label); return; }
  if (levelPorts.size) { const v = Math.max(0, Math.min(1, (db + 60) / 45)); for (const p of levelPorts) try { p.postMessage({ v }); } catch { levelPorts.delete(p); } }
  // Adaptive threshold: speech is 9 dB over the quietest recent level, never below -48 dB.
  c.floorDb = db < c.floorDb ? db : c.floorDb + 0.05;
  const speech = db > Math.max(-48, c.floorDb + 9);
  if (speech) { c.speechChunks++; c.silentChunks = 0; c.everSpoke = true; }
  else c.silentChunks++;

  if (c.cloud?.only) { if (c.n >= MAX_SECS * 10) send({ t: "off-silence" }); return; } // no on-device model: nothing to decode while listening
  const openLen = c.n - c.openFrom;
  // A long enough pause (or a very long segment) closes the segment: it is decoded once and kept.
  if ((c.speechChunks > 2 && c.silentChunks * 100 >= PAUSE_END_MS) || openLen * 0.1 >= MAX_SEGMENT_SECS) commitSegment(c);
  else if (c.speechChunks === 0 && c.silentChunks > 8) c.openFrom = c.n - 3; // drop leading silence

  const now = performance.now();
  if (!c.live && c.speechChunks > 2 && !c.partialBusy && now - c.lastPartialAt > Math.max(PARTIAL_EVERY_MS, c.lastDecodeMs)) void partial(c);

  if (c.autostopSecs > 0 && c.everSpoke && c.speechChunks === 0 && c.silentChunks >= c.autostopSecs * 10) { send({ t: "off-silence" }); c.silentChunks = 0; }
  if (c.n >= MAX_SECS * 10) send({ t: "off-silence" });
}

function commitSegment(c: Capture) {
  const audio = join(c, c.openFrom);
  c.openFrom = c.n;
  c.speechChunks = 0; c.silentChunks = 0;
  const slot = c.committed.length;
  c.committed.push("");
  c.jobs = c.jobs.then(async () => {
    try { c.committed[slot] = await transcribe(audio); } catch (e) { lastErr = String((e as Error).message ?? e); console.error("voicio: recognition failed", e); log("recognition failed: " + lastErr); }
    if (!c.stopped && !c.live) send({ t: "off-partial", words: words(c.committed.join(" ")) });
  });
}

async function partial(c: Capture) {
  c.partialBusy = true;
  c.lastPartialAt = performance.now();
  try {
    const audio = join(c, Math.max(c.openFrom, c.n - PARTIAL_WINDOW_CHUNKS));
    await c.jobs;
    const t0 = performance.now();
    const text = await transcribe(audio);
    c.lastDecodeMs = performance.now() - t0;
    const all = [...c.committed, text].join(" ");
    if (capture === c && !c.stopped && all !== c.lastPartialText) { c.lastPartialText = all; send({ t: "off-partial", words: words(all) }); }
  } catch (e) { lastErr = String((e as Error).message ?? e); log("partial failed: " + lastErr); }
  finally { c.partialBusy = false; c.lastPartialAt = performance.now(); }
}

function release(c: Capture) {
  c.stopped = true;
  c.stream.getTracks().forEach((t) => t.stop()); // mic light goes off immediately
  void c.ctx.close();
  if (capture === c) { capture = null; armIdle(); }
}

let cloudAbort: AbortController | undefined;
async function finish(cancelled: boolean, error?: ErrorKind) {
  const c = capture;
  if (!c) return;
  const t0 = performance.now();
  const open = join(c, c.openFrom);
  const hadSpeech = c.everSpoke || c.speechChunks > 0;
  lastErr = lastErr && c.committed.length === 0 ? lastErr : "";
  release(c);
  if (cancelled) { c.live?.cancel(); if (error) send({ t: "off-error", kind: error }); return; }
  if (!hadSpeech && c.committed.length === 0) {
    send({ t: "off-error", kind: c.lost ? "mic-lost" : "no-speech", detail: `${c.label}: loudest ${c.maxDb.toFixed(0)} dBFS` });
    return;
  }
  // Cloud (opt-in) runs alongside the on-device pass; any failure keeps the on-device text.
  const cp = cloudById(c.cloud?.provider ?? "");
  let cloudErr: CloudErrorKind | undefined;
  cloudAbort?.abort();
  const abort = (cloudAbort = new AbortController()); // "cancel" while the cloud request runs stops it (upload, retry wait or poll)
  abort.signal.addEventListener("abort", () => c.live?.cancel());
  const batch = () => transcribeCloud(cp!, c.cloud!.key, join(c, 0), { model: c.cloud!.model, region: c.cloud!.region, signal: abort.signal });
  // live: the stream's text; if it failed and there is no on-device model, the recording is sent whole instead
  const viaLive = c.live?.finish().catch((e: unknown) => { if (c.cloud?.only && !abort.signal.aborted) { log(`cloud ${cp?.id} live failed: ${(e as CloudError).kind}; sending whole`); return batch(); } throw e; });
  const cloudP = cp && c.cloud ? (viaLive ?? batch()).catch((e) => { cloudErr = e instanceof CloudError ? e.kind : "network"; log(`cloud ${cp.id} failed: ${cloudErr}`); return ""; }) : null;
  if (c.cloud?.only) { // no on-device model to fall back to: the cloud's answer (or its error) is the result
    const text = (await cloudP) ?? "";
    if (abort.signal.aborted) return; // the user cancelled: send nothing
    if (text) send({ t: "off-final", text, ms: Math.round(performance.now() - t0), cloud: true });
    else send(cloudErr ? { t: "off-error", kind: "engine", detail: `cloud: ${CLOUD_ERROR_TEXT[cloudErr]}` } : { t: "off-error", kind: "no-speech" });
    return;
  }
  let local = "", localErr: unknown;
  try {
    await c.jobs;
    const tail = c.speechChunks > 0 || c.committed.length === 0 ? await transcribe(open) : "";
    local = [...c.committed, tail].join(" ").replace(/\s+/g, " ").trim();
  } catch (e) { localErr = e; console.error("voicio: recognition failed", e); }
  const cloudText = (await cloudP) ?? "";
  if (abort.signal.aborted) return; // the user cancelled: send nothing
  if (cloudText) { log(`cloud ${cp!.id}: ${cloudText.length} chars`); send({ t: "off-final", text: cloudText, ms: Math.round(performance.now() - t0), cloud: true }); return; }
  if (localErr) send({ t: "off-error", kind: "engine", detail: String((localErr as Error).message ?? localErr) });
  else if (!local) send(lastErr ? { t: "off-error", kind: "engine", detail: lastErr } : { t: "off-error", kind: "no-speech" });
  else send({ t: "off-final", text: local, ms: Math.round(performance.now() - t0), cloudError: cloudErr });
}

/* ---------- model ops ---------- */
const aborts = new Map<string, AbortController>();
const keepFlags = new Map<string, boolean>();
const statuses = new Map<string, ModelStatus>();
const statusOf = (id: string): ModelStatus => statuses.get(id) ?? { state: "unknown", bytes: 0, total: totalBytes(byId(id)), speed: 0 };
const report = (id: string, s: ModelStatus) => { statuses.set(id, s); send({ t: "off-model", id, status: s }); };

async function modelCheck(id: string) {
  if (aborts.has(id)) { report(id, statusOf(id)); return; } // a download is running: don't reset its progress
  const m = byId(id);
  const ok = await dl.isInstalled(m);
  const part = ok ? 0 : await dl.partialBytes(m);
  report(id, { state: ok ? "installed" : part > 0 ? "paused" : "missing", bytes: ok ? totalBytes(m) : part, total: totalBytes(m), speed: 0 });
}
async function modelInstall(id: string) {
  if ([...aborts.keys()].length) return; // one download at a time keeps Chrome responsive
  const m = byId(id);
  const abort = new AbortController();
  aborts.set(id, abort);
  keepFlags.set(id, true);
  report(id, { ...statusOf(id), state: "downloading", speed: 0, total: totalBytes(m) });
  try {
    const r = await dl.install(m, (s) => report(id, s), abort.signal, () => keepFlags.get(id) !== false);
    if (r === "done") report(id, { state: "installed", bytes: totalBytes(m), total: totalBytes(m), speed: 0 });
    else { const keep = keepFlags.get(id) !== false; report(id, { state: keep ? "paused" : "missing", bytes: keep ? await dl.partialBytes(m) : 0, total: totalBytes(m), speed: 0 }); }
  } catch (e) {
    report(id, { state: "failed", bytes: 0, total: totalBytes(m), speed: 0, error: String((e as Error).message) });
  } finally { aborts.delete(id); }
}
async function bench(id: string) {
  const buf = await (await fetch(chrome.runtime.getURL("bench.wav"))).arrayBuffer();
  const pcm = new Int16Array(buf, 44);
  const audio = Float32Array.from(pcm, (v) => v / 32768);
  activeId = byId(id).id;
  await ensureLoaded();
  await transcribe(audio.slice(0, SR)); // warm-up
  const t = performance.now();
  await transcribe(audio);
  const ms = performance.now() - t;
  const heapMB = Math.round(((performance as unknown as { memory?: { usedJSHeapSize: number } }).memory?.usedJSHeapSize ?? 0) / 1e6);
  send({ t: "off-bench", rtf: ms / 1000 / (audio.length / SR), ms: Math.round(ms), id, heapMB });
}

chrome.runtime.onMessage.addListener((m: ToOffscreen, _s, reply) => {
  if (m?.target !== "offscreen") return;
  if (m.t === "warm") void warm(m.model).catch(() => {});
  else if (m.t === "start") void start(m.autostopSecs, m.model, m.micId, m.cloud).catch((e) => send({ t: "off-error", kind: "engine", detail: String((e as Error)?.message ?? e) }));
  else if (m.t === "stop") void finish(false);
  else if (m.t === "cancel") { cloudAbort?.abort(); void finish(true); }
  else if (m.t === "model") {
    if (m.op === "check") void Promise.all(CATALOG.map((x) => modelCheck(x.id)));
    else if (m.op === "install") void modelInstall(m.id);
    else if (m.op === "pause") { keepFlags.set(m.id, true); aborts.get(m.id)?.abort(); }
    else if (m.op === "cancel") { keepFlags.set(m.id, false); if (aborts.has(m.id)) aborts.get(m.id)!.abort(); else void dl.remove(byId(m.id)).then(() => modelCheck(m.id)); }
    else if (m.op === "remove") { if (loadedId === m.id) unload(true); void dl.remove(byId(m.id)).then(() => modelCheck(m.id)); }
    else if (m.op === "bench") void bench(m.id).catch((e) => console.error("voicio: bench failed", e));
  }
  reply(true);
  return false;
});
window.addEventListener("online", () => send({ t: "off-online" }));
send({ t: "off-ready" });
