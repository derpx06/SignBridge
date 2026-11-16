// Runs the extension's built-in bench op (5.4 s clip, warm model) a few times: node e2e/bench.mjs [runs]
// usage: VOICIO_E2E=1 VOICIO_OUT=/abs/out pnpm build && VOICIO_OUT=/abs/out node e2e/bench.mjs 4
import { chromium } from "playwright";
import path from "node:path";
import fs from "node:fs";
import os from "node:os";
import { fileURLToPath } from "node:url";
const here = path.dirname(fileURLToPath(import.meta.url));
const ext = path.resolve(here, "..", process.env.VOICIO_OUT || "dist");
const profile = fs.mkdtempSync(path.join(os.tmpdir(), "voicio-bench-"));
const ctx = await chromium.launchPersistentContext(profile, { executablePath: "/usr/bin/chromium", headless: false, args: [`--disable-extensions-except=${ext}`, `--load-extension=${ext}`, "--headless=new", "--no-sandbox"] });
let [sw] = ctx.serviceWorkers(); sw ??= await ctx.waitForEvent("serviceworker");
const id = new URL(sw.url()).host;
const ui = await ctx.newPage(); await ui.goto(`chrome-extension://${id}/src/options/index.html`);
const send = (m) => ui.evaluate((m) => chrome.runtime.sendMessage(m), m);
await send({ t: "model", op: "install" });
for (;;) { const s = await send({ t: "get" }); if (s.model.state === "installed") break; if (s.model.state === "failed") throw new Error(s.model.error); await new Promise((r) => setTimeout(r, 500)); }
await ui.evaluate(() => { window.__b = []; chrome.runtime.onMessage.addListener((m) => { if (m.t === "bench") __b.push(m); }); });
const iso = await ui.evaluate(() => crossOriginIsolated);
for (let i = 0; i < Number(process.argv[2] || 3); i++) {
  await send({ t: "model", op: "bench", id: "moonshine-base" });
  for (;;) { const n = await ui.evaluate(() => __b.length); if (n > i) break; await new Promise((r) => setTimeout(r, 300)); }
}
const r = await ui.evaluate(() => __b.map((b) => b.ms));
console.log(JSON.stringify({ ext: path.basename(ext), crossOriginIsolated: iso, ms: r }));
await ctx.close(); fs.rmSync(profile, { recursive: true, force: true });
