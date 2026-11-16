// Optional bring-your-own-key cloud speech-to-text. Pure: builds requests, encodes audio, parses replies, maps errors.
// Providers and request shapes come from the shared registry providers/cloud-stt.json (the desktop app and Android
// read the same file). The network call runs in the offscreen document; nothing here touches chrome.* or logs a key.
import { z } from "zod";
import registry from "../../../../providers/cloud-stt.json";

interface Spec {
  style: string; url?: string; auth: string[]; headers?: Record<string, string>; query?: Record<string, string>;
  file_field?: string; model_field?: string; fields?: Record<string, string>;
  json_field?: { name: string; value: string; value_auto?: string };
  lang: { field?: string; format: string; auto?: string; list?: boolean };
  body?: string; generation_config?: Record<string, unknown>; text: string; result?: string; cleanup?: string[];
  upload?: { url: string; body: string; id: string };
  create?: { url: string; ref_field: string; model_field: string; model_list?: boolean; model_fallback?: string; id: string };
  status?: { url: string; field: string; done: string; failed: string };
}
/** A provider's streaming WebSocket (registry `live`). Same rules on desktop (cloud.rs) and Android (Cloud.kt). */
export interface LiveSpec {
  models: string[]; default_model: string; max_seconds: number; experimental?: boolean; notes?: string; docs?: string;
  url: string; query?: Record<string, string>; lang?: { field?: string; format: string; auto?: string }; auth?: string[];
  /** How a browser connects: a short-lived token from `token.url` (fetched with the key in the batch auth header). */
  browser?: { token: { url: string; body?: string; text: string }; query?: string; protocols?: string[] };
  start?: string; start_auto?: string; ready?: Record<string, unknown>; rate?: number;
  /** "" = binary pcm_s16le frames; otherwise a JSON template with {audio} (base64). */
  audio?: string;
  messages: { match: Record<string, unknown>; do: string; text?: string }[];
  join?: string; end?: string; end_done?: string;
}
export interface CloudProvider {
  id: string; name: string; maker: string; models: string[]; defaultModel: string; priceNote: string; speedNote: string;
  /** Where the audio goes (named in the consent text). Azure's contains `{region}`: use hostFor. */
  host: string; needsRegion: boolean; slow: boolean; experimental: boolean; notes: string; docs: string; maxSeconds: number;
  spec: Spec;
  /** The streaming API, if any; `liveSupported` = Chrome can use it (it has a browser token path). */
  live?: LiveSpec; liveSupported: boolean;
}
interface Raw {
  id: string; name: string; maker: string; models: string[]; default_model: string; price_note: string; speed_note: string;
  host: string; needs_region: boolean; slow: boolean; experimental: boolean; notes?: string; docs: string; max_seconds: number; live?: LiveSpec;
}
/** Every supported provider, in the registry's order (model-first `name`, `maker` as the subtitle). */
export const CLOUD: CloudProvider[] = (registry.providers as unknown as Raw[]).map((p) => ({
  id: p.id, name: p.name, maker: p.maker, models: p.models, defaultModel: p.default_model, priceNote: p.price_note, speedNote: p.speed_note,
  host: p.host, needsRegion: p.needs_region, slow: p.slow, experimental: p.experimental, notes: p.notes ?? "", docs: p.docs,
  maxSeconds: p.max_seconds, spec: p as unknown as Spec, live: p.live, liveSupported: !!p.live?.browser,
}));
/** Providers left out on purpose, with the reason the UI shows. */
export const CLOUD_EXCLUDED: { id: string; name: string; reason: string }[] = registry.excluded;
export const cloudById = (id: string): CloudProvider | undefined => CLOUD.find((p) => p.id === id);

/** `region`: an Azure region ("westeurope") or a full resource host. */
export const hostFor = (p: CloudProvider, region = ""): string =>
  !p.needsRegion ? p.host : azureHost(region, p.host) ?? p.host.replace("{region}", region.trim());
