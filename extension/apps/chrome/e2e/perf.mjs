// Perf probe in headless Chromium: first-text latency, partial lag and runtime-message count while dictating a long clip.
// usage: VOICIO_E2E=1 VOICIO_OUT=/abs/out pnpm build && VOICIO_OUT=/abs/out WAV=/abs/long.wav PROFILE=/abs/profile node e2e/perf.mjs
import { chromium } from "playwright";
import http from "node:http";
import path from "node:path";
import fs from "node:fs";
import os from "node:os";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const ext = path.resolve(here, "..", process.env.VOICIO_OUT || "dist");
const wav = process.env.WAV || path.join(here, "speech.wav");
const profile = process.env.PROFILE || fs.mkdtempSync(path.join(os.tmpdir(), "voicio-perf-"));
const secs = Number(process.env.SECS || 22);
fs.rmSync(path.join(profile, "Default", "Service Worker"), { recursive: true, force: true });
const server = http.createServer((q, r) => { r.setHeader("content-type", "text/html"); r.end("<!doctype html><textarea id=ta rows=3 cols=40></textarea><iframe src=/f width=300 height=100></iframe>"); if (q.url === "/f") { } }).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;
const ctx = await chromium.launchPersistentContext(profile, { executablePath: "/usr/bin/chromium", headless: false,
  args: [`--disable-extensions-except=${ext}`, `--load-extension=${ext}`, "--headless=new", "--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream", `--use-file-for-fake-audio-capture=${wav}%noloop`, "--no-sandbox"] });
let [sw] = ctx.serviceWorkers(); sw ??= await ctx.waitForEvent("serviceworker");
const id = new URL(sw.url()).host;
const ui = await ctx.newPage();
await ui.goto(`chrome-extension://${id}/src/options/index.html`);
const send = (m) => ui.evaluate((m) => chrome.runtime.sendMessage(m), m);
const snap = () => send({ t: "get" });
const until = async (fn, ms, what) => { const t = Date.now(); for (;;) { const v = await fn(); if (v) return v; if (Date.now() - t > ms) throw new Error("timeout: " + what); await new Promise((r) => setTimeout(r, 200)); } };
const perm = await ctx.newPage(); await perm.goto(`chrome-extension://${id}/src/permission/index.html?mode=ask`); await perm.getByText("You're all set").waitFor({ timeout: 15000 }); await perm.close();
if ((await snap()).model.state !== "installed") {
  await send({ t: "model", op: "install" });
  await until(async () => ["installed", "failed"].includes((await snap()).model.state), 600000, "install");
}
console.log("model", (await snap()).model.state);
const site = await ctx.newPage(); await site.goto(base);
const tabId = await ui.evaluate(async (u) => (await chrome.tabs.query({ url: u + "*" }))[0].id, base);
// counts every runtime message the extension pages receive (everything the offscreen document and service worker broadcast)
await ui.evaluate(() => { window.__m = { n: 0, by: {}, snaps: [] }; chrome.runtime.onMessage.addListener((m) => { const k = m.t; __m.n++; __m.by[k] = (__m.by[k] || 0) + 1; if (k === "snapshot") __m.snaps.push([performance.now(), m.snap.rec, m.snap.partial.join(" ")]); }); });
await site.bringToFront(); await site.focus("#ta");
await new Promise((r) => setTimeout(r, Number(process.env.WARMDELAY || 0))); // time between focusing the field and pressing the shortcut
const t0 = await ui.evaluate(() => performance.now());
await send({ t: "cmd", cmd: "toggle", from: "shortcut", tabId });
await until(async () => (await snap()).rec === "listening", 20000, "listening");
await new Promise((r) => setTimeout(r, secs * 1000));
const tStop = await ui.evaluate(() => performance.now());
await send({ t: "cmd", cmd: "stop", from: "shortcut" });
await until(async () => ["done", "ready"].includes((await snap()).rec), 90000, "finish");
const tDone = await ui.evaluate(() => performance.now());
const m = await ui.evaluate(() => __m);
const parts = m.snaps.filter((s) => s[1] === "listening" && s[2]);
const first = parts[0]?.[0] - t0;
const gaps = parts.slice(1).map((s, i) => s[0] - parts[i][0]);
const text = await site.inputValue("#ta");
const log = await ui.evaluate(async () => (await chrome.storage.session.get("log")).log);
console.log(JSON.stringify({ firstTextMs: Math.round(first), partialUpdates: parts.length, meanGapMs: Math.round(gaps.reduce((a, b) => a + b, 0) / (gaps.length || 1)), maxGapMs: Math.round(Math.max(0, ...gaps)), finalLatencyMs: Math.round(tDone - tStop), messages: m.n, by: m.by, chars: text.length }));
console.log("text:", text.slice(0, 200));
console.log((log || []).slice(-6).join("\n"));
await ctx.close(); server.close();
