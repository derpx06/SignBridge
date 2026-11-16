import { z } from "zod";

export const SiteRule = z.object({ host: z.string().min(1), rule: z.enum(["allow", "never"]) });
export const DictionaryItem = z.object({ when: z.string(), write: z.string() });

export const Settings = z.object({
  mode: z.enum(["toggle", "hold"]).default("toggle"),
  insertAs: z.enum(["type", "paste"]).default("type"),
  autostopSecs: z.union([z.literal(0), z.literal(3), z.literal(10)]).default(10), // 0 = off
  theme: z.enum(["system", "light", "dark"]).default("system"),
  pillPos: z.enum(["dock", "above-left", "above-right", "below-left", "below-right"]).default("dock"),
  pillOffset: z.object({ x: z.number(), y: z.number() }).default({ x: 0, y: 0 }), // where the user dragged or nudged the capsule
  intoFocusedField: z.boolean().default(true),
  contextMenu: z.boolean().default(true),
  fieldMic: z.boolean().default(true),
  micId: z.string().default(""), // "" = the system default microphone
  sites: z.array(SiteRule).default([]),
  dictionary: z.array(DictionaryItem).default([]),
  commands: z.record(z.string(), z.boolean()).default({}),
});
export type Settings = z.infer<typeof Settings>;

export const defaultSettings = (): Settings => Settings.parse({});

/** Parse whatever storage returned; broken or missing values fall back to defaults field by field. */
export function loadSettings(stored: unknown): Settings {
  const r = Settings.safeParse(stored ?? {});
  if (r.success) return r.data;
  const obj = (stored ?? {}) as Record<string, unknown>;
  const clean: Record<string, unknown> = {};
  for (const [k, shape] of Object.entries(Settings.shape)) {
    if (shape.safeParse(obj[k]).success) clean[k] = obj[k];
  }
  return Settings.parse(clean);
}

export function siteRule(s: Settings, host: string): "allow" | "never" | undefined {
  return s.sites.find((x) => x.host === host)?.rule;
}
