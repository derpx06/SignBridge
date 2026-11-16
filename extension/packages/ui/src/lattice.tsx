import { useEffect, useRef } from "react";
import { comb, hexPath, TICK, CREAM_STOPS, CORE_STOPS } from "./mark";

/* The Voicio honeycomb v2 (Honeycomb2.dc.html) on the ink capsule: 37 rounded hexagons in rings 0..3, cream bevel cells and
   an orange core with a soft inner glow. Calm motion: a state change reaches ring 0 first and ring 3 last (60 ms apart), and
   every cell eases to its target in about 200 ms with no overshoot; nothing flashes. The noise gate is ported from the
   desktop pill: the room's loudness is tracked as a floor, only sound clearly above it lifts the core, and the floor shows
   as a slate outer ring ("heard, ignored"). */
export type LatticeMode = "idle" | "listening" | "finishing" | "done" | "error";

const PX = 32; // css px; the design's capsule comb is 32 px with the comb 92 units of a 100-unit box
const { s: S, cells } = comb(92);
const UNIT = new Path2D(hexPath(0, 0, 1)); // drawn scaled per cell
type RGB = [number, number, number];
const hex = (h: string): RGB => [parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16)];
const mixc = (a: RGB, b: RGB, t: number): RGB => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
/** A flat colour still gets the bevel: lighter top-left, darker bottom-right. */
// At 32 px the capsule uses the small-size variant: flatter cells (a gentle bevel), so the comb reads instead of turning to mush.
const bevel = (h: string): [RGB, RGB, RGB] => { const c = hex(h); return [mixc(c, [255, 255, 255], 0.2), c, mixc(c, [40, 20, 0], 0.12)]; };
const flat = (st: readonly string[]): [RGB, RGB, RGB] => { const [a, b, c] = st.map(hex) as [RGB, RGB, RGB]; return [mixc(b, a, 0.6), b, mixc(b, c, 0.45)]; };
const CREAM = flat(CREAM_STOPS), CORE = flat(CORE_STOPS);
const AMBER = bevel("#FFC24B"), LIME = bevel("#B8F06A"), CORAL = bevel("#FF6B5B"), SLATE = bevel("#9FB0C2");
const rgb = (c: RGB) => `rgb(${c[0] | 0},${c[1] | 0},${c[2] | 0})`;
interface Target { k: number; op: number; st: [RGB, RGB, RGB] }
// Rest at capsule size: cream rings stay at 0.55 or more and the orange core is full, so the comb is legible on ink.
const REST_OP = [1, 0.8, 0.66, 0.55], REST_K = [1.2, 0.95, 0.92, 0.9];