/** Azure: a region name, or a host under one of Azure's two Speech domains (scheme and "/" ignored); undefined otherwise. Same as desktop/Android. */
export function azureHost(region: string, template: string): string | undefined {
  const r = region.trim().toLowerCase().replace(/^https:\/\//, "").replace(/\/+$/, "");
  const label = (s: string) => /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(s);
  if (label(r)) return template.replace("{region}", r);
  const d = [".cognitiveservices.azure.com", ".api.cognitive.microsoft.com"].find((d) => r.endsWith(d) && label(r.slice(0, -d.length)));
  return d ? r : undefined;
}
/** The optional host permission Chrome must grant before audio can go to this provider. */
export const originFor = (p: CloudProvider, region = "") => `https://${hostFor(p, region)}/*`;
/** Every origin Chrome must grant for this provider: the batch host, plus the live token and WebSocket hosts when it streams. */
export const originsFor = (p: CloudProvider, region = ""): string[] => [...new Set([originFor(p, region),
  ...(p.liveSupported ? [p.live!.browser!.token.url, p.live!.url].map((u) => `https://${new URL(u).host}/*`) : [])])];
/** Every host the registry can reach, as CSP sources (region hosts as wildcards; Azure resource hosts too). Used by the manifest. */
export const cloudHosts = (): string[] => [...new Set(CLOUD.flatMap((p) => p.needsRegion ? [`https://${p.host.replace("{region}", "*")}`, "https://*.cognitiveservices.azure.com"] : [`https://${p.host}`]))];

export type CloudMode = "off" | "always" | "fallback";
/** Stored in chrome.storage.local only (never sync). `consented` lists providers whose "your audio is sent to X" notice was accepted. */
export const CloudConfig = z.object({
  provider: z.string().default(""),
  keys: z.record(z.string(), z.string()).default({}),
  consented: z.array(z.string()).default([]),
  /** "always": every dictation (on-device text is the fallback); "fallback": only when no on-device model is installed. */
  mode: z.enum(["off", "always", "fallback"]).optional(),
  models: z.record(z.string(), z.string()).default({}),
  region: z.string().default(""),
  /** Providers whose "Live" toggle is on (words stream in while speaking); off by default. */
  live: z.array(z.string()).default([]),
});
export type CloudConfig = Omit<z.infer<typeof CloudConfig>, "mode"> & { mode: CloudMode };
/** Older configs had no mode: a chosen provider meant "always". */
export const loadCloud = (stored: unknown): CloudConfig => {
  const r = CloudConfig.safeParse(stored ?? {});
  const c = r.success ? r.data : CloudConfig.parse({});
  return { ...c, mode: c.mode ?? (c.provider ? "always" : "off") };
};
export interface ActiveCloud { provider: CloudProvider; key: string; model: string; region: string; mode: CloudMode; live: boolean }
/** The provider, key and model to use, or undefined when cloud is off or incomplete. */
export function activeCloud(c: CloudConfig): ActiveCloud | undefined {
  const provider = cloudById(c.provider), key = c.keys[c.provider]?.trim();
  if (!provider || !key || c.mode === "off" || !c.consented.includes(provider.id)) return undefined;
  const m = c.models[provider.id];
  return { provider, key, model: m && provider.models.includes(m) ? m : provider.defaultModel, region: c.region, mode: c.mode, live: provider.liveSupported && c.live.includes(provider.id) };
}

/** 16 kHz mono 16-bit PCM WAV. */
export function encodeWav(pcm: Float32Array, sampleRate = 16000): Uint8Array {
  const out = new Uint8Array(44 + pcm.length * 2), v = new DataView(out.buffer);
  const tag = (o: number, s: string) => { for (let i = 0; i < 4; i++) out[o + i] = s.charCodeAt(i); };
  tag(0, "RIFF"); v.setUint32(4, 36 + pcm.length * 2, true); tag(8, "WAVE"); tag(12, "fmt ");
  v.setUint32(16, 16, true); v.setUint16(20, 1, true); v.setUint16(22, 1, true);
  v.setUint32(24, sampleRate, true); v.setUint32(28, sampleRate * 2, true); v.setUint16(32, 2, true); v.setUint16(34, 16, true);
  tag(36, "data"); v.setUint32(40, pcm.length * 2, true);
  for (let i = 0; i < pcm.length; i++) { const s = Math.max(-1, Math.min(1, pcm[i]!)); v.setInt16(44 + i * 2, s < 0 ? s * 0x8000 : s * 0x7fff, true); }
  return out;
}

export type CloudErrorKind = "auth" | "rate" | "quota" | "audio" | "server" | "network" | "reply" | "too-long" | "region" | "no-text" | "cancelled";
export class CloudError extends Error { constructor(public kind: CloudErrorKind, public status?: number) { super(`cloud ${kind}${status ? ` (${status})` : ""}`); } }
/** An HTTP failure to the error the user sees. Bodies are only searched, never shown or logged. Same rules as desktop/Android. */
export function mapStatus(s: number, body = ""): CloudErrorKind {
  const b = body.toLowerCase(), has = (w: string[]) => w.some((x) => b.includes(x));
  const credit = has(["insufficient_quota", "insufficient_credit", "credit", "balance", "payment", "billing_hard_limit", "quota_exceeded"]);
  const badKey = has(["api_key_invalid", "api key not valid", "invalid api key", "invalid_api_key", "incorrect api key", "invalid_auth", "unauthorized"]);
  if (s === 402) return "quota";
  if (s === 401 || s === 403) return credit ? "quota" : "auth";
  if (s === 429) return credit ? "quota" : "rate";
  if (s === 413) return "too-long";
  if (s === 400 && badKey) return "auth"; // Gemini: 400 API_KEY_INVALID
  if (s === 408) return "network";
  if (s === 498 || s >= 500) return "server"; // 498: Groq flex tier at capacity
  return "audio";
}
/** The same texts as the desktop app and Android. */
export const CLOUD_ERROR_TEXT: Record<CloudErrorKind, string> = {
  auth: "The provider rejected this API key.", rate: "Rate limited by the provider; try again shortly.", quota: "This account is out of credit or quota.",
  audio: "The provider could not process the audio.", server: "The provider is having problems.", network: "Could not reach the provider (offline or timed out).",
  reply: "The provider sent a reply Voicio could not read.", "too-long": "This recording is too long for the provider.",
  region: "Enter the region (for example westeurope) or the resource host of your Azure Speech resource.",
  "no-text": "The provider returned no transcript.", cancelled: "Cancelled.",
};
/** A Retry-After longer than this is not waited for: the dictation keeps the on-device text. */
const MAX_RETRY_AFTER_S = 10;
/** Milliseconds before the single retry, or undefined = do not retry (only rate limits, server errors, dropped connections). */
export function retryAfterMs(kind: CloudErrorKind, header?: string | null, random = () => Math.floor(Math.random() * 1000)): number | undefined {
  if (kind !== "rate" && kind !== "server" && kind !== "network") return undefined;
  const v = header?.trim();
  if (!v || !/^\d+$/.test(v)) return 500 + random();
  const s = Number(v);
  return s <= MAX_RETRY_AFTER_S ? s * 1000 : undefined;
}
/** Per request: 30 s plus 2 s per second of audio (covers a slow upload of the WAV). */
export const timeoutFor = (samples: number) => 30000 + Math.floor(samples * 2000 / 16000);
/** Splits audio longer than `max` samples at its quietest 100 ms in the second half of each stretch. Same as desktop/Android. */
export function splitAudio(pcm: Float32Array, max: number): [number, number][] {
  const W = 1600, STEP = 800, out: [number, number][] = [];
  let start = 0;
  while (max > W && pcm.length - start > max) {
    let best = start + max - W, bestE = Infinity;
    for (let i = start + Math.floor(max / 2); i <= start + max - W; i += STEP) {
      let e = 0;
      for (let k = i; k < i + W; k++) e += pcm[k]! * pcm[k]!;
      if (e <= bestE) { bestE = e; best = i; } // ties: the latest, for fewer requests
    }
    const cut = best + W / 2;
    out.push([start, cut]); start = cut;
  }
  out.push([start, pcm.length]);
  return out;
}

/** LLM transcribers (Gemini, OpenRouter) sometimes wrap the text: drop a code fence, a "Here is the transcript:" preamble and quotes. Same rules as desktop/Android. */
export function tidy(s: string): string {
  let t = s.trim();
  if (t.startsWith("```")) { const nl = t.indexOf("\n"); t = (nl < 0 ? "" : t.slice(nl + 1)).trimEnd(); if (t.endsWith("```")) t = t.slice(0, -3); t = t.trim(); }
  const i = t.indexOf(":");
  if (i >= 0 && i < 80) { const head = t.slice(0, i).toLowerCase(); if (!head.includes("\n") && (head.includes("transcri") || head.startsWith("here is") || head.startsWith("here's"))) t = t.slice(i + 1).trim(); }
  for (const [a, b] of [["\"", "\""], ["“", "”"]] as const) if (t.length >= 2 && t.startsWith(a) && t.endsWith(b)) { t = t.slice(1, -1).trim(); break; }
  return t;
}

/** `tidy`, then "" for an answer about the audio instead of a transcript ("I'm sorry, I can't transcribe this audio", "[silence]"). */
export function transcriptOnly(s: string): string {
  const t = tidy(s), l = t.toLowerCase();
  const marker = t.length < 40 && /^(\[.*\]|\(.*\)|<.*>)$/s.test(t);
  const openers = ["i'm sorry", "i am sorry", "sorry", "i cannot", "i can't", "i can not", "i'm unable", "i am unable", "unfortunately", "as an ai", "there is no", "there's no", "no speech", "the audio", "this audio", "the recording", "this recording", "the provided audio"];
  const aboutAudio = ["audio", "transcri", "speech", "recording"].some((w) => l.includes(w));
  return marker || (aboutAudio && openers.some((o) => l.startsWith(o))) ? "" : t;
}

const at = (v: unknown, pointer: string): unknown => pointer.split("/").slice(1).reduce<unknown>((cur, k) => (cur as Record<string, unknown> | null)?.[k], v);
const textAt = (v: unknown, pointer: string): string => { const t = at(v, pointer); if (typeof t !== "string") throw new CloudError("reply"); return t; };
const PLAIN = /^[A-Za-z0-9-]+$/;

/** The language value to send, in the provider's format; undefined = leave it out. */
function langFor(p: CloudProvider, language?: string): string | undefined {
  const l = p.spec.lang, v = language?.trim();
  const chosen = v && v !== "auto" && PLAIN.test(v) ? v : undefined;
  const base = chosen?.split("-")[0]!.toLowerCase();
  const value = chosen && (l.format === "locale" ? (chosen.includes("-") ? chosen : (registry.locales as Record<string, string>)[base!])
    : l.format === "indic" ? (chosen.includes("-") ? chosen : `${base}-IN`)
    : l.format === "name" ? (registry.language_names as Record<string, string>)[base!]
    : base); // ISO 639-1: "en-US" -> "en"
  return value || l.auto;
}

type Fetch = (url: string, init: RequestInit) => Promise<Response>;
/** `timeoutMs` overrides the per-request deadline (otherwise sized to the audio). */
export interface CloudOptions { model?: string; region?: string; language?: string; signal?: AbortSignal; fetchFn?: Fetch; timeoutMs?: number; sleep?: (ms: number) => Promise<void> }

/**
 * Send one recording. Longer than the provider's per-request cap: split at quiet points, sent in order, texts joined.
 * Throws CloudError; the caller falls back to the on-device text. `signal` cancels in-flight requests, waits and polls.
 */
export async function transcribeCloud(p: CloudProvider, key: string, pcm: Float32Array, o: CloudOptions = {}): Promise<string> {
  const texts: string[] = [];
  for (const [a, b] of splitAudio(pcm, p.maxSeconds * 16000)) texts.push((await transcribeOne(p, key, pcm.subarray(a, b), o)).trim());
  const text = texts.filter(Boolean).join(" ");
  if (!text) throw new CloudError("no-text");
  return text;
}

async function transcribeOne(p: CloudProvider, key: string, pcm: Float32Array, o: CloudOptions): Promise<string> {
  const s = p.spec, model = o.model || p.defaultModel, region = (o.region ?? "").trim(), lang = langFor(p, o.language);
  const fetchFn = o.fetchFn ?? fetch, sleep = o.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  let timeoutMs = o.timeoutMs ?? timeoutFor(pcm.length);
  const cancelled = () => o.signal?.aborted === true;
  const url = (template: string, id = "", file = "") => {
    let u = template.replaceAll("{model}", model).replaceAll("{id}", encodeURIComponent(id)).replaceAll("{file}", encodeURIComponent(file));
    if (p.needsRegion) {
      const host = azureHost(region, p.host);
      if (!host) throw new CloudError("region");
      u = u.replace(p.host, host);
    }
    return u;
  };
  const withQuery = (u: string, q: [string, string][]) => q.reduce((acc, [k, v], i) => `${acc}${i === 0 && !u.includes("?") ? "?" : "&"}${k}=${encodeURIComponent(v)}`, u);
  /** Waits `ms`; false = cancelled meanwhile. */
  const pause = async (ms: number) => { if (cancelled()) return false; await sleep(ms); return !cancelled(); };
  /** One request, retried once on a rate limit, server error or dropped connection (not on a timeout or a rejected key). */
  const call = async (method: string, u: string, body?: BodyInit, contentType?: string): Promise<unknown> => {
    const headers: Record<string, string> = { [s.auth[0]!]: `${s.auth[1]}${key}`, ...s.headers };
    if (contentType) headers["Content-Type"] = contentType;
    for (let retried = false; ; retried = true) {
      if (method !== "DELETE" && cancelled()) throw new CloudError("cancelled");
      const deadline = AbortSignal.timeout(timeoutMs);
      const signal = method === "DELETE" || !o.signal ? deadline : AbortSignal.any([o.signal, deadline]);
      let kind: CloudErrorKind, status: number | undefined, wait: number | undefined;
      try {
        const res = await fetchFn(u, { method, headers, body, signal });
        if (res.ok) {
          if (method === "DELETE") return null;
          const text = await res.text().catch(() => { throw new CloudError(cancelled() ? "cancelled" : "network"); });
          if (!text.trim()) return null;
          try { return JSON.parse(text); } catch { throw new CloudError("reply"); }
        }
        status = res.status;
        kind = mapStatus(res.status, await res.text().catch(() => ""));
        wait = retryAfterMs(kind, res.headers.get("retry-after"));
      } catch (e) {
        if (e instanceof CloudError) throw e;
        if (cancelled()) throw new CloudError("cancelled");
        kind = "network";
        wait = deadline.aborted ? undefined : retryAfterMs("network", null); // a timeout is not retried
      }
      if (wait === undefined || retried || method === "DELETE") throw new CloudError(kind, status);
      if (!(await pause(wait))) throw new CloudError("cancelled");
    }
  };
  const wav = new Blob([encodeWav(pcm) as BlobPart], { type: "audio/wav" });
  const sortedQuery = () => Object.entries(s.query ?? {}).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => [k, v.replaceAll("{model}", model)] as [string, string]);

  switch (s.style) {
    case "multipart": {
      const f = new FormData();
      if (s.model_field) f.append(s.model_field, model);
      for (const [k, v] of Object.entries(s.fields ?? {})) f.append(k, v);
      if (lang && s.lang.field) f.append(s.lang.field, lang);
      if (s.json_field) f.append(s.json_field.name, (!lang && s.json_field.value_auto ? s.json_field.value_auto : s.json_field.value.replaceAll("{lang}", lang ?? "")).replaceAll("{model}", model));
      f.append(s.file_field!, wav, "audio.wav");
      return textAt(await call("POST", withQuery(url(s.url!), sortedQuery()), f), s.text).trim();
    }
    case "raw": {
      const q = sortedQuery();
      if (lang && s.lang.field) q.push([s.lang.field, lang]);
      return textAt(await call("POST", withQuery(url(s.url!), q), wav, "audio/wav"), s.text).trim();
    }
    case "json_audio": {
      const data = await base64(wav);
      const prompt = registry.prompt + (lang ? ` The speech is in the language with code ${lang}.` : "");
      const body = s.body === "gemini"
        ? { contents: [{ role: "user", parts: [{ text: prompt }, { inlineData: { mimeType: "audio/wav", data } }] }], generationConfig: s.generation_config ?? {} }
        : { model, temperature: 0, messages: [{ role: "user", content: [{ type: "text", text: prompt }, { type: "input_audio", input_audio: { data, format: "wav" } }] }] };
      return transcriptOnly(textAt(await call("POST", url(s.url!), JSON.stringify(body), "application/json"), s.text));
    }
    case "upload_poll": {
      const up = s.upload!, create = s.create!, status = s.status!;
      let uploadBody: BodyInit = wav;
      if (up.body !== "raw") { const f = new FormData(); f.append("file", wav, "audio.wav"); uploadBody = f; }
      const file = textAt(await call("POST", url(up.url), uploadBody, up.body === "raw" ? "application/octet-stream" : undefined), up.id);
      let job = "";
      try {
        const models = create.model_fallback && create.model_fallback !== model ? [model, create.model_fallback] : [model];
        const body: Record<string, unknown> = { [create.ref_field]: file, [create.model_field]: create.model_list ? models : model };
        if (lang && s.lang.field) body[s.lang.field] = s.lang.list ? [lang] : lang;
        job = textAt(await call("POST", url(create.url), JSON.stringify(body), "application/json"), create.id);
        const started = Date.now(), limit = timeoutMs;
        for (;;) {
          const v = await call("GET", url(status.url, job, file));
          const state = textAt(v, status.field);
          if (state === status.done) return textAt(s.result ? await call("GET", url(s.result, job, file)) : v, s.text).trim();
          if (state === status.failed) throw new CloudError("audio");
          if (Date.now() - started > limit) throw new CloudError("network");
          if (!(await pause(400))) throw new CloudError("cancelled");
        }
      } finally {
        // Best effort: the provider keeps nothing once the text is back (or the job failed).
        timeoutMs = timeoutFor(0);
        for (const c of (s.cleanup ?? []).filter((c) => job || !c.includes("{id}"))) await call("DELETE", url(c, job, file)).catch(() => {});
      }
    }
  }
  throw new CloudError("reply");
}

