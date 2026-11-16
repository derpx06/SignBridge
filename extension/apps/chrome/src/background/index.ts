import { activeCloud, originFor, originsFor, initialSession, reduce, runPipeline, siteRule, type ErrorKind, type Settings, type Snapshot, type ToBackground, type ToContent, type ToOffscreen, emptyModel } from "@voicio/core";
import contentScript from "../content/index?script";
import { getCloud, getSettings, patchSettings, rememberHost, onSettings } from "../shared/storage";
import { restrictedPage, type InsertResult } from "../shared/page";
import { DEFAULT_ID, byId, totalBytes } from "../shared/models";

/* ---------- state ---------- */
let settings: Settings;
let snap: Snapshot = { rec: "ready", partial: [], final: "", secs: 0, model: emptyModel(), models: {}, activeModel: DEFAULT_ID, mic: "unknown", into: "popup", wordCount: 0 };
let target: { tabId: number; frameId: number; host: string } | null = null;
let doneTimer: ReturnType<typeof setTimeout> | undefined;
let offscreenReady: Promise<void> | null = null;
// Let content scripts read the state directly: the on-page bar must not depend on a message arriving at the right moment.
void chrome.storage.session.setAccessLevel?.({ accessLevel: "TRUSTED_AND_UNTRUSTED_CONTEXTS" }).catch(() => {});
/** Small persistent log of what Voicio decided and why (hostnames and outcomes only, never dictated text). Shown in Settings -> About. */
let diagQueue: Promise<unknown> = Promise.resolve();
function diag(e: string, d?: Record<string, unknown>) {
  diagQueue = diagQueue.then(async () => {
    const cur = ((await chrome.storage.local.get("diag")).diag as unknown[] | undefined) ?? [];
    await chrome.storage.local.set({ diag: [...cur, { t: new Date().toISOString().slice(5, 19).replace("T", " "), e, ...(d ? { d } : {}) }].slice(-80) });
  }).catch(() => {});
}
self.addEventListener("unhandledrejection", (ev) => diag("unhandled rejection", { msg: String((ev as PromiseRejectionEvent).reason?.message ?? (ev as PromiseRejectionEvent).reason).slice(0, 160) }));
self.addEventListener("error", (ev) => diag("error", { msg: String((ev as ErrorEvent).message).slice(0, 160) }));
diag("service worker started", { version: chrome.runtime.getManifest().version });

const ready = (async () => {
  settings = await getSettings();
  const saved = (await chrome.storage.local.get("activeModel")).activeModel as string | undefined;
  snap = { ...snap, activeModel: byId(saved ?? DEFAULT_ID).id };
})();
onSettings((s) => { settings = s; void syncMenu(); });

const session = () => ({ rec: snap.rec, error: snap.error, partial: snap.partial, final: snap.final });
function apply(e: Parameters<typeof reduce>[1]) {
  const next = reduce(session(), e);
  snap = { ...snap, ...next, wordCount: (next.final || next.partial.join(" ")).split(/\s+/).filter(Boolean).length };
}

/** `persist` writes the state to storage.session (the fallback for a lost message). Live transcript updates skip it: they would write per partial. */
function broadcast(persist = true) {
  snap = { ...snap, at: Date.now() }; // lets pages ignore a saved state that is no longer being updated
  if (persist) { paintBadge(); void chrome.storage.session.set({ snap }).catch(() => {}); }
  const msg: ToContent = { t: "snapshot", snap };
  void chrome.runtime.sendMessage(msg).catch(() => {});
  toTarget(msg);
}
function toTarget(msg: ToContent) {
  if (target && snap.into === "page") void chrome.tabs.sendMessage(target.tabId, msg, { frameId: target.frameId }).catch(() => {});
}

function paintBadge() {
  const dot = { listening: "#14B8A6", processing: "#14B8A6", error: "#FBBF24", nomodel: "#85919A" } as Record<string, string>;
  const c = dot[snap.rec] ?? (snap.model.state === "missing" ? "#85919A" : "");
  void chrome.action.setBadgeText({ text: c ? "●" : "" });
  if (c) {
    void chrome.action.setBadgeBackgroundColor({ color: "#00000000" });
    void chrome.action.setBadgeTextColor?.({ color: c });
  }
}