/** `level` (or a `levelRef` that is written outside React) is the newest loudness 0..1; it is read every frame, so updating it never needs a re-render. */
export function Lattice({ mode, level = 0, levelRef, frozen, size = PX }: { mode: LatticeMode; level?: number; levelRef?: { current: number }; frozen?: boolean; size?: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const live = useRef({ mode, level });
  live.current = { mode, level };
  const lref = useRef(levelRef);
  lref.current = levelRef;
  // Eased per-cell state survives mode changes, so a new state ripples in instead of snapping.
  const cur = useRef<{ k: number; op: number; st: [RGB, RGB, RGB] }[] | null>(null);
  const modeAt = useRef({ mode, prev: mode, t: 0 });

  useEffect(() => {
    const cv = ref.current!, g = cv.getContext("2d");
    if (!g) return;
    const dpr = Math.min(3, window.devicePixelRatio || 1);
    cv.width = size * dpr; cv.height = size * dpr;
    const reduce = frozen ?? window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    cur.current ??= cells.map((c) => ({ k: REST_K[c.ring]!, op: REST_OP[c.ring]!, st: c.ring === 0 ? CORE : CREAM }));
    let room = 0, noise = 0, vis = 0, phase = 0, last = performance.now(), raf = 0;
    const hist: number[] = Array(8).fill(0); // voice level per 20 ms frame, so ring n follows 60 ms * n later

    const target = (m: LatticeMode, c: (typeof cells)[number], v: number): Target => {
      const ring = c.ring;
      switch (m) {
        case "listening": {
          // rest -> voice by loudness (Honeycomb2: Rest and Voice)
          const restOp = REST_OP[ring]!, voiceOp = [1, 1, 0.92, 0.7][ring]!, voiceK = [1.3, 1.12, 1, 0.92][ring]!, restK = REST_K[ring]!;
          const t: Target = { k: restK + (voiceK - restK) * v, op: restOp + (voiceOp - restOp) * v, st: ring === 0 ? CORE : CREAM };
          const hush = ring === 3 ? Math.min(1, noise * 2.4) * (1 - Math.min(1, v * 2)) : 0; // room noise: slate outer ring
          if (hush > 0.02) { t.st = [mixc(CREAM[0], SLATE[0], hush), mixc(CREAM[1], SLATE[1], hush), mixc(CREAM[2], SLATE[2], hush)]; t.op += (0.85 - t.op) * hush; t.k += (0.92 - t.k) * hush; }
          return t;
        }
        case "finishing": {
          if (ring === 0) return { k: 1, op: 0.9, st: AMBER };
          const w = (Math.cos(c.angle + 0.9 - phase * 2.6) + 1) / 2; // a warm sweep around the edge
          return ring === 3 ? { k: 0.84 + 0.16 * w, op: 0.3 + 0.7 * w, st: AMBER } : { k: 0.9, op: 0.5, st: AMBER };
        }
        case "done": return TICK.has(`${c.q},${c.r}`) ? { k: 1.04, op: 1, st: LIME } : { k: 0.88, op: 0.28, st: CREAM };
        case "error": return { k: 0.92, op: ring >= 2 ? 0.6 : 0.95, st: CORAL };
        default: return { k: REST_K[ring]!, op: REST_OP[ring]!, st: ring === 0 ? CORE : CREAM };
      }
    };

    const frame = (now: number) => {
      const dt = Math.min(0.1, (now - last) / 1000); last = now;
      const m = live.current.mode, lv = lref.current ? lref.current.current : live.current.level;
      if (m !== modeAt.current.mode) modeAt.current = { mode: m, prev: modeAt.current.mode, t: now };
      const k = dt / 0.04; // the gate tuning below is per 40 ms tick
      phase += reduce ? 0 : dt;
      const raw = m === "listening" ? Math.sqrt(Math.max(0, Math.min(1, lv))) : 0; // sqrt: quiet speech still moves
      room += (raw - room) * (raw < room ? 0.25 : 0.006) * Math.min(k, 3);
      const voice = m === "listening" ? Math.max(0, Math.min(1, (raw - room - 0.07) / Math.max(0.3, 0.75 - room))) : 0;
      vis += (voice - vis) * Math.min(1, (voice > vis ? 0.5 : 0.14) * k);
      noise += ((m === "listening" ? room : 0) - noise) * Math.min(1, 0.12 * k);
      hist.unshift(vis); hist.length = 8;
      const ease = reduce ? 1 : 1 - Math.exp(-dt / 0.07); // ~200 ms ease-out, no overshoot
      const since = now - modeAt.current.t;

      g.setTransform(dpr, 0, 0, dpr, 0, 0);
      g.clearRect(0, 0, size, size);
      const scale = size / 100;
      g.translate(size / 2, size / 2); g.scale(scale, scale);
      // The core's inner glow, lifted by the voice.
      const glowA = m === "listening" ? 0.22 + 0.45 * vis : m === "idle" ? 0.18 : 0;
      if (glowA > 0.01) {
        const gl = g.createRadialGradient(0, 0, 0, 0, 0, S * 3.2);
        gl.addColorStop(0, `rgba(255,122,69,${glowA.toFixed(3)})`); gl.addColorStop(1, "rgba(255,122,69,0)");
        g.fillStyle = gl; g.fillRect(-50, -50, 100, 100);
      }
      cells.forEach((cell, i) => {
        // ring n switches to the new state 60 ms after ring n-1, and hears the voice 60 ms later
        const m2 = reduce || since >= cell.ring * 60 ? m : modeAt.current.prev;
        const t = target(m2, cell, hist[Math.min(7, cell.ring * 3)]!);
        const c = cur.current![i]!;
        c.k += (t.k - c.k) * ease; c.op += (t.op - c.op) * ease;
        c.st = [mixc(c.st[0], t.st[0], ease), mixc(c.st[1], t.st[1], ease), mixc(c.st[2], t.st[2], ease)];
        const R = S * 0.82 * c.k;
        g.save();
        g.translate(cell.x, cell.y); g.scale(R, R);
        const gr = g.createLinearGradient(-1, -1, 1, 1);
        gr.addColorStop(0, rgb(c.st[0])); gr.addColorStop(0.5, rgb(c.st[1])); gr.addColorStop(1, rgb(c.st[2]));
        g.globalAlpha = Math.max(0, Math.min(1, c.op));
        g.fillStyle = gr; g.fill(UNIT);
        g.restore();
      });
      g.globalAlpha = 1;
      // Reduced motion: draw the state once (and again when it changes), no loop.
      if (!reduce) raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, [frozen, size, repaintKey(mode)]);

  return <canvas ref={ref} aria-hidden style={{ width: size, height: size, flex: "none", display: "block" }} />;
}
// Under reduced motion the loop does not run, so a mode change must restart the effect to repaint.
function repaintKey(mode: LatticeMode) {
  return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches ? mode : "";
}
