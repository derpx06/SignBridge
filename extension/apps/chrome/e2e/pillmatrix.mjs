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

let failed = 0;
const check = (name, ok, extra = "") => { console.log(`${ok ? "PASS" : "FAIL"}  ${name}${extra ? "  " + extra : ""}`); if (!ok) failed++; };
const press = (keys, hold = 0.12) => execFileSync("python3", ["-I", path.join(here, "xkey.py"), keys, String(hold)]);

// The bar must show up on every kind of page. Real Alt+Shift+V on the production build (activeTab only), one page type after another.
const html = (b, extra = "") => `<!doctype html><meta charset=utf-8><title>t</title><style>${extra}</style><body style="margin:0;font:16px sans-serif">${b}`;
const field = `<textarea id=ta rows=3 cols=40 style="margin:20px"></textarea>`;
const other = http.createServer((q, r) => { r.setHeader("content-type", "text/html"); r.end(html(`<textarea id=f rows=3 cols=30></textarea>`)); }).listen(0);
const oport = other.address().port;
const frames = Array.from({ length: 6 }, () => `<iframe src="http://localhost:${oport}/" width=300 height=60></iframe>`).join("");
const routes = {
  plain: [{}, field],
  strict_csp: [{ "content-security-policy": "default-src 'none'; style-src 'none'; img-src 'none'" }, field],
  coop_only: [{ "cross-origin-opener-policy": "same-origin" }, field],
  coep_only: [{ "cross-origin-embedder-policy": "require-corp" }, field],
  coop_allow_popups: [{ "cross-origin-opener-policy": "same-origin-allow-popups" }, field],
  isolated_coep: [{ "cross-origin-embedder-policy": "require-corp", "cross-origin-opener-policy": "same-origin" }, field],
  many_cross_origin_iframes: [{}, field + frames],
  field_inside_iframe: [{}, `<iframe id=fr src="http://localhost:${oport}/" width=400 height=120></iframe>`],
  transformed_page: [{}, field, "html{transform:scale(.9);transform-origin:0 0} body{overflow:hidden}"],
  busy_page: [{}, field + `<script>const t=Date.now();setTimeout(()=>{while(Date.now()-t<2500){}},300)</script>`],
  fixed_overlay_everywhere: [{}, field + `<div style="position:fixed;inset:0;z-index:2147483647;pointer-events:none"></div><div style="position:fixed;bottom:0;left:0;right:0;height:80px;background:#222;z-index:2147483647"></div>`],
  no_body_styles: [{}, `<p>just text</p><input id=ta>`],
};
const server = http.createServer((q, r) => {
  const [h, b, css] = routes[q.url.slice(1)] ?? routes.plain;
  r.setHeader("content-type", "text/html"); for (const [k, v] of Object.entries(h)) r.setHeader(k, v);
  r.end(html(b, css));
}).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;

const ctx = await chromium.launchPersistentContext(profile, {
  executablePath: "/usr/bin/chromium", headless: false, viewport: null,
  args: [`--disable-extensions-except=${ext}`, `--load-extension=${ext}`, "--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream",
    `--use-file-for-fake-audio-capture=${wav}%noloop`, "--no-sandbox", "--ozone-platform=x11", "--window-size=1280,800", "--window-position=0,0"],
});
try {
  let [sw] = ctx.serviceWorkers(); sw ??= await ctx.waitForEvent("serviceworker");
  const id = new URL(sw.url()).host;
  const ui = await ctx.newPage();
  await ui.goto(`chrome-extension://${id}/src/options/index.html`);
  const send = (m) => ui.evaluate((m) => chrome.runtime.sendMessage(m), m);
  const snap = () => send({ t: "get" });
  const until = async (fn, ms, what) => { const t = Date.now(); for (;;) { const v = await fn(); if (v) return v; if (Date.now() - t > ms) throw new Error("timeout: " + what); await new Promise((r) => setTimeout(r, 200)); } };
  await send({ t: "model", op: "install", id: "moonshine-base" });
  await until(async () => (await snap()).model.state === "installed", 240000, "model download");
  const site = await ctx.newPage();
  for (const name of Object.keys(routes).filter((n) => !process.env.PAGES || process.env.PAGES.split(",").includes(n))) {
    try {
      await site.goto(base + name, { waitUntil: "load" });
      await site.bringToFront();
      const sel = name === "field_inside_iframe" ? null : "#ta";
      if (sel) await site.click(sel); else await site.frameLocator("#fr").locator("#f").click();
      await new Promise((r) => setTimeout(r, 400));
      press("Alt_L+Shift_L+v");
      await until(async () => (await snap()).rec === "listening", 15000, "listening");
      // the bar must be on the page within a few seconds, in any frame
      const box = await until(async () => {
        for (const f of site.frames()) {
          const b = await f.evaluate(() => { const c = document.querySelector("voicio-root")?.shadowRoot?.querySelector(".capsule"); const r = c?.getBoundingClientRect(); return r && r.width > 100 ? { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height), vw: innerWidth, vh: innerHeight } : null; }).catch(() => null);
          if (b) return b;
        }
        return null;
      }, 8000, "bar on the page").catch(() => null);
      const s = await snap();
      const inside = !!box && box.x >= 0 && box.y >= 0 && box.x + box.w <= box.vw + 1 && box.y + box.h <= box.vh + 1;
      if (name === "isolated_coep") check(`${name}: falls back to the popup and says why (site access is requested)`, s.into === "popup" && !!s.reach, `reach=${s.reach}`);
      else check(`${name}: the bar is on the page and fully visible`, s.into === "page" && inside, box ? JSON.stringify(box) : `into=${s.into} reach=${s.reach}`);
      // and the words must really land in the field (a cross-origin iframe and a fully isolated page cannot be reached without extra permission)
      if (name !== "field_inside_iframe" && name !== "isolated_coep") {
        await new Promise((r) => setTimeout(r, 7500)); // the fake microphone plays a 5 s clip
        press("Alt_L+Shift_L+v");
        await until(async () => ["done", "error", "ready"].includes((await snap()).rec), 60000, "finish");
        const text = (await site.inputValue("#ta")).trim();
        check(`${name}: the text is typed into the field`, wer(EXPECT, norm(text)) <= 0.4, JSON.stringify(text));
      }
    } catch (e) { check(`${name}: the bar is on the page and fully visible`, false, e.message); }
    await send({ t: "cmd", cmd: "cancel", from: "pill" });
    await until(async () => (await snap()).rec === "ready", 8000, "ready").catch(() => {});
  }
} catch (e) { console.error("FAIL  ", e.message); failed++; }
finally { await ctx.close(); server.close(); other.close(); fs.rmSync(profile, { recursive: true, force: true }); }
console.log(failed ? `\n${failed} check(s) failed` : "\nall checks passed");
process.exit(failed ? 1 : 0);