/* ---------- offscreen document ---------- */
async function ensureOffscreen() {
  offscreenReady ??= (async () => {
    if (!(await chrome.offscreen.hasDocument())) {
      await chrome.offscreen.createDocument({ url: "src/offscreen/index.html", reasons: [chrome.offscreen.Reason.USER_MEDIA], justification: "Record the microphone and run the speech model while the user dictates." });
    }
  })().catch((e) => { offscreenReady = null; throw e; });
  await offscreenReady;
}
async function toOffscreen(m: ToOffscreen) {
  await ensureOffscreen();
  await chrome.runtime.sendMessage(m).catch(() => {});
}

/** Load the speech model ahead of the first dictation (a field was focused or the popup opened). Cheap when already loaded. */
let warmedAt = 0;
function warm() {
  if (snap.rec !== "ready" || snap.models[snap.activeModel]?.state !== "installed" || Date.now() - warmedAt < 60_000) return;
  warmedAt = Date.now();
  void toOffscreen({ target: "offscreen", t: "warm", model: snap.activeModel }).catch(() => {});
}

/** Ask the offscreen document for the state of every catalogue model. */
const checkModels = () => toOffscreen({ target: "offscreen", t: "model", op: "check", id: snap.activeModel }).catch(() => {});

/* ---------- targets ---------- */
const hostOf = (url?: string) => { try { return new URL(url ?? "").hostname; } catch { return ""; } };

let reachError = "";
/** The content script loads asynchronously after injection: wait until it answers before talking to it. */
async function waitForContent(tabId: number, frameId = 0) {
  for (let i = 0; i < 25; i++) {
    try { await chrome.tabs.sendMessage(tabId, { t: "ping" } satisfies ToContent, { frameId }); return true; }
    catch { await new Promise((r) => setTimeout(r, 120)); }
  }
  return false;
}
/** Inject the content script (activeTab/host permission) and find the frame that holds the focused field. */
async function findTarget(tabId: number): Promise<typeof target> {
  try {
    // Top frame only; a subframe gets the script on demand, below, when the focused field turns out to be inside it.
    await chrome.scripting.executeScript({ target: { tabId }, files: [contentScript] });
    if (!(await waitForContent(tabId))) throw new Error("the page script did not start");
    const res = await chrome.scripting.executeScript({
      target: { tabId, allFrames: true },
      func: () => {
        const el = document.activeElement as HTMLElement | null;
        const ok = !!el && (el.isContentEditable || el instanceof HTMLTextAreaElement ||
          (el instanceof HTMLInputElement && !["checkbox", "radio", "button", "submit", "file", "range", "color", "image", "reset"].includes(el.type)));
        return { ok, depth: ok ? 1 : 0, host: location.hostname };
      },
    }).catch(() => [] as chrome.scripting.InjectionResult<{ ok: boolean; depth: number; host: string }>[]);
    const hit = res.filter((r) => r.result?.ok).pop();
    const tab = await chrome.tabs.get(tabId);
    if (hit && hit.frameId !== 0) {
      await chrome.scripting.executeScript({ target: { tabId, frameIds: [hit.frameId] }, files: [contentScript] }).catch(() => {});
      await waitForContent(tabId, hit.frameId);
    }
    if (hit) return { tabId, frameId: hit.frameId, host: hostOf(tab.url) || (hit.result?.host ?? "") };
    return { tabId, frameId: 0, host: hostOf(tab.url) };
  } catch (e) {
    console.warn("voicio: cannot reach the page", e);
    reachError = String((e as Error)?.message ?? e).replace(/^Error:\s*/, "").slice(0, 160);
    return null; // chrome:// pages, the Web Store, PDFs...
  }
}

/* ---------- dictation ---------- */
async function activeTab(): Promise<chrome.tabs.Tab | undefined> {
  return (await chrome.tabs.query({ active: true, currentWindow: true }))[0] ?? (await chrome.tabs.query({ active: true, lastFocusedWindow: true }))[0];
}

