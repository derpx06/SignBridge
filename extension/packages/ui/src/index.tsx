import { useEffect, useId, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { ICONS, type IconName } from "./icons";
import { markSvg } from "./mark";
export { markSvg };

export { Lattice, type LatticeMode } from "./lattice";
export { ICONS };
export type { IconName };

/* ---------- primitives ported from voicio-kit.js ---------- */

export function Icon({ name, size = 16, stroke = 1.75, style }: { name: IconName; size?: number; stroke?: number; style?: CSSProperties }) {
  return (
    <svg
      width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={stroke}
      strokeLinecap="round" strokeLinejoin="round" aria-hidden style={{ flex: "none", display: "block", ...style }}
      dangerouslySetInnerHTML={{ __html: ICONS[name] }}
    />
  );
}

/** The brand mark (an ink tile with the honeycomb). `tile={false}` draws the honeycomb alone. */
export function Logo({ size = 24, tile = true }: { size?: number; tile?: boolean }) {
  return <span aria-hidden style={{ display: "block", flex: "none", width: size, height: size, lineHeight: 0 }} dangerouslySetInnerHTML={{ __html: markSvg(Math.round(size), { tile }) }} />;
}

const rnd = (i: number) => { const x = Math.sin(i * 127.1 + 311.7) * 43758.5453; return x - Math.floor(x); };

export type WaveMode = "live" | "processing" | "idle" | "static";
/** `levels` (0..1, newest last) drives real heights; without it bars use the animated placeholder. */
export function Waveform({ n = 40, h = 48, w = 4, gap = 4, mode = "live", grad = false, seed = 0, levels }: {
  n?: number; h?: number; w?: number; gap?: number; mode?: WaveMode; grad?: boolean; seed?: number; levels?: number[];
}) {
  return (
    <div aria-hidden style={{ display: "flex", alignItems: "center", gap, height: h }}>
      {Array.from({ length: n }, (_, i) => {
        const env = Math.pow(Math.sin((Math.PI * (i + 0.5)) / n), 0.7);
        const v = Math.max(0.14, env * (0.35 + 0.65 * rnd(i + seed)));
        const pc = Math.round((Math.abs(i - (n - 1) / 2) / ((n - 1) / 2)) * 100);
        const s: CSSProperties = {
          width: w, borderRadius: w, flex: "none", transformOrigin: "center",
          background: grad ? `color-mix(in oklch, var(--ac) ${100 - pc}%, var(--ac2) ${pc}%)` : "var(--ac)",
        };
        if (mode === "idle") { s.height = w; s.background = "var(--tx3)"; s.opacity = 0.6; }
        else if (mode === "processing") { s.height = Math.max(w, h * v * 0.45); s.animation = `vk-shimmer 1.2s ease-in-out ${((i / n) * 1.2).toFixed(2)}s infinite`; }
        else if (mode === "static") s.height = Math.max(w, h * v);
        else if (levels) {
          const l = levels[levels.length - n + i] ?? 0;
          s.height = Math.max(w, h * Math.min(1, 0.12 + l * env * 1.6));
          s.transition = "height .08s linear";
        } else {
          s.height = Math.max(w, h * v);
          s.animation = `vk-wave ${(0.42 + rnd(i * 3 + 7) * 0.5).toFixed(2)}s ease-in-out ${(-rnd(i * 5 + 1)).toFixed(2)}s infinite alternate`;
        }
        return <span key={i} style={s} />;
      })}
    </div>
  );
}

/** Segmented level meter. `level` is 0..1. */
export function LevelMeter({ level, n = 24, h = 8 }: { level: number; n?: number; h?: number }) {
  const row = (fill: (i: number) => string, extra?: CSSProperties) => (
    <div style={{ display: "flex", gap: 3, ...extra }}>
      {Array.from({ length: n }, (_, i) => <span key={i} style={{ flex: 1, height: h, borderRadius: 2, background: fill(i) }} />)}
    </div>
  );
  const clip = `inset(0 ${Math.round((1 - Math.min(1, Math.max(0, level))) * 100)}% 0 0)`;
  return (
    <div role="meter" aria-label="Microphone level" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(level * 100)} style={{ position: "relative", width: "100%" }}>
      {row(() => "var(--line2)")}
      {row((i) => (i >= n * 0.84 ? "var(--warn)" : "var(--ac)"), { position: "absolute", inset: 0, clipPath: clip, transition: "clip-path .08s linear" })}
    </div>
  );
}

export function Spinner({ size = 14 }: { size?: number }) {
  return <span className="v-spin" style={{ width: size, height: size }} />;
}

/* ---------- controls ---------- */

type BtnKind = "primary" | "secondary" | "ghost" | "danger";
export function Button({ kind = "secondary", icon, large, block, loading, children, ...rest }: {
  kind?: BtnKind; icon?: IconName; large?: boolean; block?: boolean; loading?: boolean; children?: ReactNode;
} & React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button {...rest} disabled={rest.disabled || loading} className={`v-btn ${kind}${large ? " lg" : ""}${block ? " block" : ""} ${rest.className ?? ""}`}>
      {loading ? <Spinner /> : icon && <Icon name={icon} size={14} />}
      {children}
    </button>
  );
}

