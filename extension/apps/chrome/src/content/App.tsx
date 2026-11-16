import { useEffect, useLayoutEffect, useReducer, useRef, useState, type KeyboardEvent as RKE, type PointerEvent as RPE } from "react";
import { UNSETTLED_WORDS, type Settings, type Snapshot, type ToBackground } from "@voicio/core";
import { Button, Icon, Lattice, Tooltip } from "@voicio/ui";
import { fieldRect, type Field } from "./fields";
import { errorCopy, type FixAction } from "../shared/errors";
import { patchSettings } from "../shared/storage";

export interface Bridge {
  get(): { snap: Snapshot | null; field: Field | null; settings: Settings };
  listeners: Set<() => void>;
  notify(): void;
  send(m: ToBackground): void;
  level?: (v: number) => void;
}

const fmt = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
const PILL_W = 330, PILL_H = 40, CARD_W = 330, STACK_H = PILL_H + 8 + 110; // the stack reserves room for an error card, so the capsule never jumps
// Keep mousedown from moving focus out of the field.
const keepFocus = (e: { preventDefault(): void }) => e.preventDefault();

export function App({ bridge }: { bridge: Bridge }) {
  const [, force] = useReducer((n: number) => n + 1, 0);
  const level = useRef(0); // read by the lattice every frame; no re-render per audio chunk
  const [hover, setHover] = useState<Field | null>(null);
  const drag = useRef<{ dx: number; dy: number; moved: boolean } | null>(null);
  const saved = bridge.get().settings.pillOffset; // where the user last left the capsule (synced setting)
  const [offset, setOffsetRaw] = useState(saved);
  // Never let the offset run far past the window: the position is clamped on screen, but a huge offset would stick to the edge.
  const clamp = (o: { x: number; y: number }) => ({ x: Math.max(-innerWidth, Math.min(innerWidth, o.x)), y: Math.max(-innerHeight, Math.min(innerHeight, o.y)) });
  const setOffset = (o: { x: number; y: number }) => setOffsetRaw(clamp(o));
  useEffect(() => { if (!drag.current) setOffsetRaw(saved); }, [saved.x, saved.y]);
  const saveTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  // Debounced: storage.sync allows only a few writes a minute, and a held arrow key repeats fast.
  const remember = (o: { x: number; y: number }) => { clearTimeout(saveTimer.current); saveTimer.current = setTimeout(() => void patchSettings({ pillOffset: { x: Math.round(o.x), y: Math.round(o.y) } }).catch(() => {}), 500); };
  const [shortcut, setShortcut] = useState("");
  useEffect(() => { void chrome.runtime.sendMessage({ t: "shortcut" } satisfies ToBackground).then((r) => typeof r === "string" && setShortcut(r)).catch(() => {}); }, []);

  useEffect(() => {
    const fn = () => force();
    bridge.listeners.add(fn);
    bridge.level = (v) => { level.current = v; };
    return () => { bridge.listeners.delete(fn); bridge.level = undefined; };
  }, [bridge]);

  // The field mic follows the hovered field.
  useEffect(() => {
    const over = (e: MouseEvent) => {
      const t = e.composedPath()[0] as Element | undefined;
      const el = t instanceof HTMLElement ? (t.closest("textarea, input, [contenteditable='true'], [contenteditable=''], [contenteditable='plaintext-only']") as Field | null) : null;
      setHover(el && el.offsetHeight >= 24 ? el : null);
    };
    document.addEventListener("mousemove", over, { passive: true });
    return () => document.removeEventListener("mousemove", over);
  }, []);
  useLayoutEffect(() => { /* positions are computed in render from live rects */ });

  const { snap, field, settings } = bridge.get();
  const rec = snap?.rec ?? "ready";
  const busy = rec !== "ready";
  const page = snap?.into === "page" && busy;
  const never = settings.sites.some((s) => s.host === location.hostname && s.rule === "never");

  const showMic = settings.fieldMic && !never && !busy && hover && hover.isConnected;
  const mic = showMic ? micPos(hover!) : null;

  const anchor = field && field.isConnected && page ? fieldRect(field) : null;
  // Default: a pill docked at the bottom centre, like a dictation bar. It also stands in when no field was found.
  const pos = !page ? null : anchor && settings.pillPos !== "dock" ? pillPos(anchor, settings.pillPos, offset) : dockPos(offset);

  const down = (e: RPE) => {
    if ((e.target as HTMLElement).closest("button")) return;
    drag.current = { dx: e.clientX - offset.x, dy: e.clientY - offset.y, moved: false };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const move = (e: RPE) => { if (drag.current) { drag.current.moved = true; setOffset({ x: e.clientX - drag.current.dx, y: e.clientY - drag.current.dy }); } };
  const up = () => { if (drag.current?.moved) remember(offset); drag.current = null; };
  /** Arrow keys move the focused capsule (Shift for bigger steps), the keyboard version of dragging it. */
  const nudge = (e: RKE) => {
    const step = e.shiftKey ? 32 : 8;
    const d = ({ ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] } as Record<string, [number, number]>)[e.key];
    if (!d || e.target !== e.currentTarget) return;
    e.preventDefault(); e.stopPropagation();
    const next = clamp({ x: offset.x + d[0], y: offset.y + d[1] });
    setOffset(next); remember(next);
  };

  const unsettledFrom = Math.max(0, (snap?.partial.length ?? 0) - UNSETTLED_WORDS);
  const cmd = (c: "cancel" | "start" | "toggle" | "stop") => bridge.send({ t: "cmd", cmd: c, from: "pill" });
  const partial = snap?.partial ?? [];

  return (
    <>
      {mic && (
        <div style={{ position: "fixed", left: mic.x, top: mic.y, pointerEvents: "none" }}>
          <Tooltip text="Dictate" hint={shortcut}>
            <button className="fieldmic" style={{ position: "static" }} aria-label="Dictate into this field" onMouseDown={keepFocus}
              onClick={() => { (hover as HTMLElement).focus({ preventScroll: true }); bridge.send({ t: "cmd", cmd: "start", from: "field" }); }}>
              <Icon name="mic" size={14} />
            </button>
          </Tooltip>
        </div>
      )}

      {pos && snap && (
        <div className={`stack ${pos.vertical} ${pos.side}`} style={{ left: pos.x, top: pos.y }} onMouseDown={keepFocus}>
          {rec === "error" && <ErrorCard snap={snap} cmd={cmd} />}
          {rec === "nomodel" && (
            <div className="card glass" role="alert">
              <h4>Download a speech model first</h4>
              <p className="sub">It's a one-time download and works offline once saved.</p>
              <div className="actions">
                <Button kind="primary" onClick={() => void chrome.runtime.sendMessage({ t: "open", page: "options" } satisfies ToBackground).catch(() => {})}>Open settings</Button>
                <Button kind="ghost" onClick={() => cmd("cancel")}>Not now</Button>
              </div>
            </div>
          )}
          {/* capsule */}
          <div className={`capsule ${rec === "error" || rec === "nomodel" ? "err" : rec}`} role="group" tabIndex={0} aria-label="Voicio dictation. Arrow keys move it." onKeyDown={nudge} onPointerDown={down} onPointerMove={move} onPointerUp={up}>
            <Capsule snap={snap} level={level} unsettledFrom={unsettledFrom} cancel={() => cmd("cancel")} field={field} />
          </div>
          {/* Screen readers hear state changes only, never the live transcript. */}
          <span className="sr" role="status" aria-live="polite">{stateText(snap)}</span>
        </div>
      )}
    </>
  );
}