/** Ask for access to one site (shown by Chrome as a normal permission prompt), remember it, and put the bar on the page. */
async function askForSite(tabId: number, url: string) {
  const u = new URL(url);
  const granted = await chrome.permissions.request({ origins: [`${u.origin}/*`] }).catch(() => false);
  if (!granted) return;
  const cur = await getSettings();
  if (!cur.sites.some((s) => s.host === u.hostname)) await patchSettings({ sites: [...cur.sites, { host: u.hostname, rule: "allow" }] });
  if (snap.rec !== "listening" && snap.rec !== "processing") return;
  const found = await findTarget(tabId);
  if (!found) return;
  target = found;
  snap = { ...snap, into: "page", target: { tabId: found.tabId, frameId: found.frameId }, reach: undefined };
  broadcast();
}

let pillAcked = false;
let procTimer: ReturnType<typeof setTimeout> | undefined;
async function start(from: string, tabId?: number) {
  await ready;
  const t0 = Date.now();
  if (snap.rec === "listening" || snap.rec === "processing") return;
  clearTimeout(doneTimer);
  let cloudOnly: Awaited<ReturnType<typeof cloudFor>>;
  if (snap.model.state !== "installed") {
    await checkModels();
    // The answer arrives as an off-model message; give it a moment.
    for (let i = 0; i < 20 && (snap.model.state as string) === "unknown"; i++) await new Promise((r) => setTimeout(r, 100));
    // No on-device model: cloud mode "fallback" (or "always") can still take the dictation.
    if ((snap.model.state as string) !== "installed") cloudOnly = await cloudFor(false);
    if ((snap.model.state as string) !== "installed" && !cloudOnly) {
      // Say so where the user is looking: on the page when it can be reached, otherwise in the toolbar popup.
      const t = tabId ? await chrome.tabs.get(tabId).catch(() => undefined) : await activeTab();
      target = t?.id && from !== "options" ? await findTarget(t.id) : null;
      snap = { ...snap, rec: "nomodel", partial: [], final: "", into: target ? "page" : "popup", target: target ? { tabId: target.tabId, frameId: target.frameId } : undefined };
      broadcast();
      if (!target && from !== "options" && from !== "popup") void chrome.action.openPopup().catch(() => {});
      return;
    }
  }
  const tab = tabId ? await chrome.tabs.get(tabId) : await activeTab();
  const host = hostOf(tab?.url);
  diag("start", { from, host: host || "(none)", restricted: restrictedPage(tab?.url), tab: tab?.id ?? null });
  if (host && siteRule(settings, host) === "never") return; // inert on "Never" sites
  let into: Snapshot["into"] = "popup";
  target = null;
  if (tab?.id && from !== "options") {
    const found = await findTarget(tab.id);
    diag(found ? "page reached" : "page NOT reached", found ? { frame: found.frameId, ms: Date.now() - t0 } : { why: reachError });
    if (found) {
      target = found;
      into = settings.intoFocusedField || from !== "popup" ? "page" : "popup";
    }
  }
  if (into === "popup" && from !== "options" && from !== "popup" && tab?.id && !restrictedPage(tab.url)) {
    // Some pages (for example cross-origin isolated ones) refuse the shortcut's one-time access. Ask Chrome for this one site;
    // when it is allowed, the running dictation moves onto the page by itself.
    void askForSite(tab.id, tab.url!);
  }
  if (into === "popup" && from !== "options" && from !== "popup") {
    // The page can't be reached (chrome:// pages, the Web Store, PDFs): show the live text in the toolbar popup instead.
    void chrome.action.openPopup().catch(() => {});
  }
  const cloud = cloudOnly ?? (await cloudFor(true));
  snap = { ...snap, cloud: !!cloud, cloudError: undefined, into, target: target ? { tabId: target.tabId, frameId: target.frameId } : undefined, device: undefined, reach: into === "popup" && from !== "options" && from !== "popup" && tab?.id ? (restrictedPage(tab.url) ? "restricted" : reachError || "the page did not respond") : undefined, startedAt: Date.now(), host: target?.host ?? host, secs: 0, phase: undefined, error: undefined };
  apply({ type: "start", cloudOnly: !!cloudOnly });
  broadcast();
  pillAcked = false;
  if (into === "page") {
    // If the page never confirms it drew the pill (blocked, odd frame, strict site), show the live text in the toolbar popup instead.
    // First look again: a heavy page can take a moment to start the script. Only then give up and use the popup.
    setTimeout(() => { if (!pillAcked && snap.rec === "listening" && snap.into === "page" && target) void waitForContent(target.tabId, target.frameId).then(() => broadcast()); }, 1200);
    setTimeout(() => {
      if (pillAcked || snap.rec !== "listening" || snap.into !== "page") return;
      diag("bar not confirmed by the page, using the popup", { ms: Date.now() - t0 });
      snap = { ...snap, into: "popup", reach: "the page did not confirm it could show the bar" };
      broadcast();
      void chrome.action.openPopup().catch(() => {});
    }, 3500);
  }
  await toOffscreen({ target: "offscreen", t: "start", autostopSecs: settings.autostopSecs, model: snap.activeModel, micId: settings.micId, cloud: cloud && { provider: cloud.provider.id, key: cloud.key, model: cloud.model, region: cloud.region, only: !!cloudOnly, live: cloud.live } });
}

