// End-to-end: real Chromium + the built extension + a fake microphone playing a LibriSpeech clip.
// Needs network once (model download from huggingface.co); the profile is reused so later runs are offline.
// usage: VOICIO_E2E=1 pnpm build && node e2e/run.mjs
import { chromium } from "playwright";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import fs from "node:fs";
import os from "node:os";
import { execFileSync } from "node:child_process";

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
  if (req.url === "/frame") return res.end(page(`<textarea id=f rows=3 cols=40></textarea>`));
  res.end(page(`<textarea id=ta rows=3 cols=40></textarea><br>
    <div id=ce contenteditable=true style="border:1px solid #999;min-height:40px;width:300px"></div><br>
    <input id=pw type=password><br><iframe id=fr src="/frame" width=360 height=120></iframe>`));
}).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;

let failed = 0;
const check = (name, ok, extra = "") => { console.log(`${ok ? "PASS" : "FAIL"}  ${name}${extra ? "  " + extra : ""}`); if (!ok) failed++; };
const press = (keys, hold = 0.12) => execFileSync("python3", ["-I", path.join(here, "xkey.py"), keys, String(hold)]);

// Real site, real key press, production build: does the pill show up?
const URL_ = process.env.SITE_URL, SEL = process.env.SITE_SEL || "input";
const ctx = await chromium.launchPersistentContext(profile, {
  executablePath: "/usr/bin/chromium", headless: false, viewport: null,
  args: [`--disable-extensions-except=${ext}`, `--load-extension=${ext}`, "--use-fake-ui-for-media-stream",
    "--use-fake-device-for-media-stream", `--use-file-for-fake-audio-capture=${wav}%noloop`, "--no-sandbox", "--ozone-platform=x11", "--window-size=1280,800", "--window-position=0,0"],
});
try {
  let [sw] = ctx.serviceWorkers(); sw ??= await ctx.waitForEvent("serviceworker");
  const id = new URL(sw.url()).host;
  const ui = await ctx.newPage();
  await ui.goto(`chrome-extension://${id}/src/options/index.html`);
  const send = (m) => ui.evaluate((m) => chrome.runtime.sendMessage(m), m);
  const snap = () => send({ t: "get" });
  const until = async (fn, ms, what) => { const t = Date.now(); for (;;) { const v = await fn(); if (v) return v; if (Date.now() - t > ms) throw new Error("timeout: " + what); await new Promise((r) => setTimeout(r, 250)); } };
  await send({ t: "model", op: "install", id: "moonshine-base" });
  await until(async () => (await snap()).model.state === "installed", 240000, "model download");
  const site = await ctx.newPage();
  await site.goto(URL_, { waitUntil: "domcontentloaded", timeout: 60000 });
  await site.waitForTimeout(4000);
  await site.bringToFront();
  await site.locator(SEL).first().click({ timeout: 15000 }).catch((e) => console.log("click failed:", e.message.split("\n")[0]));
  await site.waitForTimeout(500);
  press("Alt_L+Shift_L+v");
  const s1 = await until(async () => (await snap()).rec === "listening" && (await snap()), 15000, "listening");
  await site.waitForTimeout(2500);
  const s2 = await snap();
  const box = await site.evaluate(() => { const c = document.querySelector("voicio-root")?.shadowRoot?.querySelector(".capsule"); const r = c?.getBoundingClientRect(); return r ? { w: r.width, x: r.x, y: r.y, vw: innerWidth, vh: innerHeight } : null; });
  check("dictation started", true, `into=${s2.into}`);
  check("the pill is on the page (not the toolbar popup)", s2.into === "page" && !!box, JSON.stringify(box));
  await site.screenshot({ path: path.join(here, "site-pill.png") });
  press("Alt_L+Shift_L+v");
  await until(async () => ["done", "error", "ready"].includes((await snap()).rec), 60000, "finish");
  console.log("final:", JSON.stringify({ rec: (await snap()).rec, error: (await snap()).error, detail: (await snap()).detail }));
} catch (e) { console.error("FAIL  ", e.message); failed++; }
finally { await ctx.close(); fs.rmSync(profile, { recursive: true, force: true }); }
process.exit(failed ? 1 : 0);
