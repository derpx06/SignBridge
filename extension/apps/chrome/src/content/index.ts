// Content script: finds the focused field, shows the field mic and the pill (Shadow DOM, so the page's CSS can't touch it),
// and inserts the recognised text at the caret. It never runs on password or credit-card fields.
import { createRoot } from "react-dom/client";
import { createElement } from "react";
import { isForbiddenField, loadSettings, type Settings, type Snapshot, type ToContent } from "@voicio/core";
import tokens from "@voicio/ui/tokens.css?inline";
import components from "@voicio/ui/components.css?inline";
import pillCss from "./pill.css?inline";
import { insertText, pressEnter } from "./insert";
import { isEditable, deepActive, type Field } from "./fields";
import { App, type Bridge } from "./App";

declare global { interface Window { __voicio?: boolean } }

if (!window.__voicio) {
  window.__voicio = true;
  boot();
}

function boot() {
  let settings: Settings = loadSettings({});
  let field: Field | null = null; // last focused editable
  let snap: Snapshot | null = null;
  const bridge: Bridge = {
    get: () => ({ snap, field, settings }),
    listeners: new Set(),
    notify() { this.listeners.forEach((l) => l()); },
    send: (m) => void chrome.runtime.sendMessage(m).catch(() => {}),
  };

  const host = document.createElement("voicio-root");
  host.style.cssText = "all:initial;position:fixed;inset:0;width:0;height:0;z-index:2147483647";
  const shadow = host.attachShadow({ mode: "open" });
  // Constructable stylesheets are not subject to the page's CSP, unlike an inline <style> element.
  const sheet = new CSSStyleSheet();
  sheet.replaceSync(`${tokens}\n${components}\n${pillCss}`);
  shadow.adoptedStyleSheets = [sheet];
  const mount = document.createElement("div");
  // Never the page-level "voicio" class: it paints a background, and this layer sits on top of the whole site.
  mount.className = "root";
  mount.style.cssText = "background:transparent;width:0;height:0";
  shadow.append(mount);
  (document.documentElement || document.body).append(host);
  const reactRoot = createRoot(mount);
  const render = () => reactRoot.render(createElement(App, { bridge }));

  const theme = () => {
    const t = settings.theme;
    if (t === "system") mount.removeAttribute("data-theme"); else mount.setAttribute("data-theme", t);
    host.dataset.theme = t === "system" ? "" : t;
  };
  const loadAll = async () => {
    settings = loadSettings((await chrome.storage.sync.get("settings")).settings);
    theme(); bridge.notify();
  };
  void loadAll();
  chrome.storage.onChanged.addListener((c, a) => { if (a === "sync" && c.settings) { settings = loadSettings(c.settings.newValue); theme(); bridge.notify(); } });

  const siteNever = () => settings.sites.some((s) => s.host === location.hostname && s.rule === "never");

  /* focus tracking */
  const onFocus = () => {
    const el = deepActive();
    if (el && isEditable(el) && isForbiddenField(el as HTMLInputElement)) {
      field = null; // a password or card field is focused: never type anywhere else instead
    } else if (el && isEditable(el) && !siteNever()) {
      field = el;
      bridge.send({ t: "focus", editable: true, host: location.hostname });
    } else if (document.hasFocus() || !field?.isConnected) {
      // The user clicked somewhere else on the page: a field they left must not receive text later.
      // (When the window itself lost focus, e.g. to the popup, the field stays the target.)
      field = null;
    }
    bridge.notify();
  };
  document.addEventListener("focusin", onFocus, true);
  document.addEventListener("focusout", () => setTimeout(onFocus, 0), true);
  window.addEventListener("scroll", () => bridge.notify(), { capture: true, passive: true });
  window.addEventListener("resize", () => bridge.notify());
  onFocus();

  /* keyboard: Esc cancels. Hold mode: releasing the shortcut keys after a real hold (>350 ms) finishes; a quick tap just toggles. */
  const active = () => snap && (snap.rec === "listening" || snap.rec === "processing");
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && (active() || snap?.rec === "error" || snap?.rec === "nomodel" || snap?.rec === "done") && snap?.into === "page") { e.preventDefault(); e.stopPropagation(); bridge.send({ t: "cmd", cmd: "cancel", from: "pill" }); }
  }, true);
  window.addEventListener("keyup", (e) => {
    if (settings.mode !== "hold" || snap?.rec !== "listening" || !snap.startedAt) return;
    if ((e.code === "KeyV" || e.key === "Alt" || e.key === "Shift") && Date.now() - snap.startedAt > 350) bridge.send({ t: "cmd", cmd: "stop", from: "shortcut" });
  }, true);

  /* messages from the background */
  chrome.runtime.onMessage.addListener((m: ToContent, _s, reply) => {
    if (m.t === "snapshot") take(m.snap, true)
    else if (m.t === "insert") {
      // The field the user is in (or left when the window lost focus), else the focused one. Never a guess: with no field the user is told so.
      const el = [field, deepActive()].find((e): e is Field => !!e && e.isConnected && isEditable(e) && !isForbiddenField(e as HTMLInputElement));
      if (!el) { bridge.send({ t: "inserted", ok: false, reason: "no-field" }); reply?.({ ok: false, reason: "no-field" }); return false; }
      let ok = true;
      if (m.text) ok = insertText(el, m.text, m.mode);
      if (ok && m.pressEnter) pressEnter(el);
      field = el;
      bridge.send({ t: "inserted", ok });
      reply?.({ ok, reason: ok ? undefined : "blocked" });
      return false;
    } else if (m.t === "ping") { reply?.(true); return false;
    } else if (m.t === "remove-ui") { reactRoot.unmount(); host.remove(); window.__voicio = false; }
    reply?.(true);
    return false;
  });

  // Which tab and frame is this? Only the frame the background chose draws the bar.
  let me: { tabId: number; frameId: number } | null = null;
  let acked = false;
  let levelPort: chrome.runtime.Port | null = null;
  const openLevels = () => {
    if (levelPort) return;
    try { levelPort = chrome.runtime.connect({ name: "voicio-level" }); levelPort.onMessage.addListener((m: { v: number }) => bridge.level?.(m.v)); levelPort.onDisconnect.addListener(() => { levelPort = null; }); } catch { levelPort = null; }
  };
  const closeLevels = () => { levelPort?.disconnect(); levelPort = null; };
  const take = (s: Snapshot, direct = false) => {
    if (!direct && s.rec !== "ready" && s.at && Date.now() - s.at > 6000) return; // a leftover from a session that is no longer running
    const mine = direct || !s.target || (me && s.target.tabId === me.tabId && s.target.frameId === me.frameId);
    snap = mine ? s : { ...s, into: "popup" };
    bridge.notify();
    if (mine && s.into === "page" && s.rec !== "ready" && !acked) { acked = true; bridge.send({ t: "pill-ack" }); }
    if (s.rec === "ready") acked = false;
    // Audio levels come straight from the offscreen document over a port, only while this frame shows the bar and is listening.
    if (mine && s.into === "page" && s.rec === "listening") openLevels(); else closeLevels();
  };
  // The state is also read from storage, so a lost or early message can never leave the page without its bar.
  chrome.storage.onChanged.addListener((c, a) => { if (a === "session" && c.snap?.newValue) take(c.snap.newValue as Snapshot); });
  void chrome.runtime.sendMessage({ t: "whoami" }).then((r) => { me = r as typeof me; return chrome.storage.session.get("snap"); })
    .then((r) => { if (r.snap) take(r.snap as Snapshot); }).catch(() => {});

  render();
}