/** Cloud speech is used only when the user turned it on, accepted the notice, saved a key, and Chrome still holds the host permission.
 *  `alwaysOnly`: with an on-device model installed, only mode "always" sends audio ("fallback" waits for a missing model). */
async function cloudFor(alwaysOnly: boolean) {
  const c = activeCloud(await getCloud());
  if (!c || (alwaysOnly && c.mode !== "always")) return undefined;
  if (!(await chrome.permissions.contains({ origins: [originFor(c.provider, c.region)] }).catch(() => false))) return undefined;
  // Live also needs its token and WebSocket hosts; without them the dictation is sent whole (batch).
  const live = c.live && (await chrome.permissions.contains({ origins: originsFor(c.provider, c.region) }).catch(() => false));
  return { ...c, live };
}

async function stop() {
  if (snap.rec !== "listening") return;
  apply({ type: "stop" });
  clearTimeout(procTimer);
  // Recognition that never returns must not leave the UI on "Cleaning up…" forever.
  procTimer = setTimeout(() => { if (snap.rec === "processing") { void toOffscreen({ target: "offscreen", t: "cancel" }); fail("engine", "recognition took too long"); } }, 60000);
  snap = { ...snap, phase: "cleaning" };
  broadcast();
  await toOffscreen({ target: "offscreen", t: "stop" });
}

async function cancel() {
  clearTimeout(doneTimer);
  if (snap.rec === "listening") await toOffscreen({ target: "offscreen", t: "cancel" });
  apply({ type: "cancel" });
  snap = { ...snap, phase: undefined, error: undefined, into: snap.rec === "nomodel" ? "popup" : snap.into }; // hides the on-page "no model" card
  broadcast();
}

function fail(kind: ErrorKind, detail?: string) {
  diag("dictation failed", { kind, detail: detail?.slice(0, 120) });
  clearTimeout(procTimer);
  if (snap.rec !== "listening" && snap.rec !== "processing") return;
  apply({ type: "fail", error: kind });
  snap = { ...snap, detail, mic: kind === "mic-blocked" ? "denied" : kind === "no-mic" ? "none" : snap.mic };
  broadcast();
  if (kind === "mic-prompt") void chrome.tabs.create({ url: chrome.runtime.getURL("src/permission/index.html?mode=ask") });
}

function finishOk() {
  apply({ type: "inserted" });
  snap = { ...snap, phase: undefined };
  broadcast();
  doneTimer = setTimeout(() => { apply({ type: "reset" }); broadcast(); }, snap.final ? 4000 : 1500); // long enough to reach Undo / Copy
}

