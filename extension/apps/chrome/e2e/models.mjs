// Per-model check in real Chromium: download, verify, dictate a clip, and measure memory.
// Memory = resident set of every process of this Chromium instance, minus the idle baseline (the extension's footprint),
// sampled every 400 ms while the model loads and decodes. The limit is 1 GB for the extension plus the model.
// usage: VOICIO_E2E=1 pnpm build && node e2e/models.mjs [model-id ...]
import { chromium } from "playwright";
import { execSync } from "node:child_process";
import path from "node:path";
import os from "node:os";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import http from "node:http";

const here = path.dirname(fileURLToPath(import.meta.url));
const ext = path.join(here, "..", process.env.VOICIO_OUT || "dist");
const wav = path.join(here, "speech.wav");
const catalog = JSON.parse(fs.readFileSync(path.join(here, "..", "src", "models.json"), "utf8")).models;
const ids = process.argv.slice(2).length ? process.argv.slice(2) : catalog.map((m) => m.id);
const LIMIT_MB = 1024;
const EXPECT = "whose feet are as the feet of harts and underneath the everlasting arms";
const norm = (s) => s.toLowerCase().replace(/[^a-z' ]+/g, " ").replace(/\s+/g, " ").trim();
const wer = (a, b) => { const r = a.split(" "), h = b.split(" "); const d = Array.from({ length: r.length + 1 }, (_, i) => [i, ...Array(h.length).fill(0)]); for (let j = 0; j <= h.length; j++) d[0][j] = j; for (let i = 1; i <= r.length; i++) for (let j = 1; j <= h.length; j++) d[i][j] = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + (r[i - 1] === h[j - 1] ? 0 : 1)); return d[r.length][h.length] / r.length; };

const profile = fs.mkdtempSync(path.join(os.tmpdir(), "voicio-models-"));
const breakdown = () => execSync(`ps -eo rss,args | grep -F -- "${profile}" | grep -v grep || true`, { encoding: "utf8" }).split("\n").filter(Boolean).map((l) => { const [r, ...a] = l.trim().split(/\s+/); const t = (a.join(" ").match(/--type=([a-z-]+)/) ?? [, "browser"])[1]; return `${t}:${Math.round(parseInt(r, 10) / 1024)}`; }).sort((x, y) => parseInt(y.split(":")[1]) - parseInt(x.split(":")[1])).slice(0, 8).join(" ");
const rssMB = () => {
  const out = execSync(`ps -eo rss,args | grep -F -- "${profile}" | grep -v grep || true`, { encoding: "utf8" });
  return out.split("\n").filter(Boolean).reduce((n, l) => n + parseInt(l.trim().split(/\s+/)[0], 10), 0) / 1024;
};
const ctx = await chromium.launchPersistentContext(profile, { executablePath: "/usr/bin/chromium", headless: false, args: [`--disable-extensions-except=${ext}`, `--load-extension=${ext}`, "--headless=new", "--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream", `--use-file-for-fake-audio-capture=${wav}%noloop`, "--no-sandbox"] });
let [sw] = ctx.serviceWorkers(); sw ??= await ctx.waitForEvent("serviceworker");
const id0 = new URL(sw.url()).host;
const ui = await ctx.newPage();
await ui.goto(`chrome-extension://${id0}/src/options/index.html`);
const send = (m) => ui.evaluate((m) => chrome.runtime.sendMessage(m), m);
const snap = () => send({ t: "get" });
const until = async (fn, ms, what) => { const t = Date.now(); for (;;) { const v = await fn(); if (v) return v; if (Date.now() - t > ms) throw new Error("timeout: " + what); await new Promise((r) => setTimeout(r, 300)); } };
const perm = await ctx.newPage(); await perm.goto(`chrome-extension://${id0}/src/permission/index.html?mode=ask`);
await perm.getByRole("button", { name: "Continue" }).click(); await perm.getByText("You're all set").waitFor(); await perm.close();
const server = http.createServer((q, r) => { r.setHeader("content-type", "text/html"); r.end("<textarea id=ta rows=3 cols=40></textarea>"); }).listen(0);
const base_url = `http://127.0.0.1:${server.address().port}/`;
const site = await ctx.newPage();
await site.goto(base_url);
// The default model downloads by itself; stop that so each model is measured alone.
await send({ t: "model", op: "pause" });
await new Promise((r) => setTimeout(r, 1500));

let failed = 0;
const rows = [];
for (const id of ids) {
  const m = catalog.find((x) => x.id === id);
  try {
    await send({ t: "model", op: "install", id });
    await until(async () => ["installed", "failed"].includes((await snap()).models[id]?.state), 900000, `install ${id}`);
    const st = (await snap()).models[id];
    if (st.state !== "installed") throw new Error(st.error ?? "install failed");
    await send({ t: "model", op: "use", id });
    await site.bringToFront(); await site.focus("#ta"); await site.fill("#ta", "");
    const base = rssMB();
    let peak = base, peakBreak = "";
    console.log("   idle:", breakdown());
    const series = [];
    const timer = setInterval(() => { const r = rssMB(); series.push(Math.round(Math.max(...execSync(`ps -eo rss,args | grep -F -- "${profile}" | grep -v grep | grep "type=renderer" | awk '{print $1}' || true`, { encoding: "utf8" }).split("\n").filter(Boolean).map((x) => parseInt(x) / 1024)))); if (r > peak) { peak = r; peakBreak = breakdown(); } }, 400);
    const tabId = await ui.evaluate(async (u) => (await chrome.tabs.query({ url: u + "*" }))[0].id, base_url);
    await send({ t: "cmd", cmd: "toggle", from: "shortcut", tabId });
    await until(async () => (await snap()).rec === "listening", 30000, "listening");
    await new Promise((r) => setTimeout(r, 8500));
    const t0 = Date.now();
    await send({ t: "cmd", cmd: "stop", from: "shortcut" });
    await until(async () => ["done", "error", "ready"].includes((await snap()).rec), 120000, "finish");
    const finishMs = Date.now() - t0;
    if (process.env.DEBUG_SNAP) console.log(JSON.stringify(await ui.evaluate(() => chrome.storage.session.get("log"))), (await snap()).error + " " + (await snap()).rec + " " + (await snap()).detail);
    clearInterval(timer);
    console.log("   peak:", peakBreak);
    console.log("   biggest renderer MB every 400 ms:", series.join(" "));
    const text = await site.inputValue("#ta");
    const w = wer(EXPECT, norm(text));
    const extMB = Math.round(peak - base);
    const ok = w <= 0.3 && extMB <= LIMIT_MB; // memory budget: the extension plus the model stays under 1 GB
    rows.push({ id, MB: Math.round(m.files.reduce((n, f) => n + f.size, 0) / 1e6), WERpct: Math.round(w * 100), stopToTextMs: finishMs, peakProcessesMB: Math.round(peak), extensionDeltaMB: extMB, ok });
    console.log(`${ok ? "PASS" : "FAIL"}  ${id}  WER ${(w * 100).toFixed(0)}%  stop→text ${finishMs} ms  extension +${extMB} MB (all processes ${Math.round(peak)} MB)  “${text.trim()}”`);
    if (!ok) failed++;
    // free the model and wait for the worker to be released before the next one
    await send({ t: "model", op: "remove", id });
    await new Promise((r) => setTimeout(r, 1500));
  } catch (e) { console.log(`FAIL  ${id}  ${e.message}`); failed++; }
}
await ctx.close(); server.close(); fs.rmSync(profile, { recursive: true, force: true });
fs.writeFileSync(path.join(here, "..", "..", "..", "docs", "model-measurements.json"), JSON.stringify(rows, null, 1));
console.log(failed ? `\n${failed} failed` : "\nall models within limits");
process.exit(failed ? 1 : 0);
