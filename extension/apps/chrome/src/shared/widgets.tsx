import { useEffect, useRef, useState } from "react";
import { UNSETTLED_WORDS, type Snapshot } from "@voicio/core";
import { Button, Card, Chip, Icon, Keycaps, Progress, Spinner, Waveform } from "@voicio/ui";
import { send, useElapsed, useLevels } from "./hooks";
import { byId, formatBytes, progressLine, subtitleOf, totalBytes } from "./models";
import { NO_MIC_BODY, NO_MIC_TITLE, errorCopy, type FixAction } from "./errors";
import type { InsertResult } from "./page";
import { HEARD_DB, startMeter, useMicProbe } from "./mic";

export const fmtTime = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

/** Mic button + waveform + status line + hint. `size` 68 in the popup, 88 in options. */
export function MicStage({ snap, keys, size = 68, compact }: { snap: Snapshot; keys: string[]; size?: number; compact?: boolean }) {
  const listening = snap.rec === "listening";
  const working = snap.rec === "processing";
  const levels = useLevels(40, listening);
  const elapsed = useElapsed(snap.startedAt, listening);
  const area = Math.round(size * 1.36);
  const status = listening ? "Listening…" : working ? "Cleaning up…" : snap.rec === "done" ? "Done" : "Ready";
  const btn = (
    <div className="p-glow" style={{ width: area, height: area, flex: "none" }}>
        {listening && <><span className="p-ring" style={{ inset: (area - size) / 2 }} /><span className="p-ring" style={{ inset: (area - size) / 2, animationDelay: ".9s" }} /></>}
        {working && <span style={{ position: "absolute", inset: (area - size) / 2 - 4, borderRadius: "50%", border: "2px solid var(--line2)", borderTopColor: "var(--ac)", animation: "vk-spin .9s linear infinite" }} />}
        <button className={`p-mic${listening ? " on" : ""}`} style={{ width: size, height: size }} disabled={working}
          aria-label={listening ? "Stop dictation" : "Start dictation"} onClick={() => send({ t: "cmd", cmd: "toggle", from: "popup" })}>
          <Icon name={listening ? "stop" : working ? "wand" : "mic"} size={Math.round(size * 0.4)} />
        </button>
      </div>
  );
  const hint = listening ? <>Press <Keycaps keys={keys} /> again to finish</> : working ? "Removing filler words and fixing punctuation" : <>Click the mic or press <Keycaps keys={keys} /></>;
  const statusLine = (
    <div role="status" style={{ display: "flex", alignItems: "center", gap: 8, font: "700 17px/24px var(--font)", letterSpacing: "-.01em" }}>
      <i className={`v-dot${listening ? " breathe" : ""}`} style={{ color: working ? "#FFC24B" : listening ? "var(--ac)" : "var(--ok)" }} />
      {status}
      {listening && <span className="p-mono" style={{ fontSize: 13, color: "var(--tx2)", fontWeight: 400 }}>{fmtTime(elapsed)}</span>}
    </div>
  );
  if (compact) {
    return (
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 14 }}>
          {btn}
          <div style={{ minWidth: 0, display: "flex", flexDirection: "column", gap: 4 }}>{statusLine}<div className="p-hint" style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}>{hint}</div></div>
        </div>
        <Waveform n={36} h={20} w={4} gap={4} levels={levels} mode={listening ? "live" : working ? "processing" : "idle"} />
      </div>
    );
  }
  return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 10 }}>
      {btn}
      <Waveform n={40} h={26} w={4} gap={4} levels={levels} mode={listening ? "live" : working ? "processing" : "idle"} />
      {statusLine}
      <div className="p-hint" style={{ display: "flex", alignItems: "center", gap: 6 }}>{hint}</div>
    </div>
  );
}