async function onFinal(text: string, cloud?: boolean, cloudError?: Snapshot["cloudError"]) {
  await ready;
  clearTimeout(procTimer);
  apply({ type: "final", text: "" });
  const r = runPipeline(text, { dictionary: settings.dictionary, commands: settings.commands });
  snap = { ...snap, final: r.text, cloud: cloud === true, cloudError, wordCount: r.text.split(/\s+/).filter(Boolean).length, phase: "typing" };
  broadcast();
  if (!r.text && !r.pressEnter) return fail("no-speech");
  await chrome.storage.session.set({ transcript: r.text }).catch(() => {});
  if (snap.into === "page" && target) {
    const msg: ToContent = { t: "insert", text: r.text, mode: settings.insertAs, pressEnter: r.pressEnter };
    try {
      await chrome.tabs.sendMessage(target.tabId, msg, { frameId: target.frameId });
      // the content script answers with {t:'inserted'}
    } catch {
      fail("output-blocked");
    }
  } else {
    finishOk();
  }
}

/* ---------- context menu ---------- */
async function syncMenu() {
  await chrome.contextMenus.removeAll();
  if (!(await getSettings()).contextMenu) return;
  chrome.contextMenus.create({ id: "dictate-here", title: "Dictate here", contexts: ["editable"] });
}
chrome.runtime.onInstalled.addListener((d) => {
  void syncMenu();
  if (d.reason === "install") {
    // Nothing downloads until the user picks a model on the welcome page.
    void checkModels().catch(() => {});
    void chrome.tabs.create({ url: chrome.runtime.getURL("src/welcome/index.html") });
  } else void checkModels().catch(() => {});
});
chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId === "dictate-here" && tab?.id) void handle({ t: "cmd", cmd: "toggle", from: "menu" }, tab.id);
});
chrome.commands.onCommand.addListener((name, tab) => {
  if (name !== "toggle-dictation") return;
  // Chrome swallows the key press that triggers a command, so hold-to-talk is: the command starts, the page sees the release.
  void handle({ t: "cmd", cmd: "toggle", from: "shortcut" }, tab?.id);
});
chrome.runtime.onStartup.addListener(() => void checkModels().catch(() => {}));

/* ---------- the toolbar popup closing ends a dictation that only the popup was showing ---------- */
chrome.runtime.onConnect.addListener((port) => {
  if (port.name !== "voicio-popup") return;
  warm();
  port.onDisconnect.addListener(() => { if (snap.into === "popup" && snap.rec === "listening") void stop(); });
});

/* ---------- site rules: permanent content script for allowed sites ---------- */
async function registerAllowed() {
  const s = await getSettings();
  const wanted = s.sites.filter((x) => x.rule === "allow").map((x) => x.host);
  const have = (await chrome.scripting.getRegisteredContentScripts()).map((x) => x.id);
  const drop = have.filter((id) => id.startsWith("site:") && !wanted.includes(id.slice(5)));
  if (drop.length) await chrome.scripting.unregisterContentScripts({ ids: drop });
  for (const host of wanted) {
    if (have.includes(`site:${host}`)) continue;
    const origins = [`*://${host}/*`];
    if (!(await chrome.permissions.contains({ origins }))) continue;
    await chrome.scripting.registerContentScripts([{ id: `site:${host}`, matches: origins, js: [contentScript], allFrames: false, runAt: "document_idle", persistAcrossSessions: true }]).catch(() => {});
  }
}
/** "All sites": one permission, one content script, so the pill never depends on activeTab. */
async function registerEverywhere() {
  const { allSites } = await chrome.storage.local.get("allSites");
  const origins = ["http://*/*", "https://*/*"];
  const on = !!allSites && (await chrome.permissions.contains({ origins }));
  const have = (await chrome.scripting.getRegisteredContentScripts()).some((x) => x.id === "all-sites");
  if (on && !have) await chrome.scripting.registerContentScripts([{ id: "all-sites", matches: origins, js: [contentScript], allFrames: false, runAt: "document_idle", persistAcrossSessions: true }]).catch(() => {});
  else if (!on && have) await chrome.scripting.unregisterContentScripts({ ids: ["all-sites"] }).catch(() => {});
}
chrome.storage.onChanged.addListener((c, a) => { if (a === "local" && c.allSites) void registerEverywhere(); });
chrome.permissions.onRemoved.addListener(() => void registerEverywhere());
chrome.permissions.onAdded.addListener(() => void registerEverywhere());
chrome.runtime.onInstalled.addListener(() => void registerEverywhere());
chrome.runtime.onStartup.addListener(() => void registerEverywhere());
onSettings(() => void registerAllowed());
chrome.permissions.onAdded.addListener(() => void registerAllowed());
chrome.runtime.onInstalled.addListener(() => void registerAllowed());