/* ---------- live (streaming) ---------- */

const collapse = (s: string) => s.split(/\s+/).filter(Boolean).join(" ");
const matches = (v: unknown, when: Record<string, unknown>) =>
  Object.entries(when).every(([ptr, want]) => { const got = at(v, ptr); return got != null && (want === "*" || JSON.stringify(got) === JSON.stringify(want)); });

/** What one streamed dictation has heard so far. Same rules as desktop (LiveState) and Android. */
export class LiveState {
  finals = ""; partial = ""; ready: boolean; endSent = false; done = false; error?: string;
  constructor(private live: LiveSpec) { this.ready = !live.ready; }
  private append(t: string) { if (this.finals && t) this.finals += this.live.join ?? ""; this.finals += t; }
  /** One server message; true = the text shown changed. */
  on(msg: string): boolean {
    let v: unknown;
    try { v = JSON.parse(msg); } catch { return false; }
    const before = this.text(), live = this.live;
    if (live.ready && matches(v, live.ready)) this.ready = true;
    const rule = live.messages.find((r) => matches(v, r.match));
    if (!rule) return false;
    const text = () => { const t = rule.text ? at(v, rule.text) : undefined; return typeof t === "string" ? t : t == null ? "" : JSON.stringify(t); };
    switch (rule.do) {
      case "partial": this.partial = text(); break;
      case "delta": this.partial += text(); break;
      case "final": this.append(text()); this.partial = ""; if (this.endSent && live.end_done === "final") this.done = true; break;
      case "tokens": {
        let partial = "";
        for (const t of (at(v, rule.text ?? "") as { text?: string; is_final?: boolean }[] | undefined) ?? []) {
          const s = t.text ?? "";
          if (s.startsWith("<") && s.endsWith(">")) continue; // <fin>, <end> markers
          if (t.is_final) this.finals += s; else partial += s;
        }
        this.partial = partial;
        break;
      }
      case "result": this.finals = text(); this.partial = ""; this.done = true; break;
      case "done": this.done = true; break;
      case "error": this.error = rule.text ? text() : msg; break;
    }
    return this.text() !== before;
  }
  text(): string { return collapse(this.finals + (this.finals && this.partial ? this.live.join ?? "" : "") + this.partial); }
}

