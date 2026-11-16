import data from "../models.json";

export interface ModelFile { path: string; size: number; sha256: string }
export interface ModelEntry {
  id: string; name: string; tier: "Tiny" | "Small" | "Medium"; kind: string; license: string; ramMB?: number;
  repo: string; revision: string; dtype: string; wer: number; rtf: number; files: ModelFile[];
}
export const CATALOG = data.models as ModelEntry[];
/** What each model is, shown under its name (model first; languages and maker as the subtitle). */
export const INFO: Record<string, { languages: string; maker: string; tagline: string }> = {
  "moonshine-tiny": { languages: "English", maker: "Useful Sensors", tagline: "Lightest and quickest. Fine for short phrases and older computers; it mishears a few more words." },
  "moonshine-base": { languages: "English", maker: "Useful Sensors", tagline: "More accurate for emails and messages, a little larger. Recommended for most people." },
};
export const infoOf = (m: ModelEntry) => INFO[m.id] ?? { languages: "", maker: "", tagline: m.kind };
/** "English · by Useful Sensors" */
export const subtitleOf = (m: ModelEntry) => [infoOf(m).languages, infoOf(m).maker && `by ${infoOf(m).maker}`].filter(Boolean).join(" · ");
export const DEFAULT_ID = data.default;
export const byId = (id: string): ModelEntry => CATALOG.find((m) => m.id === id) ?? CATALOG.find((m) => m.id === DEFAULT_ID)!;
export const cacheName = (id: string) => `voicio-model-${id}`;
export const fileUrl = (m: ModelEntry, path: string) => `https://huggingface.co/${m.repo}/resolve/${m.revision}/${path}`;
export const totalBytes = (m: ModelEntry) => m.files.reduce((n, f) => n + f.size, 0);

export function formatBytes(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
  if (n >= 1e6) return `${Math.round(n / 1e6)} MB`;
  return `${Math.max(1, Math.round(n / 1e3))} KB`;
}
/** "263 of 640 MB · 21.3 MB/s · 18 s left" */
export function progressLine(bytes: number, total: number, speed: number): string {
  const unit = total >= 1e9 ? 1e9 : 1e6;
  const u = unit === 1e9 ? "GB" : "MB";
  const fmt = (n: number) => (n / unit).toFixed(unit === 1e9 ? 1 : 0);
  let s = `${fmt(bytes)} of ${fmt(total)} ${u}`;
  if (speed > 0) {
    s += ` · ${(speed / 1e6).toFixed(1)} MB/s`;
    const left = Math.max(0, Math.round((total - bytes) / speed));
    s += ` · ${left} s left`;
  }
  return s;
}