export function IconButton({ icon, label, destructive, large, ...rest }: {
  icon: IconName; label: string; destructive?: boolean; large?: boolean;
} & React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button {...rest} aria-label={label} title={label} className={`v-ib${destructive ? " destructive" : ""}${large ? " lg" : ""}`}>
      <Icon name={icon} size={large ? 16 : 14} />
    </button>
  );
}

export function Segmented<T extends string>({ value, options, onChange, label, block }: {
  value: T; options: { value: T; label: string }[]; onChange: (v: T) => void; label: string; block?: boolean;
}) {
  const move = (e: React.KeyboardEvent, i: number) => {
    const d = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const next = options[(i + d + options.length) % options.length]!;
    onChange(next.value);
    (e.currentTarget.parentElement?.children[(i + d + options.length) % options.length] as HTMLElement | undefined)?.focus();
  };
  return (
    <div role="radiogroup" aria-label={label} className={`v-seg${block ? " block" : ""}`}>
      {options.map((o, i) => (
        <button key={o.value} role="radio" aria-checked={o.value === value} tabIndex={o.value === value ? 0 : -1} onClick={() => onChange(o.value)} onKeyDown={(e) => move(e, i)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Switch({ checked, onChange, label, disabled }: { checked: boolean; onChange: (v: boolean) => void; label: string; disabled?: boolean }) {
  return <button role="switch" aria-checked={checked} aria-label={label} disabled={disabled} className="v-switch" onClick={() => onChange(!checked)} />;
}

export function Keycaps({ keys, large }: { keys: string[]; large?: boolean }) {
  return (
    <span className={`v-keys${large ? " lg" : ""}`}>
      {keys.map((k, i) => (
        <span key={i} style={{ display: "contents" }}>
          {i > 0 && <span>+</span>}
          <kbd className="v-key">{k}</kbd>
        </span>
      ))}
    </span>
  );
}

export function Chip({ kind = "neutral", icon, dot, breathe, children }: { kind?: "neutral" | "ok" | "warn" | "err" | "line" | "ink" | "sky" | "ac"; icon?: IconName; dot?: boolean; breathe?: boolean; children: ReactNode }) {
  return (
    <span className={`v-chip ${kind === "neutral" ? "" : kind}`}>
      {dot && <i className={`v-dot${breathe ? " breathe" : ""}`} />}
      {icon && <Icon name={icon} size={12} />}
      {children}
    </span>
  );
}

export function Progress({ value, state, large, label }: { value: number; state?: "paused" | "error"; large?: boolean; label: string }) {
  const pct = Math.round(Math.min(1, Math.max(0, value)) * 100);
  return (
    <div role="progressbar" aria-label={label} aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100} className={`v-progress${large ? " lg" : ""} ${state ?? ""}`}>
      <i style={{ width: `${pct}%` }} />
    </div>
  );
}

export function Card({ accent, children, style }: { accent?: boolean; children: ReactNode; style?: CSSProperties }) {
  return <div className={`v-card${accent ? " accent" : ""}`} style={style}>{children}</div>;
}

export function Toast({ children, error }: { children: ReactNode; error?: boolean }) {
  return (
    <div role="status" className={`v-toast${error ? " error" : ""}`}>
      <span className={error ? "" : "ok"} style={{ display: "flex" }}><Icon name={error ? "alertCircle" : "check"} size={14} /></span>
      {children}
    </div>
  );
}

/** Shows after 400 ms hover or on focus; Esc hides it. */
export function Tooltip({ text, hint, children }: { text: string; hint?: string; children: ReactNode }) {
  const [show, setShow] = useState(false);
  const timer = useRef<number>(undefined);
  const id = useId();
  useEffect(() => () => clearTimeout(timer.current), []);
  return (
    <span className={`v-tip${show ? " show" : ""}`} onMouseEnter={() => { timer.current = window.setTimeout(() => setShow(true), 400); }}
      onMouseLeave={() => { clearTimeout(timer.current); setShow(false); }} onFocus={() => setShow(true)} onBlur={() => setShow(false)}
      onKeyDown={(e) => e.key === "Escape" && setShow(false)} aria-describedby={id}>
      {children}
      <span className="tip" role="tooltip" id={id}>{text}{hint && <kbd>{hint}</kbd>}</span>
    </span>
  );
}

export function EmptyState({ icon, title, body, children }: { icon: IconName; title: string; body: string; children?: ReactNode }) {
  return (
    <div className="v-empty">
      <div className="badge"><Icon name={icon} size={30} /></div>
      <h3>{title}</h3>
      <p>{body}</p>
      {children}
    </div>
  );
}

/** Applies the theme preference to <html data-theme>. "system" removes the attribute. */
export function applyTheme(theme: "system" | "light" | "dark") {
  if (theme === "system") document.documentElement.removeAttribute("data-theme");
  else document.documentElement.setAttribute("data-theme", theme);
}