function stateText(s: Snapshot): string {
  switch (s.rec) {
    case "listening": return "Listening";
    case "processing": return s.phase === "typing" ? "Typing" : "Cleaning up";
    case "done": return `Done, ${s.wordCount} ${s.wordCount === 1 ? "word" : "words"}`;
    case "error": return errorCopy(s.error, s.detail, s.device, location.hostname).title;
    case "nomodel": return "Model not installed";
    default: return "";
  }
}

function Capsule({ snap, level, unsettledFrom, cancel, field }: { snap: Snapshot; level: { current: number }; unsettledFrom: number; cancel: () => void; field: Field | null }) {
  const [copied, setCopied] = useState(false);
  const copy = () => { void navigator.clipboard.writeText(snap.final).then(() => setCopied(true)).catch(() => {}); };
  const undo = () => { (field as HTMLElement | null)?.focus({ preventScroll: true }); document.execCommand("undo"); cancel(); };
  const x = <button className="x" aria-label="Cancel dictation (Esc)" onMouseDown={keepFocus} onClick={cancel}><Icon name="x" size={12} /></button>;
  // The lattice reads the ref each frame; the capsule re-renders on state and transcript changes only.
  const cloud = snap.cloud ? <span className="cloud" title="This dictation is sent to your cloud speech provider">Cloud</span> : null;
  const lat = (mode: "finishing" | "done" | "error") => <Lattice mode={mode} />;
  switch (snap.rec) {
    case "listening":
      return (<>
        <LiveLattice level={level} />
        <span className="words" dir="auto"><span>{snap.partial.length === 0
          ? <span className="dim"><Elapsed startedAt={snap.startedAt} /></span>
          : snap.partial.map((w, i) => <span key={i} className={i >= unsettledFrom ? "dim" : ""}>{w} </span>)}</span></span>{cloud}{x}</>);
    case "processing":
      return (<>{lat("finishing")}<span className="label work">{snap.phase === "typing" ? "Typing…" : "Tidying…"}</span>{cloud}{x}</>);
    case "done":
      return (<>{lat("done")}<span className="label"><b>Typed</b> · {snap.wordCount} {snap.wordCount === 1 ? "word" : "words"}</span>{cloud}
        {snap.final && <><button className="act" onMouseDown={keepFocus} onClick={undo}>Undo</button><button className="act" onMouseDown={keepFocus} onClick={copy}>{copied ? "Copied" : "Copy"}</button></>}</>);
    case "error":
      return (<>{lat("error")}<span className="label err">{errorCopy(snap.error, snap.detail, snap.device, location.hostname).title}</span>{x}</>);
    case "nomodel":
      return (<>{lat("error")}<span className="label err">Model not installed</span>{x}</>);
    default:
      return null;
  }
}

