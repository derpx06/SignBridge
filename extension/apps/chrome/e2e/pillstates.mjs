// Pill state matrix: screenshots of the lattice capsule in every state, light and dark, plus reduced motion.
// Headless, no model or microphone needed (states are published straight into session storage).
// usage: VOICIO_E2E=1 VOICIO_OUT=dist-e2e pnpm exec vite build && VOICIO_OUT=dist-e2e node e2e/pillstates.mjs OUTDIR
import { chromium } from "playwright";
import http from "node:http";
import path from "node:path";
import os from "node:os";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
const here = path.dirname(fileURLToPath(import.meta.url));
const ext = path.join(here, "..", process.env.VOICIO_OUT || "dist-e2e");
const out = process.argv[2] ?? path.join(os.tmpdir(), "voicio-pill");
fs.mkdirSync(out, { recursive: true });
const server = http.createServer((q, r) => { r.setHeader("content-type", "text/html"); r.end(`<body style="font:16px sans-serif;margin:0;padding:40px;background:#fff"><textarea id=ta rows=4 cols=50></textarea>`); }).listen(0);
const url = `http://127.0.0.1:${server.address().port}/`;
const model = { state: "installed", bytes: 0, total: 0, speed: 0 };
const base = { rec: "ready", partial: [], final: "", secs: 0, model, models: {}, activeModel: "moonshine-base", mic: "granted", into: "page", wordCount: 0 };
const STATES = {
  listening_empty: { rec: "listening", secs: 3 },
  listening_words: { rec: "listening", secs: 5, partial: "so the plan for tomorrow is to ship".split(" ") },
  listening_long: { rec: "listening", secs: 9, partial: "and then we should also remember to email the whole team about the new release schedule before friday".split(" ") },
  processing_cleaning: { rec: "processing", phase: "cleaning", partial: ["done"] },
  processing_typing: { rec: "processing", phase: "typing" },
  done: { rec: "done", wordCount: 12 },
  error_no_speech: { rec: "error", error: "no-speech" },
  error_mic_blocked: { rec: "error", error: "mic-blocked" },
  nomodel: { rec: "nomodel" },
};
const profile = fs.mkdtempSync(path.join(os.tmpdir(), "voicio-pill-"));
const ctx = await chromium.launchPersistentContext(profile, { executablePath: "/usr/bin/chromium", headless: false, args: [`--disable-extensions-except=${ext}`, `--load-extension=${ext}`, "--headless=new", "--no-sandbox"], viewport: { width: 700, height: 360 }, deviceScaleFactor: 2 });
try {
  let [sw] = ctx.serviceWorkers(); sw ??= await ctx.waitForEvent("serviceworker");
  const page = await ctx.newPage();
  await page.goto(url);
  const inject = () => sw.evaluate(async (u) => {
    const [t] = await chrome.tabs.query({ url: u + "*" });
    return t.id;
  }, url);
  const tabId = await inject();
  const loader = fs.readdirSync(path.join(ext, "assets")).find((f) => /^index\.ts-loader/.test(f));
  await sw.evaluate(([id, f]) => chrome.scripting.executeScript({ target: { tabId: id }, files: [f] }), [tabId, `assets/${loader}`]);
  await page.focus("#ta");
  for (const scheme of ["dark", "light"]) for (const reduced of ["no-preference", "reduce"]) {
    await page.emulateMedia({ colorScheme: scheme, reducedMotion: reduced });
    for (const [name, s] of Object.entries(STATES)) {
      await sw.evaluate((snap) => chrome.storage.session.set({ snap }), { ...base, ...s, at: Date.now(), startedAt: Date.now() - 3000 });
      await page.waitForTimeout(name === "done" ? 900 : 500);
      if (s.rec === "listening") await sw.evaluate(async (u) => { const [t] = await chrome.tabs.query({ url: u + "*" }); for (let i = 0; i < 12; i++) { chrome.tabs.sendMessage(t.id, { t: "level", v: 0.35 + 0.5 * Math.abs(Math.sin(i)) }); await new Promise((r) => setTimeout(r, 60)); } }, url);
      await page.screenshot({ path: path.join(out, `${scheme}-${reduced === "reduce" ? "reduced-" : ""}${name}.png`), clip: { x: 100, y: 120, width: 500, height: 240 } });
    }
  }
  console.log("screenshots in", out);
} finally { await ctx.close(); server.close(); }
