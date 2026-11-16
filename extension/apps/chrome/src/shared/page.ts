// Pages Chrome never lets an extension touch (nothing the extension does can change that).
export function restrictedPage(url?: string): boolean {
  if (!url) return true;
  if (!/^https?:\/\//i.test(url)) return true; // chrome://, chrome-search:// (the New Tab page), about:, edge://, file:// ...
  try {
    const u = new URL(url);
    return u.hostname === "chromewebstore.google.com" || (u.hostname === "chrome.google.com" && u.pathname.startsWith("/webstore"));
  } catch { return true; }
}

export type InsertResult = { ok: true } | { ok: false; reason: "restricted" | "unreachable" | "no-field" | "blocked" | "empty"; detail?: string };
