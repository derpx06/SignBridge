// End-to-end: real Chromium + the built extension + a fake microphone playing a LibriSpeech clip.
// Needs network once (model download from huggingface.co); the profile is reused so later runs are offline.
// usage: VOICIO_E2E=1 pnpm build && node e2e/run.mjs
import { chromium } from "playwright";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import fs from "node:fs";
import os from "node:os";

const here = path.dirname(fileURLToPath(import.meta.url));
const ext = path.join(here, "..", process.env.VOICIO_OUT || "dist");
// A fresh profile every run: Chrome caches an unpacked extension's service worker inside the profile.
const profile = fs.mkdtempSync(path.join(os.tmpdir(), "voicio-e2e-"));
const wav = path.join(here, "speech.wav");
const EXPECT = "whose feet are as the feet of harts and underneath the everlasting arms";
const norm = (s) => s.toLowerCase().replace(/[^a-z' ]+/g, " ").replace(/\s+/g, " ").trim();
const wer = (a, b) => { // word error rate, Levenshtein over words
  const r = a.split(" "), h = b.split(" ");
  const d = Array.from({ length: r.length + 1 }, (_, i) => [i, ...Array(h.length).fill(0)]);
  for (let j = 0; j <= h.length; j++) d[0][j] = j;
  for (let i = 1; i <= r.length; i++) for (let j = 1; j <= h.length; j++)
    d[i][j] = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + (r[i - 1] === h[j - 1] ? 0 : 1));
  return d[r.length][h.length] / r.length;
};

const page = (body) => `<!doctype html><meta charset=utf-8><title>t</title><body style="font:16px sans-serif">${body}`;
const server = http.createServer((req, res) => {
  res.setHeader("content-type", "text/html");
  if (req.url === "/single") return res.end(page(`<input id=one placeholder="Search">`));
  if (req.url === "/frame") return res.end(page(`<textarea id=f rows=3 cols=40></textarea>`));
  res.end(page(`<textarea id=ta rows=3 cols=40></textarea><br>
    <div id=ce contenteditable=true style="border:1px solid #999;min-height:40px;width:300px"></div><br>
    <input id=pw type=password><br><iframe id=fr src="/frame" width=360 height=120></iframe>`));
}).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;

let failed = 0;
const check = (name, ok, extra = "") => { console.log(`${ok ? "PASS" : "FAIL"}  ${name}${extra ? "  " + extra : ""}`); if (!ok) failed++; };

const ctx = await chromium.launchPersistentContext(profile, {
  executablePath: "/usr/bin/chromium",
  headless: false,
  args: [`--disable-extensions-except=${ext}`, `--load-extension=${ext}`, "--headless=new", "--use-fake-ui-for-media-stream",
    "--use-fake-device-for-media-stream", `--use-file-for-fake-audio-capture=${wav}%noloop`, "--no-sandbox"],
});
try {
  let [sw] = ctx.serviceWorkers();
  sw ??= await ctx.waitForEvent("serviceworker");
  const id = new URL(sw.url()).host;
  const ui = await ctx.newPage(); // an extension page we use to talk to the background
  await ui.goto(`chrome-extension://${id}/src/options/index.html`);
  const send = (m) => ui.evaluate((m) => chrome.runtime.sendMessage(m), m);
  const snap = () => send({ t: "get" });
  const until = async (fn, ms, what) => { const t = Date.now(); for (;;) { const v = await fn(); if (v) return v; if (Date.now() - t > ms) throw new Error("timeout: " + what); await new Promise((r) => setTimeout(r, 250)); } };

  // 1. microphone permission through the real permission page
  const perm = await ctx.newPage();
  await perm.goto(`chrome-extension://${id}/src/permission/index.html?mode=ask`);
  // the page asks Chrome by itself; with the prompt auto-accepted it lands on "all set" without any click
  await perm.getByText("You're all set").waitFor({ timeout: 10000 });
  check("microphone permission page asks by itself", true);

  // 2. model download (resumable, checksum-verified)
  // First run: the welcome tab opens by itself and starts the recommended pack's download (no choices to make).
  await until(async () => ["downloading", "installed"].includes((await snap()).model.state), 60000, "the welcome page starts the download");
  check("recommended pack downloads by itself from the welcome page", true);
  await until(async () => ["installed", "failed"].includes((await snap()).model.state), 600000, "model install");
  const s = await snap();
  check("model installed and verified", s.model.state === "installed", s.model.error ?? "");

  // 3. dictate into each kind of field
  const site = await ctx.newPage();
  await site.goto(base);
  const tabId = await ui.evaluate(async (u) => (await chrome.tabs.query({ url: u + "*" }))[0].id, base);
  const run = async (name, focus, read, { expectText = true } = {}) => {
    await site.bringToFront();
    await focus();
    await send({ t: "cmd", cmd: "toggle", from: "shortcut", tabId });
    await until(async () => (await snap()).rec === "listening", 20000, "listening");
    await new Promise((r) => setTimeout(r, 8500)); // the fake mic plays the 5.4 s clip
    await send({ t: "cmd", cmd: "stop", from: "shortcut" });
    await until(async () => ["done", "error", "ready"].includes((await snap()).rec), 60000, "finish");
    const text = await read();
    if (expectText) {
      const w = wer(EXPECT, norm(text));
      check(`${name}: text inserted`, w <= 0.25, `WER ${(w * 100).toFixed(0)}%  “${text.trim()}”`);
    } else check(`${name}: nothing typed`, text === "", `value “${text}”`);
    if (!expectText) { { const x = await snap(); check("password field shows an error instead of typing elsewhere", x.rec === "error", JSON.stringify({ seq: await ui.evaluate(() => window.__seq), ta: await site.inputValue("#ta"), ce: (await site.textContent("#ce")).slice(-30), pw: await site.inputValue("#pw") })); } await send({ t: "cmd", cmd: "cancel", from: "pill" }); }
    await until(async () => (await snap()).rec === "ready", 8000, "back to ready");
  };
  await run("textarea", () => site.focus("#ta"), () => site.inputValue("#ta"));
  await run("contenteditable", () => site.focus("#ce"), () => site.textContent("#ce"));
  const frame = site.frame({ url: /\/frame$/ });
  await run("iframe textarea", () => frame.focus("#f"), () => frame.inputValue("#f"));
  // 3a. the popup's "Insert into page": into the box you last used; never a guess when no box has focus
  await site.bringToFront(); await site.fill("#ta", ""); await site.focus("#ta");
  check("popup Insert reports success and types into the focused box", (await send({ t: "insert-popup-text" }))?.ok === true && wer(EXPECT, norm(await site.inputValue("#ta"))) <= 0.25);
  const one = await ctx.newPage(); await one.goto(base + "single"); await one.bringToFront();
  const noField = await send({ t: "insert-popup-text" });
  check("popup Insert with no focused box says so and types nothing", noField?.ok === false && noField.reason === "no-field" && (await one.inputValue("#one")) === "", JSON.stringify({ ret: noField, value: await one.inputValue("#one") }));
  await one.focus("#one");
  const okOne = await send({ t: "insert-popup-text" });
  check("popup Insert types into the box once it is focused", okOne?.ok === true && wer(EXPECT, norm(await one.inputValue("#one"))) <= 0.25, JSON.stringify({ ret: okOne, value: await one.inputValue("#one") }));
  await one.close();
  // on a page Chrome keeps extensions off, "Insert into page" must say so instead of pretending to look for a text box
  const blank = await ctx.newPage(); await blank.goto("about:blank"); await blank.bringToFront();
  const rBlank = await send({ t: "insert-popup-text" });
  check("popup Insert explains a page extensions can't use", rBlank && rBlank.ok === false && rBlank.reason === "restricted", JSON.stringify(rBlank));
  await blank.close(); await site.bringToFront();

  await ui.evaluate(() => { window.__seq = []; chrome.runtime.onMessage.addListener((m) => { if (m.t === "snapshot") { const k = m.snap.rec + ":" + (m.snap.error || "") + ":" + (m.snap.detail || "") + ":" + (m.snap.phase || "") + "@" + (Date.now() % 100000); if (window.__seq.at(-1) !== k) window.__seq.push(k); } }); });
  await run("password field is skipped", () => site.focus("#pw"), () => site.inputValue("#pw"), { expectText: false });

  // 3b. the pill must show even when no field has focus (docked at the bottom centre)
  await site.bringToFront(); await site.evaluate(() => document.activeElement && document.activeElement.blur());
  await send({ t: "cmd", cmd: "toggle", from: "shortcut", tabId });
  await until(async () => (await snap()).rec === "listening", 20000, "listening");
  await new Promise((r) => setTimeout(r, 800));
  const box = await site.evaluate(() => { const c = document.querySelector("voicio-root")?.shadowRoot?.querySelector(".capsule"); const r = c?.getBoundingClientRect(); return r ? { x: r.x, y: r.y, w: r.width, h: r.height, vw: innerWidth, vh: innerHeight } : null; });
  check("pill is visible with no field focused", !!box && box.w >= 96 && box.h >= 28 && box.y > box.vh / 2 && Math.abs(box.x + box.w / 2 - box.vw / 2) < 80, JSON.stringify(box));
  await site.screenshot({ path: new URL("./pill-dock.png", import.meta.url).pathname });
  // arrow keys move the focused capsule, and the spot is remembered in settings
  const capX = () => site.evaluate(() => document.querySelector("voicio-root").shadowRoot.querySelector(".capsule").getBoundingClientRect().x);
  await site.evaluate(() => document.querySelector("voicio-root").shadowRoot.querySelector(".capsule").focus());
  const x0 = await capX();
  await site.keyboard.press("ArrowLeft"); await site.keyboard.press("Shift+ArrowLeft");
  const x1 = await capX();
  const saved = await until(async () => { const o = (await ui.evaluate(() => chrome.storage.sync.get("settings"))).settings?.pillOffset; return o && o.x !== 0 ? o : null; }, 4000, "offset saved");
  check("arrow keys nudge the capsule and the position is remembered", Math.round(x0 - x1) === 40 && saved.x === -40 && saved.y === 0, JSON.stringify({ x0, x1, saved }));
  await site.keyboard.press("Escape");
  await until(async () => (await snap()).rec === "ready", 5000, "cancelled");
  await ui.evaluate(async () => { const { settings } = await chrome.storage.sync.get("settings"); await chrome.storage.sync.set({ settings: { ...settings, pillOffset: { x: 0, y: 0 } } }); });

  // 3c. "all sites" registers the content script, so a fresh page has the pill host before any dictation
  await ui.evaluate(() => chrome.storage.local.set({ allSites: true }));
  const fresh = await ctx.newPage(); await fresh.goto(base);
  await until(async () => (await fresh.evaluate(() => !!document.querySelector("voicio-root"))), 8000, "content script registered");
  const ov = await fresh.evaluate(() => { const r = document.querySelector("voicio-root")?.shadowRoot?.querySelector(".root"); const c = r && getComputedStyle(r); return c ? { bg: c.backgroundColor, rect: r.getBoundingClientRect().width } : null; });
  check("page is not covered: overlay is transparent", !!ov && /rgba\(0, 0, 0, 0\)|transparent/.test(ov.bg), JSON.stringify(ov));
  check("all sites: content script present without injection", true);
  await fresh.close();

  // 3d. closing the toolbar popup ends a dictation that only the popup was showing
  await send({ t: "cmd", cmd: "start", from: "options" });
  await until(async () => (await snap()).rec === "listening", 20000, "listening in the popup");
  await ui.evaluate(() => { window.__port = chrome.runtime.connect({ name: "voicio-popup" }); });
  await new Promise((r) => setTimeout(r, 600));
  await ui.evaluate(() => window.__port.disconnect());
  await until(async () => (await snap()).rec !== "listening", 8000, "listening stops after the popup closes");
  check("closing the popup stops listening", true);
  await send({ t: "cmd", cmd: "cancel", from: "pill" });

  // 4. Esc cancels
  await site.bringToFront(); await site.focus("#ta");
  await send({ t: "cmd", cmd: "toggle", from: "shortcut", tabId });
  await until(async () => (await snap()).rec === "listening", 20000, "listening");
  await site.keyboard.press("Escape");
  await until(async () => (await snap()).rec === "ready", 5000, "cancelled");
  check("Esc cancels", true);
} catch (e) {
  console.error("FAIL  ", e.message);
  failed++;
  try { const u = ctx.pages().find((p) => p.url().startsWith("chrome-extension://")); console.error(JSON.stringify(await u.evaluate(async () => ({ snap: await chrome.runtime.sendMessage({ t: "get" }), log: (await chrome.storage.session.get("log")).log })), null, 1).slice(0, 2500)); } catch {}
} finally {
  await ctx.close();
  server.close();
  fs.rmSync(profile, { recursive: true, force: true }); // never leave browser profiles behind in /tmp
}
console.log(failed ? `\n${failed} check(s) failed` : "\nall checks passed");
process.exit(failed ? 1 : 0);