/** A live stream's error message to the error the user sees. */
export function classifyLive(msg: string): CloudErrorKind {
  const m = msg.toLowerCase();
  if (mapStatus(400, msg) === "auth" || ["authenticat", "not_authorised", "not authorized", "invalid token", "1008"].some((w) => m.includes(w))) return "auth";
  if (["quota", "credit", "balance", "insufficient", "payment"].some((w) => m.includes(w))) return "quota";
  if (m.includes("rate")) return "rate";
  return "reply";
}

/** 16 kHz to `rate` (24 kHz for OpenAI) by linear interpolation. */
export function resampleFrom16k(pcm: Float32Array, rate: number): Float32Array {
  if (rate === 16000 || !pcm.length) return pcm;
  const out = new Float32Array(Math.floor(pcm.length * rate / 16000));
  for (let i = 0; i < out.length; i++) { const x = i * 16000 / rate, a = Math.floor(x), f = x - a; out[i] = pcm[a]! * (1 - f) + pcm[Math.min(a + 1, pcm.length - 1)]! * f; }
  return out;
}

export interface LiveOptions {
  model?: string; region?: string; language?: string; signal?: AbortSignal; onText?: (text: string) => void;
  WebSocketImpl?: typeof WebSocket; fetchFn?: Fetch;
}
export interface LiveCloud { feed(pcm16k: Float32Array): void; finish(): Promise<string>; cancel(): void }