/** The lattice reads the level ref in its own animation frame loop, which only runs while this is mounted (listening). */
function LiveLattice({ level }: { level: { current: number } }) {
  return <Lattice mode="listening" levelRef={level} />;
}

/** Elapsed time from startedAt, ticking once a second only while no words have arrived yet. */
function Elapsed({ startedAt }: { startedAt?: number }) {
  const [, tick] = useReducer((n: number) => n + 1, 0);
  useEffect(() => { const t = setInterval(tick, 1000); return () => clearInterval(t); }, []);
  return <>{fmt(startedAt ? Math.max(0, Math.floor((Date.now() - startedAt) / 1000)) : 0)}</>;
}

function ErrorCard({ snap, cmd }: { snap: Snapshot; cmd: (c: "cancel" | "start") => void }) {
  const c = errorCopy(snap.error, snap.detail, snap.device, location.hostname);
  const open = (page: "permission" | "permission-blocked" | "mic-settings" | "restart") => void chrome.runtime.sendMessage({ t: "open", page } satisfies ToBackground).catch(() => {});
  const label: Record<FixAction, string> = { retry: "Try again", allow: "Allow microphone", unblock: "Fix it", "choose-mic": "Choose microphone", restart: "Restart Chrome" };
  const run = (a: FixAction) => (a === "retry" ? cmd("start") : a === "allow" ? open("permission") : a === "unblock" ? open("permission-blocked") : a === "restart" ? open("restart") : open("mic-settings"));
  return (
    <div className="card glass" role="alert">
      <h4>{c.title}</h4>
      <p className="sub">{c.body}</p>
      <div className="actions">
        {c.actions.map((a, i) => <Button key={a} kind={i === 0 ? "primary" : "secondary"} onClick={() => run(a)}>{label[a]}</Button>)}
        <Button kind="ghost" onClick={() => cmd("cancel")}>Dismiss</Button>
      </div>
    </div>
  );
}

function micPos(el: Field) {
  const r = fieldRect(el);
  return { x: Math.min(window.innerWidth - 34, r.right - 34), y: Math.max(2, Math.min(window.innerHeight - 34, r.bottom - 34)) };
}

/** The part of the page the user can actually see: the visual viewport (pinch-zoom, on-screen keyboard), and the part of the
 * window that is on the monitor the window is on (a window can extend past the screen edge). Screen geometry is only used
 * when the window sits inside one screen's work area; in multi-monitor layouts the screen object describes a single display
 * and would wrongly shrink the box, so it is ignored there. */
function visibleBox() {
  const vv = window.visualViewport;
  const ox = vv?.offsetLeft ?? 0, oy = vv?.offsetTop ?? 0;
  let w = Math.min(window.innerWidth, vv?.width ?? Infinity), h = Math.min(window.innerHeight, vv?.height ?? Infinity);
  const sc = window.screen as Screen & { availLeft?: number; availTop?: number; isExtended?: boolean };
  if (!sc.isExtended) {
    const top = window.screenY + Math.max(0, window.outerHeight - window.innerHeight);
    const left = window.screenX + Math.max(0, (window.outerWidth - window.innerWidth) / 2);
    const roomH = (sc.availTop ?? 0) + sc.availHeight - top, roomW = (sc.availLeft ?? 0) + sc.availWidth - left;
    if (roomH > 120) h = Math.min(h, roomH);
    if (roomW > 200) w = Math.min(w, roomW);
  }
  return { w, h, ox, oy };
}

function dockPos(off: { x: number; y: number }) {
  const { w, h, ox, oy } = visibleBox();
  const total = STACK_H;
  const x = ox + Math.max(8, Math.min(w - CARD_W - 8, (w - CARD_W) / 2 + off.x));
  const y = oy + Math.max(8, Math.min(h - total - 8, h - total - 28 + off.y));
  return { x, y, vertical: "up", side: "mid" } as const;
}

function pillPos(r: DOMRect, where: Settings["pillPos"], off: { x: number; y: number }) {
  const above = where.startsWith("above");
  const right = where.endsWith("right");
  const w = Math.max(CARD_W, PILL_W);
  let x = right ? r.right - w : r.left;
  let y = above ? r.top - 8 - (STACK_H) : r.bottom + 8;
  const vb = visibleBox();
  x = vb.ox + Math.max(8, Math.min(vb.w - w - 8, x - vb.ox + off.x));
  y = vb.oy + Math.max(8, Math.min(vb.h - (STACK_H) - 8, y - vb.oy + off.y));
  // card sits on the far side of the capsule from the field
  return { x, y, vertical: above ? "up" : "down", side: right ? "right" : "left" } as const;
}