/** Live transcript: unsettled words are dimmed, the box scrolls to the newest words. */
export function Transcript({ snap, tall, fill }: { snap: Snapshot; tall?: boolean; fill?: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  const live = snap.rec === "listening" || snap.rec === "processing";
  const words = live || !snap.final ? snap.partial : [];
  useEffect(() => { ref.current?.scrollTo({ top: 1e6 }); }, [snap.partial.length, snap.final]);
  const empty = !snap.final && words.length === 0;
  return (
    <div style={{ flex: fill ? "1 1 auto" : "none", minHeight: fill ? 84 : undefined, display: "flex", flexDirection: "column", background: "var(--surf)", borderRadius: 20, padding: "12px 16px 14px", boxShadow: "0 18px 40px rgba(60,40,10,.05)" }}>
      <div className="p-row" style={{ justifyContent: "space-between", marginBottom: 4 }}>
        <span className="p-hint" style={{ fontWeight: 500 }}>Transcript</span>
        <span className="p-hint p-mono">{snap.wordCount} words</span>
      </div>
      <div ref={ref} className="p-scroll" aria-live="off" style={{ ...(fill ? { flex: 1, minHeight: 0 } : { height: tall ? 120 : 64 }), overflowY: "auto", font: "400 13.5px/19px var(--font)", color: empty ? "var(--tx3)" : "var(--tx)", WebkitMaskImage: "linear-gradient(to bottom,transparent 0,#000 10px)", maskImage: "linear-gradient(to bottom,transparent 0,#000 10px)" }}>
        {empty && "Your words appear here as you speak."}
        {snap.final && !live && snap.final}
        {words.map((w, i) => <span key={i} style={i >= words.length - UNSETTLED_WORDS && snap.rec === "listening" ? { color: "var(--tx3)" } : undefined}>{w} </span>)}
      </div>
    </div>
  );
}

export function TranscriptActions({ snap }: { snap: Snapshot }) {
  const [copied, setCopied] = useState(false);
  const [note, setNote] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const disabled = !snap.final || snap.rec === "listening" || snap.rec === "processing";
  const insert = async () => {
    setBusy(true); setNote(null);
    const r = (await send({ t: "insert-popup-text" })) as InsertResult | undefined;
    setBusy(false);
    if (r?.ok) return setNote({ ok: true, text: "Inserted into the page." });
    // Whatever the reason, the words must not be lost: put them on the clipboard.
    const copiedNow = await navigator.clipboard.writeText(snap.final).then(() => true, () => false);
    const tail = copiedNow ? " Your text is copied: click a text box on a website and press Ctrl+V." : "";
    const why = r?.reason;
    setNote({ ok: false, text:
      why === "restricted" ? `Chrome doesn't let extensions use this page (the New Tab page and chrome:// pages are off limits). Open a website to dictate into it.${tail}`
      : why === "unreachable" ? `Voicio couldn't reach this page.${tail}`
      : why === "blocked" ? `This page blocked the text.${tail}`
      : "No text box found. Click inside one on the page and press Insert again." + tail });
  };
  return (
    <div style={{ flex: "none", display: "flex", flexDirection: "column", gap: 8 }}>
      <Button kind="primary" large block icon="enter" disabled={disabled} loading={busy} onClick={() => void insert()}>Insert into page</Button>
      <div className="p-row" style={{ gap: 8 }}>
        <Button block icon={copied ? "check" : "copy"} disabled={disabled}
          onClick={() => { void navigator.clipboard.writeText(snap.final).then(() => { setNote(null); setCopied(true); setTimeout(() => setCopied(false), 1500); }, () => setNote({ ok: false, text: "Chrome wouldn't copy it. Select the text in the transcript and press Ctrl+C." })); }}>
          {copied ? "Copied" : "Copy"}
        </Button>
        <Button block icon="eraser" disabled={disabled} onClick={() => { setNote(null); void send({ t: "clear" }); }}>Clear</Button>
      </div>
      {note && <span className="p-hint" role="status" style={{ color: note.ok ? "var(--ok-tx)" : "var(--warn)" }}>{note.text}</span>}
    </div>
  );
}

export function ModelPanel({ snap, compact, id }: { snap: Snapshot; compact?: boolean; id?: string }) {
  const entry = byId(id ?? snap.activeModel);
  const m = id ? snap.models[id] ?? snap.model : snap.model;
  const op = (o: "install" | "pause" | "cancel" | "remove") => send({ t: "model", op: o, id: entry.id });
  const frac = m.total ? m.bytes / m.total : 0;
  if (m.state === "downloading" || m.state === "paused") {
    return (
      <div style={{ width: "100%", display: "flex", flexDirection: "column", gap: 8 }}>
        <Progress label="Model download" large value={frac} state={m.state === "paused" ? "paused" : undefined} />
        <div className="p-hint p-mono" style={{ textAlign: "center" }}>{m.state === "paused" ? `Paused · ${formatBytes(m.bytes)} of ${formatBytes(m.total)}` : progressLine(m.bytes, m.total, m.speed)}</div>
        <div className="p-row" style={{ justifyContent: "center", gap: 8 }}>
          {m.state === "paused" ? <Button kind="primary" icon="play" onClick={() => op("install")}>Resume</Button> : <Button icon="pause" onClick={() => op("pause")}>Pause</Button>}
          <Button kind="ghost" onClick={() => op("cancel")}>Cancel</Button>
        </div>
      </div>
    );
  }
  return (
    <div style={{ width: "100%", display: "flex", flexDirection: "column", alignItems: "center", gap: 8 }}>
      {m.state === "failed" && (
        <Card style={{ background: "var(--err-soft)", width: "100%", textAlign: "left" }}>
          <div className="p-row" style={{ alignItems: "flex-start" }}>
            <Icon name="alertCircle" size={16} style={{ color: "var(--err)", marginTop: 2 }} />
            <div><div className="p-label">Download failed</div><div className="p-hint" style={{ color: "var(--tx2)" }}>{m.error}</div></div>
          </div>
        </Card>
      )}
      <Button kind="primary" large icon="download" onClick={() => op("install")}>{m.state === "failed" ? "Retry download" : `Download · ${formatBytes(totalBytes(entry))}`}</Button>
      {!compact && <span className="p-hint">{entry.name} · {subtitleOf(entry)}</span>}
    </div>
  );
}

/** Small status chip for the popup and settings header. */
export function StatusChip({ snap }: { snap: Snapshot }) {
  const model = snap.model.state;
  if (snap.rec === "listening") return <Chip kind="ac" dot breathe>Listening</Chip>;
  if (snap.rec === "processing") return <Chip kind="warn" icon="wand">Working</Chip>;
  if (model !== "installed" && model !== "unknown") return <Chip kind="warn" icon="download">No model</Chip>;
  return <Chip kind="line" dot>On-device</Chip>;
}

/** The error a dictation ended with, in words, with the buttons that fix it. Shown in the popup and in settings. */
export function ErrorNotice({ snap }: { snap: Snapshot }) {
  if (snap.rec !== "error") return null;
  const c = errorCopy(snap.error, snap.detail, snap.device);
  const label: Record<FixAction, string> = { retry: "Try again", allow: "Allow microphone", unblock: "Fix it", "choose-mic": "Choose microphone", restart: "Restart Chrome" };
  const run = (a: FixAction) => void (a === "retry" ? send({ t: "cmd", cmd: "start", from: "popup" })
    : send({ t: "open", page: a === "allow" ? "permission" : a === "unblock" ? "permission-blocked" : a === "restart" ? "restart" : "mic-settings" }));
  return (
    <Card style={{ background: "var(--warn-soft)" }}>
      <div role="alert">
        <div className="p-row" style={{ alignItems: "flex-start" }}>
          <Icon name="alertCircle" size={16} style={{ color: "var(--warn)", marginTop: 2 }} />
          <div style={{ flex: 1 }}><div className="p-label">{c.title}</div><div className="p-hint" style={{ color: "var(--tx2)" }}>{c.body}</div></div>
        </div>
        <div className="p-row" style={{ gap: 8, marginTop: 10, flexWrap: "wrap" }}>
          {c.actions.map((a, i) => <Button key={a} kind={i === 0 ? "primary" : "secondary"} onClick={() => run(a)}>{label[a]}</Button>)}
          <Button kind="ghost" onClick={() => void send({ t: "cmd", cmd: "cancel", from: "popup" })}>Dismiss</Button>
        </div>
      </div>
    </Card>
  );
}

/** Choose the microphone, see that it works, and understand why when it does not. */
export function MicPicker({ micId, onChange }: { micId: string; onChange: (id: string) => void }) {
  const probe = useMicProbe();
  const bars = useRef<HTMLElement[]>([]);
  const meter = useRef<{ stop: () => void } | null>(null);
  const [testing, setTesting] = useState(false);
  const [heard, setHeard] = useState<"" | "yes" | "no">("");
  const [err, setErr] = useState("");
  const stop = () => { meter.current?.stop(); meter.current = null; setTesting(false); };
  useEffect(() => stop, []);
  // the device list changed or another microphone was chosen: a running test must follow it
  useEffect(() => { if (testing) { stop(); } }, [micId]); // eslint-disable-line react-hooks/exhaustive-deps

  const test = async () => {
    setErr(""); setHeard("");
    try {
      let hits = 0; const t0 = performance.now(); const hist = Array(bars.current.length).fill(0);
      meter.current = await startMeter(micId, (v, db) => {
        hist.shift(); hist.push(v);
        bars.current.forEach((b, i) => { if (b) b.style.transform = `scaleY(${Math.max(0.08, hist[i] ?? 0)})`; });
        if (db > HEARD_DB && ++hits === 6) setHeard("yes");
        if (performance.now() - t0 > 4000 && hits < 6) setHeard((h) => h || "no");
      });
      setTesting(true);
      probe.refresh(); // labels become readable once the microphone was allowed
    } catch (e) {
      const n = (e as DOMException).name;
      setErr(n === "NotAllowedError" ? "Chrome is blocking the microphone for Voicio." : n === "NotFoundError" || n === "OverconstrainedError" ? "That microphone isn't available." : n === "NotReadableError" ? "Another app is using the microphone." : "The microphone didn't open.");
    }
  };

  const noDevice = probe.ready && probe.permission === "granted" && probe.devices.length === 0;
  const status = probe.permission === "denied" ? ["warn", "Blocked in Chrome"] : probe.permission === "prompt" ? ["neutral", "Not allowed yet"]
    : noDevice ? ["warn", "No microphone detected"] : probe.permission === "granted" ? ["ok", `${probe.devices.length} microphone${probe.devices.length === 1 ? "" : "s"} found`] : ["neutral", "Checking…"];
  const known = micId === "" || probe.devices.some((d) => d.id === micId);
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10, width: "100%" }}>
      <div className="p-row" style={{ gap: 8, flexWrap: "wrap" }}>
        <select aria-label="Microphone" value={known ? micId : ""} onChange={(e) => onChange(e.target.value)}
          className="p-input" style={{ flex: "1 1 220px" }}>
          <option value="">System default</option>
          {probe.devices.filter((d) => d.id !== "default" && d.id !== "communications").map((d) => <option key={d.id} value={d.id}>{d.label}</option>)}
        </select>
        <Chip kind={status[0] as "ok" | "warn" | "neutral"}>{status[1]}</Chip>
      </div>
      {!known && <div className="p-hint" style={{ color: "var(--warn)" }}>The microphone you chose isn't connected. Voicio is using the system default.</div>}
      <div className="p-row" style={{ gap: 12 }}>
        <Button onClick={() => (testing ? stop() : void test())} disabled={probe.permission === "denied"}>{testing ? "Stop test" : "Test microphone"}</Button>
        <div aria-hidden style={{ flex: 1, display: "flex", alignItems: "center", gap: 3, height: 28 }}>
          {Array.from({ length: 24 }, (_, i) => <i key={i} ref={(el) => { if (el) bars.current[i] = el; }} style={{ display: "block", width: 4, height: "100%", borderRadius: 2, background: "var(--ac)", transform: "scaleY(.08)", transition: "transform 60ms linear" }} />)}
        </div>
      </div>
      {testing && <div className="p-hint" role="status">{heard === "yes" ? "I can hear you. This microphone works." : heard === "no" ? "No sound yet. It may be muted, or try another microphone." : "Say something…"}</div>}
      {err && <div className="p-hint" role="alert" style={{ color: "var(--warn)" }}>{err}</div>}
      {noDevice && <div className="p-hint" style={{ color: "var(--warn)" }}>{NO_MIC_TITLE}. {NO_MIC_BODY}<div className="p-row" style={{ gap: 8, marginTop: 8 }}><Button kind="primary" onClick={() => void send({ t: "open", page: "restart" })}>Restart Chrome</Button><Button onClick={() => probe.refresh()}>Check again</Button></div></div>}
      {probe.permission === "denied" && <Button kind="primary" onClick={() => void send({ t: "open", page: "permission-blocked" })}>Fix microphone access</Button>}
      {probe.permission === "prompt" && <Button kind="primary" onClick={() => void send({ t: "open", page: "permission" })}>Allow microphone</Button>}
    </div>
  );
}
