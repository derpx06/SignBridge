// The same real-keypress test, but against the user's actual browser (Google Chrome, Flatpak) driven over CDP.
// Needs: Xvfb on :99, Chrome started with --remote-debugging-port=9444 --enable-unsafe-extension-debugging, extension loaded.
import { chromium } from "playwright";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const here = path.dirname(fileURLToPath(import.meta.url));
const EXPECT = "whose feet are as the feet of harts and underneath the everlasting arms";
const norm = (s) => s.toLowerCase().replace(/[^a-z' ]+/g, " ").replace(/\s+/g, " ").trim();
const wer = (a, b) => { const r = a.split(" "), h = b.split(" "); const d = Array.from({ length: r.length + 1 }, (_, i) => [i, ...Array(h.length).fill(0)]); for (let j = 0; j <= h.length; j++) d[0][j] = j; for (let i = 1; i <= r.length; i++) for (let j = 1; j <= h.length; j++) d[i][j] = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + (r[i - 1] === h[j - 1] ? 0 : 1)); return d[r.length][h.length] / r.length; };
const press = (keys, hold = 0.12) => execFileSync("python3", ["-I", path.join(here, "xkey.py"), keys, String(hold)], { env: { ...process.env, DISPLAY: ":99" } });
let failed = 0;
const check = (name, ok, extra = "") => { console.log(`${ok ? "PASS" : "FAIL"}  ${name}${extra ? "  " + extra : ""}`); if (!ok) failed++; };

const server = http.createServer((q, r) => { r.setHeader("content-type", "text/html"); r.end(`<!doctype html><meta charset=utf-8><body style="margin:0;font:16px sans-serif"><p>A page</p><textarea id=ta rows=3 cols=40 style="margin:20px"></textarea><br><input id=one style="margin:20px" placeholder="Search">`); });
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${server.address().port}/`;

const browser = await chromium.connectOverCDP("http://127.0.0.1:9444");
const ctx = browser.contexts()[0];
try {
  const sw = ctx.serviceWorkers().find((w) => w.url().includes("service-worker-loader"));
  const id = new URL(sw.url()).host;
  console.log("Chrome", browser.version(), "| extension", id);
  const ui = await ctx.newPage();
  await ui.goto(`chrome-extension://${id}/src/options/index.html`);
  const send = (m) => ui.evaluate((m) => chrome.runtime.sendMessage(m), m);
  const snap = () => send({ t: "get" });
  const until = async (fn, ms, what) => { const t = Date.now(); for (;;) { const v = await fn(); if (v) return v; if (Date.now() - t > ms) throw new Error("timeout: " + what); await new Promise((r) => setTimeout(r, 200)); } };
  const m = (await snap()).model.state;
  if (m !== "installed") { await send({ t: "model", op: "install", id: "moonshine-base" }); await until(async () => (await snap()).model.state === "installed", 300000, "model download"); }
  check("model installed", true);

  const site = await ctx.newPage();
  await site.goto(base, { waitUntil: "load" });
  await site.bringToFront();
  await site.click("#ta");
  await new Promise((r) => setTimeout(r, 500));
  press("Alt_L+Shift_L+v");
  const started = await until(async () => (await snap()).rec === "listening" && (await snap()), 15000, "listening").catch(() => null);
  check("real Alt+Shift+V starts dictation", !!started);
  await new Promise((r) => setTimeout(r, 2500));
  const s = await snap();
  const box = await site.evaluate(() => { const c = document.querySelector("voicio-root")?.shadowRoot?.querySelector(".capsule"); const r = c?.getBoundingClientRect(); return r ? { w: Math.round(r.width), x: Math.round(r.x), y: Math.round(r.y), vh: innerHeight } : null; });
  check("the pill is on the page", s.into === "page" && !!box, JSON.stringify({ into: s.into, reach: s.reach, box }));
  await new Promise((r) => setTimeout(r, 5500));
  press("Alt_L+Shift_L+v");
  await until(async () => ["done", "error", "ready"].includes((await snap()).rec), 60000, "finish");
  const text = (await site.inputValue("#ta")).trim();
  check("the text is typed into the field", wer(EXPECT, norm(text)) <= 0.4, JSON.stringify(text));
  const end = await snap();
  if (!text) console.log("state:", JSON.stringify({ rec: end.rec, error: end.error, detail: end.detail, into: end.into, reach: end.reach }));
} catch (e) { console.error("FAIL  ", e.message); failed++; }
finally { server.close(); await browser.close().catch(() => {}); }
console.log(failed ? `\n${failed} check(s) failed` : "\nall checks passed");
process.exit(failed ? 1 : 0);
