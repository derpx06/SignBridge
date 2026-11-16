import { useEffect, useState } from "react";
import { defaultSettings, emptyModel, type Settings, type Snapshot, type ToBackground, type ToContent } from "@voicio/core";
import { getSettings, onSettings, patchSettings } from "./storage";
import { applyTheme } from "@voicio/ui";

export const send = (m: ToBackground) => chrome.runtime.sendMessage(m).catch(() => undefined);

export const blankSnapshot = (): Snapshot => ({
  rec: "ready", partial: [], final: "", secs: 0, model: emptyModel(), models: {}, activeModel: "", mic: "unknown", into: "popup", wordCount: 0,
});

export function useSettings(): [Settings, (p: Partial<Settings>) => void] {
  const [s, setS] = useState<Settings>(defaultSettings());
  useEffect(() => {
    void getSettings().then(setS);
    return onSettings(setS);
  }, []);
  useEffect(() => applyTheme(s.theme), [s.theme]);
  return [s, (p) => { setS((cur) => ({ ...cur, ...p })); void patchSettings(p); }];
}

/** Fill anything missing so a snapshot from another build can never crash a page. */
const norm = (s: Snapshot): Snapshot => ({ ...blankSnapshot(), ...s, models: s.models ?? {}, partial: s.partial ?? [], model: s.model ?? emptyModel() });

export function useSnapshot(): Snapshot {
  const [snap, setSnap] = useState<Snapshot>(blankSnapshot());
  useEffect(() => {
    void send({ t: "get" }).then((s) => s && setSnap(norm(s as Snapshot)));
    const fn = (m: ToContent) => { if (m?.t === "snapshot") setSnap(norm(m.snap)); };
    chrome.runtime.onMessage.addListener(fn);
    return () => chrome.runtime.onMessage.removeListener(fn);
  }, []);
  return snap;
}

/** Newest-last list of mic levels (0..1) for the waveform. Levels come straight from the offscreen document over a port. */
export function useLevels(n: number, active: boolean): number[] {
  const [levels, setLevels] = useState<number[]>(() => Array(n).fill(0));
  useEffect(() => {
    if (!active) { setLevels(Array(n).fill(0)); return; }
    const port = chrome.runtime.connect({ name: "voicio-level" });
    port.onMessage.addListener((m: { v: number }) => setLevels((l) => [...l.slice(1), m.v]));
    return () => port.disconnect();
  }, [n, active]);
  return levels;
}

/** Whole seconds since `startedAt`, re-read once a second while `active`. The background does not tick. */
export function useElapsed(startedAt: number | undefined, active: boolean): number {
  const [, tick] = useState(0);
  useEffect(() => { if (!active) return; const t = setInterval(() => tick((n) => n + 1), 1000); return () => clearInterval(t); }, [active]);
  return active && startedAt ? Math.max(0, Math.floor((Date.now() - startedAt) / 1000)) : 0;
}

export const shortcutKeys = async (): Promise<string[]> => {
  const cmds = await chrome.commands.getAll();
  const s = cmds.find((c) => c.name === "toggle-dictation")?.shortcut;
  return s ? s.split("+").map((k) => k.replace("⌥", "Alt").replace("⇧", "Shift")) : [];
};
export function useShortcut(): string[] {
  const [k, setK] = useState<string[]>(["Alt", "Shift", "V"]);
  useEffect(() => { void shortcutKeys().then((x) => x.length && setK(x)); }, []);
  return k;
}

const EVERYWHERE = ["http://*/*", "https://*/*"];
/** Lets Voicio show its pill on every website without relying on the toolbar click. */
export function useEverywhere() {
  const [on, setOn] = useState(false);
  useEffect(() => {
    const read = async () => setOn(!!(await chrome.storage.local.get("allSites")).allSites && (await chrome.permissions.contains({ origins: EVERYWHERE })));
    void read();
    chrome.permissions.onAdded.addListener(read); chrome.permissions.onRemoved.addListener(read);
    return () => { chrome.permissions.onAdded.removeListener(read); chrome.permissions.onRemoved.removeListener(read); };
  }, []);
  const set = async (v: boolean) => {
    if (v && !(await chrome.permissions.request({ origins: EVERYWHERE }))) return;
    if (!v) await chrome.permissions.remove({ origins: EVERYWHERE }).catch(() => {});
    await chrome.storage.local.set({ allSites: v });
    setOn(v);
  };
  return [on, set] as const;
}