/**
 * Streams audio to the provider while the user speaks (browser path: a short-lived token, never the key, in the URL or
 * subprotocol). `feed` 16 kHz chunks; `finish` sends the end message and resolves with the final text (rejects with
 * CloudError: the caller keeps the on-device text). Holds at most ~2 s of audio before the provider is ready.
 */
/** The live model for the chosen (batch) model: its mapped counterpart, the same id when it is also a live model, else the live default. Same as desktop/Android. */
export function liveModelFor(p: CloudProvider, model: string): string {
  const live = p.live!;
  return (live as { map?: Record<string, string> }).map?.[model] ?? (live.models.includes(model) ? model : live.default_model);
}
export function startLiveCloud(p: CloudProvider, key: string, o: LiveOptions = {}): LiveCloud {
  const live = p.live, browser = live?.browser;
  if (!live || !browser) throw new CloudError("reply");
  const WS = o.WebSocketImpl ?? WebSocket, fetchFn = o.fetchFn ?? fetch;
  const model = liveModelFor(p, o.model ?? "");
  const v = o.language?.trim(), chosen = v && v !== "auto" && PLAIN.test(v) ? v.split("-")[0]!.toLowerCase() : undefined;
  const lang = live.lang ? chosen ?? live.lang.auto : undefined;
  const start = (!lang && live.start_auto ? live.start_auto : (live.start ?? "").replaceAll("{lang}", lang ?? "")).replaceAll("{model}", model);
  const rate = live.rate ?? 16000, frame = rate / 10; // 100 ms
  const st = new LiveState(live);
  let ws: WebSocket | undefined, failed: CloudError | undefined, stopped = false, closed = false, seq = 0;
  let held: Float32Array[] = [], pending: number[] = [], lastMsg = Date.now(), endAt = 0;
  const t0 = Date.now();
  let wake = () => {};
  const fail = (e: CloudError) => { failed ??= e; try { ws?.close(); } catch { /* already closed */ } wake(); };
  const pump = () => {
    if (!ws || ws.readyState !== 1 || !st.ready || failed) { if (held.length > 20) held = held.slice(-20); return; }
    for (const c of held) for (const x of resampleFrom16k(c, rate)) { const s = Math.max(-1, Math.min(1, x)); pending.push(s < 0 ? s * 0x8000 : s * 0x7fff); }
    held = [];
    while (pending.length >= frame || (stopped && pending.length)) {
      const bytes = new Uint8Array(Int16Array.from(pending.splice(0, frame)).buffer);
      if (live.audio) { let bin = ""; for (const b of bytes) bin += String.fromCharCode(b); ws.send(live.audio.replace("{audio}", btoa(bin))); }
      else ws.send(bytes);
      seq++;
    }
    if (stopped && !st.endSent) {
      if (live.end != null) ws.send(live.end.replaceAll("{seq}", String(seq))); else ws.close();
      st.endSent = true; endAt = Date.now();
    }
  };
  o.signal?.addEventListener("abort", () => fail(new CloudError("cancelled")));
  void (async () => {
    // the short-lived token: the long-lived key goes only in this request's header
    const res = await fetchFn(browser.token.url, {
      method: "POST", headers: { [p.spec.auth[0]!]: `${p.spec.auth[1]}${key}`, ...p.spec.headers, "Content-Type": "application/json" },
      body: (browser.token.body ?? "").replaceAll("{model}", model), signal: o.signal,
    });
    const body = await res.text();
    if (!res.ok) throw new CloudError(mapStatus(res.status, body), res.status);
    let token: string;
    try { token = textAt(JSON.parse(body), browser.token.text); } catch { throw new CloudError("reply"); }
    if (failed) return;
    const q: [string, string][] = Object.entries(live.query ?? {}).map(([k, x]) => [k, x.replaceAll("{model}", model)]);
    if (lang && live.lang?.field) q.push([live.lang.field, lang]);
    if (browser.query) q.push([browser.query, token]);
    const url = q.reduce((u, [k, x], i) => `${u}${i === 0 && !live.url.includes("?") ? "?" : "&"}${k}=${encodeURIComponent(x)}`, live.url);
    const sock = new WS(url, browser.protocols?.map((s) => s.replaceAll("{token}", token)));
    ws = sock;
    sock.binaryType = "arraybuffer";
    sock.onopen = () => { if (start) sock.send(start); pump(); };
    sock.onmessage = (e: MessageEvent) => {
      if (typeof e.data !== "string") return;
      lastMsg = Date.now();
      if (st.on(e.data)) o.onText?.(st.text());
      if (st.error) return fail(new CloudError(classifyLive(st.error)));
      pump();
      if (st.done) wake();
    };
    sock.onclose = (e: CloseEvent) => { closed = true; if (!st.endSent) fail(new CloudError(classifyLive(`closed ${e.code} ${e.reason}`))); wake(); };
    sock.onerror = () => { if (!st.endSent) fail(new CloudError("network")); };
  })().catch((e) => fail(e instanceof CloudError ? e : new CloudError(o.signal?.aborted ? "cancelled" : "network")));
  return {
    feed(pcm) { if (stopped || failed) return; held.push(pcm); pump(); },
    cancel() { fail(new CloudError("cancelled")); },
    async finish() {
      stopped = true;
      pump();
      for (;;) {
        if (failed) throw failed;
        const now = Date.now();
        if (st.done || closed) break;
        if (endAt ? now - endAt > 10000 || (now - endAt > 2000 && now - lastMsg > 2000) : now - t0 > 15000) { if (!endAt) fail(new CloudError("network")); break; }
        await new Promise<void>((r) => { wake = r; setTimeout(r, 200); });
        pump();
      }
      if (failed) throw failed;
      try { ws?.close(); } catch { /* already closed */ }
      const text = st.text();
      if (!text) throw new CloudError("no-text");
      return text;
    },
  };
}

async function base64(b: Blob): Promise<string> {
  const bytes = new Uint8Array(await b.arrayBuffer());
  let bin = "";
  for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(bin);
}

/** "Test key": one second of silence. A key the provider accepts but audio it rejects (400/422, or no text) still counts as valid. */
export async function testKey(p: CloudProvider, key: string, o: CloudOptions = {}): Promise<{ ok: true } | { ok: false; error: CloudErrorKind }> {
  try { await transcribeCloud(p, key, new Float32Array(16000), { timeoutMs: 15000, ...o }); return { ok: true }; }
  catch (e) { const k = (e as CloudError).kind ?? "network"; return k === "audio" || k === "reply" || k === "no-text" ? { ok: true } : { ok: false, error: k }; }
}
