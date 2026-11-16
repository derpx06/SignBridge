// The extension must never change how a website looks. Each test page is screenshotted in a browser WITHOUT the
// extension and in one WITH it (content script on every page): the pixels must be identical, idle and while the pill is up.
// usage: VOICIO_OUT=dist-test pnpm build && VOICIO_OUT=dist-test node e2e/pagepaint.mjs
import { chromium } from "playwright";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import fs from "node:fs";
import os from "node:os";

const here = path.dirname(fileURLToPath(import.meta.url));
const ext = path.join(here, "..", process.env.VOICIO_OUT || "dist");
const pages = {
  plain: `<!doctype html><title>Example Domain</title><body style="margin:0;font:18px sans-serif"><div style="margin:60px auto;width:600px"><h1>Example Domain</h1><p>This domain is for use in illustrative examples.</p></div>`,
  coloured: `<!doctype html><body style="margin:0;background:#fff"><div style="height:300px;background:#ffe9a8"></div><div style="height:300px;background:#a8d8ff"></div><textarea></textarea>`,
  dark: `<!doctype html><body style="margin:0;background:#0b0b0b;color:#eee;font:18px sans-serif"><p style="padding:40px">Dark page</p><input>`,
  nobg: `<!doctype html><body><p>No background set at all.</p>`,
};
const server = http.createServer((req, res) => {
  const name = req.url.slice(1).split("?")[0] || "plain";
  res.setHeader("content-type", "text/html");
  if (name === "csp") res.setHeader("content-security-policy", "default-src 'none'; style-src 'none'; img-src 'none'");
  res.end(name === "csp" ? pages.coloured : pages[name] ?? pages.plain);
}).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;
const names = [...Object.keys(pages), "csp"];
const clip = { x: 0, y: 0, width: 1000, height: 420 }; // the top of the page: the pill lives at the bottom

const dirs = [];
const launch = (withExt) => chromium.launchPersistentContext(dirs[dirs.push(fs.mkdtempSync(path.join(os.tmpdir(), "vpp-"))) - 1], {
  executablePath: "/usr/bin/chromium", headless: false, viewport: { width: 1000, height: 700 },
  args: ["--headless=new", "--no-sandbox", ...(withExt ? [`--disable-extensions-except=${ext}`, `--load-extension=${ext}`] : [])],
});

const plainCtx = await launch(false);
const baseline = {};
for (const n of names) { const p = await plainCtx.newPage(); await p.goto(base + n); await p.waitForTimeout(300); baseline[n] = await p.screenshot({ clip }); await p.close(); }
await plainCtx.close();

let failed = 0;
const check = (name, ok, extra = "") => { console.log(`${ok ? "PASS" : "FAIL"}  ${name}${extra ? "  " + extra : ""}`); if (!ok) failed++; };
const ctx = await launch(true);
try {
  let [sw] = ctx.serviceWorkers(); sw ??= await ctx.waitForEvent("serviceworker");
  const id = new URL(sw.url()).host;
  const ui = await ctx.newPage(); await ui.goto(`chrome-extension://${id}/src/options/index.html`);
  await ui.evaluate(() => chrome.storage.local.set({ allSites: true }));
  // "all sites" needs the host permission; the E2E build has it, the production build asks the user instead
  const has = await ui.evaluate(() => chrome.permissions.contains({ origins: ["http://*/*"] }));
  if (!has) { console.log("SKIP  this build has no host permission, so the all-sites content script cannot be registered"); process.exit(0); }
  await ui.waitForTimeout(1000);
  const send = (m) => ui.evaluate((m) => chrome.runtime.sendMessage(m), m);
  for (const n of names) {
    const p = await ctx.newPage(); await p.goto(base + n); await p.waitForTimeout(1200);
    const present = await p.evaluate(() => !!document.querySelector("voicio-root"));
    const idle = await p.screenshot({ clip });
    check(`${n}: content script is on the page`, present);
    check(`${n}: idle page is pixel-identical to the page without Voicio`, Buffer.compare(idle, baseline[n]) === 0);
    // while the pill / a state card is showing (no model installed here, so the "download a model" card appears)
    const tabId = await ui.evaluate(async (u) => (await chrome.tabs.query({ url: u + "*" }))[0].id, base + n);
    await p.bringToFront();
    await send({ t: "cmd", cmd: "start", from: "shortcut", tabId });
    await p.waitForTimeout(1500);
    const shown = await p.evaluate(() => !!document.querySelector("voicio-root")?.shadowRoot?.querySelector(".capsule"));
    const busy = await p.screenshot({ clip });
    check(`${n}: pill shows`, shown);
    check(`${n}: page above the pill is untouched while it shows`, Buffer.compare(busy, baseline[n]) === 0);
    await send({ t: "cmd", cmd: "cancel", from: "pill" });
    await p.close();
  }
} catch (e) { console.error("FAIL  ", e.message); failed++; }
finally { await ctx.close(); server.close(); for (const d of dirs) fs.rmSync(d, { recursive: true, force: true }); }
console.log(failed ? `\n${failed} check(s) failed` : "\nall checks passed");
process.exit(failed ? 1 : 0);