/* ---------- messages ---------- */
async function handle(m: ToBackground, senderTab?: number): Promise<unknown> {
  await ready;
  switch (m.t) {
    case "get": return snap;
    case "cmd": {
      if (m.cmd === "cancel") return cancel();
      if (m.cmd === "stop") return stop();
      if (m.cmd === "start") return start(m.from, m.tabId ?? senderTab);
      return snap.rec === "listening" ? stop() : start(m.from, m.tabId ?? senderTab);
    }
    case "pill-ack": if (!pillAcked) diag("bar confirmed by the page"); pillAcked = true; return;
    case "inserted": {
      if (snap.rec !== "processing") return; // a popup "Insert into page" is not part of a dictation
      diag("insert into page", { ok: m.ok, reason: m.reason });
      if (!m.ok) return fail("output-blocked");
      if (target) void rememberHost(target.host);
      return finishOk();
    }
    case "clear": snap = { ...snap, final: "", partial: [], wordCount: 0 }; apply({ type: "reset" }); return broadcast();
    case "model": {
      const id = byId(m.id ?? snap.activeModel).id;
      if (m.op === "use") {
        await chrome.storage.local.set({ activeModel: id });
        snap = { ...snap, activeModel: id, model: snap.models[id] ?? emptyModel(), rec: snap.rec === "nomodel" && snap.models[id]?.state === "installed" ? "ready" : snap.rec };
        if (snap.rec === "ready" && snap.models[id]?.state !== "installed") snap = { ...snap, rec: "nomodel" };
        return broadcast();
      }
      // What the user last asked for decides whether the download starts by itself later.
      if (m.op === "install") {
        await chrome.storage.local.set({ wantModel: true });
        // Choosing a model to download while none is ready makes it the one in use.
        if (snap.models[snap.activeModel]?.state !== "installed" && snap.activeModel !== id) {
          await chrome.storage.local.set({ activeModel: id });
          snap = { ...snap, activeModel: id, model: snap.models[id] ?? snap.model };
          broadcast();
        }
      }
      else if (m.op === "pause" || m.op === "cancel" || m.op === "remove") await chrome.storage.local.set({ wantModel: false });
      return toOffscreen({ target: "offscreen", t: "model", op: m.op, id });
    }
    case "mic": {
      if (m.op === "ask-done") { snap = { ...snap, mic: "granted" }; if (snap.rec === "error") { apply({ type: "reset" }); } broadcast(); }
      return;
    }
    case "insert-popup-text": {
      const tab = await activeTab();
      if (!snap.final) return { ok: false, reason: "empty" } satisfies InsertResult;
      if (!tab?.id || restrictedPage(tab.url)) return { ok: false, reason: "restricted" } satisfies InsertResult; // New Tab page, chrome:// pages, the Web Store
      const found = await findTarget(tab.id);
      if (!found) { diag("insert: page not reached", { why: reachError }); return { ok: false, reason: "unreachable", detail: reachError } satisfies InsertResult; }
      try {
        const res = (await chrome.tabs.sendMessage(found.tabId, { t: "insert", text: snap.final, mode: settings.insertAs, pressEnter: false } satisfies ToContent, { frameId: found.frameId })) as { ok?: boolean; reason?: string } | undefined;
        diag("insert from popup", { ok: res?.ok === true, reason: res?.reason });
        return (res?.ok === true ? { ok: true } : { ok: false, reason: res?.reason === "blocked" ? "blocked" : "no-field" }) satisfies InsertResult; // the page says whether the text really went in
      } catch { return { ok: false, reason: "unreachable", detail: "the page did not answer" } satisfies InsertResult; }
    }
    case "open": {
      const pages = { options: "src/options/index.html", permission: "src/permission/index.html?mode=ask", "permission-blocked": "src/permission/index.html?mode=blocked", "mic-settings": "src/options/index.html#dictate" } as const;
      if (m.page === "shortcuts") return void chrome.tabs.create({ url: "chrome://extensions/shortcuts" });
      if (m.page === "restart") return void chrome.tabs.create({ url: "chrome://restart" }); // restarts Chrome and brings the tabs back
      return void chrome.tabs.create({ url: chrome.runtime.getURL(pages[m.page]) });
    }
    case "off-partial": apply({ type: "partial", words: m.words }); return broadcast(false);
    case "off-final": return onFinal(m.text, m.cloud, m.cloudError);
    case "off-error": return fail(m.kind, m.detail);
    case "off-device": snap = { ...snap, device: m.label }; return broadcast();
    case "off-silence": return void stop();
    case "off-model": {
      const models = { ...snap.models, [m.id]: m.status };
      let active = snap.activeModel;
      // The active model was removed but another is installed: switch to it so dictation keeps working.
      if (models[active]?.state === "missing" && Object.values(models).some((x) => x.state === "installed")) {
        active = Object.keys(models).find((k) => models[k]!.state === "installed")!;
        await chrome.storage.local.set({ activeModel: active });
      }
      const model = models[active] ?? emptyModel();
      snap = { ...snap, models, activeModel: active, model, rec: snap.rec === "nomodel" && model.state === "installed" ? "ready" : snap.rec };
      broadcast();
      // Start or resume the active model's download by itself unless the user paused, cancelled or removed it.
      if (m.id === active && (m.status.state === "missing" || m.status.state === "paused") && (await chrome.storage.local.get("wantModel")).wantModel === true) {
        await toOffscreen({ target: "offscreen", t: "model", op: "install", id: active });
      }
      return;
    }
    case "off-bench": return void chrome.runtime.sendMessage({ t: "bench", rtf: m.rtf, ms: m.ms, id: m.id, heapMB: m.heapMB }).catch(() => {});
    case "off-online": {
      // Wi-Fi came back: carry on with a download that stopped on an error, if the user still wants it.
      const st = snap.model.state;
      if ((st === "failed" || st === "paused") && (await chrome.storage.local.get("wantModel")).wantModel === true) await toOffscreen({ target: "offscreen", t: "model", op: "install", id: snap.activeModel });
      return;
    }
    case "off-log": {
      const log = (((await chrome.storage.session.get("log")).log as string[] | undefined) ?? []).concat(`${new Date().toISOString().slice(11, 19)} ${m.msg}`).slice(-100);
      return void chrome.storage.session.set({ log });
    }
    case "off-ready": return;
    case "focus": return warm();
  }
}

chrome.runtime.onMessage.addListener((m: ToBackground | ToOffscreen, sender, reply) => {
  if ((m as ToOffscreen).target === "offscreen") return false;
  if ((m as ToBackground).t === "shortcut") { void chrome.commands.getAll().then((c) => reply(c.find((x) => x.name === "toggle-dictation")?.shortcut ?? "")); return true; }
  if ((m as ToBackground).t === "whoami") { reply({ tabId: sender.tab?.id ?? -1, frameId: sender.frameId ?? 0 }); return false; }
  void handle(m as ToBackground, sender.tab?.id).then((r) => reply(r)).catch((e) => { diag("message failed", { t: (m as ToBackground).t, msg: String(e).slice(0, 160) }); reply({ ok: false, error: String(e) }); });
  return true;
});

void (async () => {
  await ready;
  // Nothing is cleaned up here: a start can arrive at the very moment the worker wakes, and a new recording already drops any old one.
  snap.model = { ...snap.model, total: totalBytes(byId(snap.activeModel)) };
  void syncMenu();
  void checkModels().catch(() => {});
})();
