import { loadCloud, loadSettings, type CloudConfig, type Settings } from "@voicio/core";

export async function getSettings(): Promise<Settings> {
  return loadSettings((await chrome.storage.sync.get("settings")).settings);
}
export async function patchSettings(patch: Partial<Settings>): Promise<Settings> {
  const next = { ...(await getSettings()), ...patch };
  await chrome.storage.sync.set({ settings: next });
  return next;
}
export function onSettings(cb: (s: Settings) => void): () => void {
  const fn = (changes: Record<string, chrome.storage.StorageChange>, area: string) => {
    if (area === "sync" && changes.settings) cb(loadSettings(changes.settings.newValue));
  };
  chrome.storage.onChanged.addListener(fn);
  return () => chrome.storage.onChanged.removeListener(fn);
}

/** Hosts the user dictated on recently, for the "This site" list. */
export async function recentHosts(): Promise<string[]> {
  return ((await chrome.storage.local.get("recent")).recent as string[] | undefined) ?? [];
}
export async function rememberHost(host: string) {
  if (!host) return;
  const list = [host, ...(await recentHosts()).filter((h) => h !== host)].slice(0, 12);
  await chrome.storage.local.set({ recent: list });
}

/** Cloud speech settings and keys: chrome.storage.local only (never sync), read by the extension's own pages and worker. */
export const getCloud = async (): Promise<CloudConfig> => loadCloud((await chrome.storage.local.get("cloud")).cloud);
export const setCloud = (c: CloudConfig) => chrome.storage.local.set({ cloud: c });
