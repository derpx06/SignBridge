/* The Voicio honeycomb v2 (Honeycomb2.dc.html): pointy-top rounded hexagons, axial radius 3 = 37 cells in rings 1/6/12/18,
   cream bevel cells and an orange core. Pure (no DOM), shared by the live lattice (lattice.tsx), the brand mark (Logo) and
   the icon script (apps/chrome/e2e/icons.mjs), so every surface draws the very same shape. */

/** Rounded pointy-top hexagon centred on (cx, cy), corner rounding `rad` (28% of R in the design). */
export function hexPath(cx: number, cy: number, R: number, rad = R * 0.28): string {
  const pts: [number, number][] = [];
  for (let i = 0; i < 6; i++) { const a = (Math.PI / 180) * (60 * i - 90); pts.push([cx + R * Math.cos(a), cy + R * Math.sin(a)]); }
  let d = "";
  const t = rad / R;
  for (let i = 0; i < 6; i++) {
    const p = pts[i]!, prev = pts[(i + 5) % 6]!, next = pts[(i + 1) % 6]!;
    const a = [p[0] + (prev[0] - p[0]) * t, p[1] + (prev[1] - p[1]) * t], b = [p[0] + (next[0] - p[0]) * t, p[1] + (next[1] - p[1]) * t];
    d += `${i === 0 ? "M" : "L"}${a[0]!.toFixed(2)} ${a[1]!.toFixed(2)} Q${p[0].toFixed(2)} ${p[1].toFixed(2)} ${b[0]!.toFixed(2)} ${b[1]!.toFixed(2)} `;
  }
  return d + "Z";
}

export interface Cell { q: number; r: number; ring: number; x: number; y: number; angle: number }
/** The 37 cells for a comb `size` units across (cell size s = size / 11.6), centred on 0,0. */
export function comb(size: number): { s: number; cells: Cell[] } {
  const s = size / 11.6, cells: Cell[] = [];
  for (let r = -3; r <= 3; r++) for (let q = -3; q <= 3; q++) {
    const ring = Math.max(Math.abs(q), Math.abs(r), Math.abs(q + r));
    if (ring > 3) continue;
    const x = s * Math.sqrt(3) * (q + r / 2), y = s * 1.5 * r;
    cells.push({ q, r, ring, x, y, angle: Math.atan2(y, x) });
  }
  return { s, cells };
}
/** The seven cells that form the "typed" tick. */
export const TICK = new Set(["-1,-1", "-1,0", "-1,1", "0,0", "1,-1", "2,-2", "3,-3"]);
export const CREAM_STOPS = ["#FFFFFF", "#F1EADB", "#CDBFA7"] as const;
export const CORE_STOPS = ["#FFC9AA", "#FF7A45", "#B8380B"] as const;

/** The brand mark: the honeycomb on an ink tile (Honeycomb2.dc.html "icons"). Below 40 px the cells are bigger and flatter. */
export function markSvg(px: number, { tile = true }: { tile?: boolean } = {}): string {
  const small = px < 40, id = `vm${px}`;
  const { s, cells } = comb(small ? 88 : 76);
  const k = small ? 0.96 : 1;
  const ops = small ? [1, 0.95, 0.95, 0.55] : [1, 0.92, 0.62, 0.32];
  const paths = cells.map((c) => `<path d="${hexPath(c.x, c.y, s * 0.82 * k)}" fill="url(#${id}${c.ring === 0 ? "o" : "c"})" opacity="${ops[c.ring]}"/>`).join("");
  const grad = (n: string, st: readonly string[]) => `<linearGradient id="${id}${n}" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${st[0]}"/><stop offset="${n === "o" ? ".45" : ".55"}" stop-color="${st[1]}"/><stop offset="1" stop-color="${st[2]}"/></linearGradient>`;
  const rad = px * 0.235, inner = px * 0.86, off = (px - inner) / 2;
  const base = tile
    ? `<linearGradient id="${id}t" x1=".33" y1="0" x2=".67" y2="1"><stop offset="0" stop-color="#2E2822"/><stop offset=".55" stop-color="#16130F"/><stop offset="1" stop-color="#0C0A08"/></linearGradient>` +
      `</defs><rect width="${px}" height="${px}" rx="${rad}" fill="url(#${id}t)"/><path d="M${rad} .5H${px - rad}" stroke="#fff" stroke-opacity="${px >= 64 ? 0.12 : 0.1}"/>`
    : "</defs>";
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${px}" height="${px}" viewBox="0 0 ${px} ${px}"><defs>${grad("c", CREAM_STOPS)}${grad("o", CORE_STOPS)}${base}` +
    `<svg x="${off}" y="${off}" width="${inner}" height="${inner}" viewBox="-50 -50 100 100">${paths}</svg></svg>`;
}
