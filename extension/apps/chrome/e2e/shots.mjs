// Screenshots of every surface, light and dark, for visual review: node e2e/shots.mjs OUTDIR
// Real model and mic where it matters; popup states are pinned with a fake snapshot so each one is deterministic.
// usage: VOICIO_E2E=1 VOICIO_OUT=dist-test pnpm build && VOICIO_OUT=dist-test node e2e/shots.mjs OUTDIR
import { chromium } from "playwright";
import http from "node:http";
import path from "node:path";
import os from "node:os";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
const here = path.dirname(fileURLToPath(import.meta.url));
const ext = path.join(here, "..", process.env.VOICIO_OUT || "dist");
const out = process.argv[2] ?? path.join(os.tmpdir(), "voicio-shots");
fs.mkdirSync(out, { recursive: true });
const profile = fs.mkdtempSync(path.join(os.tmpdir(), "voicio-shots-"));
const server = http.createServer((q, r) => { r.setHeader("content-type", "text/html"); r.end(`<body style="font:16px sans-serif;padding:40px;background:#fff;color:#222"><h2>Compose</h2><textarea id=ta rows=5 cols=60 style="margin-top:80px"></textarea>`); }).listen(0);
const site = `http://127.0.0.1:${server.address().port}/`;
const ctx = await chromium.launchPersistentContext(profile, { executablePath: "/usr/bin/chromium", headless: false, args: [`--disable-extensions-except=${ext}`, `--load-extension=${ext}`, "--headless=new", "--no-sandbox", "--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream"], viewport: { width: 1100, height: 720 } });
const SCHEMES = ["light", "dark"];
try {
  let [sw] = ctx.serviceWorkers(); sw ??= await ctx.waitForEvent("serviceworker");
  const id = new URL(sw.url()).host;
  const url = (p) => `chrome-extension://${id}/${p}`;
  const ui = await ctx.newPage();
  await ui.goto(url("src/options/index.html"));
  const bg = (m) => ui.evaluate((m) => chrome.runtime.sendMessage(m), m);
  // Real states need the model and the mic: the download starts by itself after install; the permission page asks by itself.
  for (let i = 0; i < 2400 && (await bg({ t: "get" })).model.state !== "installed"; i++) await ui.waitForTimeout(250);
  const perm = await ctx.newPage(); await perm.goto(url("src/permission/index.html?mode=ask")); await perm.getByText("You're all set").waitFor(); await perm.close();
  for (const p of ctx.pages()) if (p.url().includes("welcome")) await p.close();
  await ui.evaluate(async () => { const { settings } = await chrome.storage.sync.get("settings"); await chrome.storage.sync.set({ settings: { ...settings, sites: [{ host: "mail.example.com", rule: "allow" }, { host: "bank.example.com", rule: "never" }] } }); });

  /** Open an extension page in a fresh tab; `init` runs before the page's own scripts. */
  const open = async (scheme, p, size, init) => {
    const pg = await ctx.newPage(); await pg.setViewportSize(size); await pg.emulateMedia({ colorScheme: scheme });
    if (init) await pg.addInitScript(init);
    await pg.goto(url(p)); await pg.waitForTimeout(900); // entrance animations
    return pg;
  };
  const save = (pg, name, sel) => (sel ? pg.locator(sel).screenshot({ path: path.join(out, name + ".png") }) : pg.screenshot({ path: path.join(out, name + ".png"), fullPage: true }));

  /* ---------- popup: every state, pinned with a fake snapshot ---------- */
  const installed = { state: "installed", bytes: 67e6, total: 67e6, speed: 0 };
  const base = { rec: "ready", partial: [], final: "", secs: 0, model: installed, models: { "moonshine-base": installed }, activeModel: "moonshine-base", mic: "granted", into: "popup", wordCount: 0 };
  const text = "Please send the report to the whole team by Friday afternoon, and copy Priya.";
  const popupStates = {
    nomodel: { ...base, rec: "nomodel", model: { state: "missing", bytes: 0, total: 67e6, speed: 0 }, models: { "moonshine-base": { state: "missing", bytes: 0, total: 67e6, speed: 0 } } },
    downloading: { ...base, rec: "nomodel", model: { state: "downloading", bytes: 29e6, total: 67e6, speed: 4.2e6 }, models: { "moonshine-base": { state: "downloading", bytes: 29e6, total: 67e6, speed: 4.2e6 } } },
    ready: base,
    listening: { ...base, rec: "listening", startedAt: Date.now(), partial: "please send the report to the whole team by friday".split(" "), wordCount: 9 },
    processing: { ...base, rec: "processing", phase: "cleaning", wordCount: 10, partial: "please send the report to the whole team by friday afternoon".split(" ") },
    done: { ...base, rec: "done", final: text, wordCount: 14 },
    error: { ...base, rec: "error", error: "no-speech", detail: "Microphone: loudest -62 dBFS" },
    cloud: { ...base, rec: "listening", cloud: true, startedAt: Date.now(), wordCount: 7, partial: "cloud speech is on for this one".split(" ") },
  };
  const pin = (snap) => `(() => { const s = ${JSON.stringify(snap)};
    // the popup belongs to a normal web page, not to the tab the screenshot runs in
    const tq = chrome.tabs.query.bind(chrome.tabs);
    chrome.tabs.query = (q) => (q && q.active ? Promise.resolve([{ id: 1, url: "https://mail.example.com/compose" }]) : tq(q));
    const sm = chrome.runtime.sendMessage.bind(chrome.runtime);
    chrome.runtime.sendMessage = (m, ...a) => (m && m.t === "get" ? Promise.resolve(s) : sm(m, ...a));
    const add = chrome.runtime.onMessage.addListener.bind(chrome.runtime.onMessage);
    chrome.runtime.onMessage.addListener = (fn) => add((m, ...r) => (m && m.t === "snapshot" ? undefined : fn(m, ...r))); })()`;
  for (const scheme of SCHEMES) {
    for (const [name, s] of Object.entries(popupStates)) {
      const pg = await open(scheme, "src/popup/index.html", { width: 360, height: 600 }, pin(s));
      await save(pg, `${scheme}-popup-${name}`, "#root"); await pg.close();
    }
    for (const tab of ["This site", "Settings"]) {
      const pg = await open(scheme, "src/popup/index.html", { width: 360, height: 600 }, pin(base));
      await pg.getByText(tab, { exact: true }).click(); await pg.waitForTimeout(300);
      await save(pg, `${scheme}-popup-tab-${tab.replace(" ", "-").toLowerCase()}`, "#root"); await pg.close();
    }
  }

  /* ---------- welcome: all three steps ---------- */
  // Step 2 ("microphone") only appears when the microphone is refused, so that run fakes a refusal.
  const refuse = `(() => { const q = navigator.permissions.query.bind(navigator.permissions);
    navigator.permissions.query = (d) => (d && d.name === "microphone" ? Promise.resolve({ state: "prompt", onchange: null }) : q(d));
    navigator.mediaDevices.getUserMedia = () => Promise.reject(new DOMException("refused", "NotAllowedError")); })()`;
  for (const scheme of SCHEMES) {
    const size = { width: 1100, height: 760 };
    let pg = await open(scheme, "src/welcome/index.html", size);
    await save(pg, `${scheme}-welcome-1-intro`);
    await pg.getByRole("button", { name: "Get started" }).click(); await pg.waitForTimeout(900);
    await save(pg, `${scheme}-welcome-3-ready`); await pg.close();
    pg = await open(scheme, "src/welcome/index.html", size, refuse);
    await pg.getByRole("button", { name: "Allow microphone and start" }).click(); await pg.waitForTimeout(900);
    await save(pg, `${scheme}-welcome-2-mic-refused`); await pg.close();
    pg = await open(scheme, "src/welcome/index.html", { width: 420, height: 800 });
    await save(pg, `${scheme}-welcome-1-intro-narrow`); await pg.close();
  }

  /* ---------- options: every section, the cloud card in each state ---------- */
  await ctx.route("https://api.openai.com/**", (r) => r.fulfill({ status: 401, body: "{}" })); // never reach a real provider from a test
  for (const scheme of SCHEMES) {
    for (const sec of ["welcome", "dictate", "general", "sites", "words", "appearance", "model", "cloud", "privacy", "about"]) {
      await ui.evaluate(() => chrome.storage.local.set({ cloud: { provider: "", keys: {}, consented: [] } }));
      const pg = await open(scheme, `src/options/index.html#${sec}`, { width: 1100, height: 720 });
      await save(pg, `${scheme}-options-${sec}`); await pg.close();
    }
    // consented, key typed, saved: the automatic test fails (fake 401) and the key stays off
    await ui.evaluate(() => chrome.storage.local.set({ cloud: { provider: "", keys: {}, consented: ["openai"] } }));
    const pg = await open(scheme, "src/options/index.html#cloud", { width: 1100, height: 720 });
    await pg.getByRole("button", { name: /^OpenAI/ }).click();
    await pg.getByLabel("OpenAI API key").fill("sk-test-not-a-real-key-1234");
    await pg.getByRole("button", { name: "Save and test" }).click();
    await pg.getByRole("alert").filter({ hasText: "rejected" }).waitFor({ timeout: 20000 }); await pg.waitForTimeout(500); // let button transitions settle
    const off = await pg.getByRole("button", { name: "Set as default" }).isDisabled();
    if (!off) throw new Error("a key that failed its test can still be made the default");
    await save(pg, `${scheme}-options-cloud-key-failed`); await pg.close();
    // consent step for a region provider, plus the collapsed lists opened
    await ui.evaluate(() => chrome.storage.local.set({ cloud: { provider: "", keys: {}, consented: [] } }));
    const pc = await open(scheme, "src/options/index.html#cloud", { width: 1100, height: 720 });
    await pc.getByRole("button", { name: /Microsoft Azure/ }).click();
    for (const t of [/More providers/, /Not supported/]) await pc.getByText(t).click();
    await pc.waitForTimeout(300);
    await save(pc, `${scheme}-options-cloud-consent-more`); await pc.close();
    const narrow = await open(scheme, "src/options/index.html#sites", { width: 560, height: 720 });
    await save(narrow, `${scheme}-options-narrow-sites`); await narrow.close();
    for (const mode of ["ask", "blocked"]) { const p = await open(scheme, `src/permission/index.html?mode=${mode}`, { width: 1000, height: 640 }); await save(p, `${scheme}-permission-${mode}`); await p.close(); }
  }
  await ui.evaluate(() => chrome.storage.local.set({ cloud: { provider: "", keys: {}, consented: [] } }));

  /* ---------- the in-page capsule on a sample page ---------- */
  const page = await ctx.newPage(); await page.setViewportSize({ width: 900, height: 560 });
  await page.goto(site);
  const tabId = await ui.evaluate(async (u) => (await chrome.tabs.query({ url: u + "*" }))[0].id, site);
  const loader = "assets/" + fs.readdirSync(path.join(ext, "assets")).find((f) => /^index\.ts-loader.*\.js$/.test(f) && fs.readFileSync(path.join(ext, "assets", f), "utf8").includes("import"));
  await sw.evaluate(([tabId, loader]) => chrome.scripting.executeScript({ target: { tabId }, files: [loader] }), [tabId, loader]);
  await page.waitForTimeout(800);
  await page.focus("#ta");
  const pill = (rec, extra = {}) => ({ ...base, rec, into: "page", wordCount: 14, startedAt: Date.now(), ...extra });
  // A speaking voice for the "listening-voice" shot: the content script takes levels from whichever extension page answers its port.
  await ui.evaluate(() => chrome.runtime.onConnect.addListener((port) => { if (port.name !== "voicio-level") return; let t = 0; const id = setInterval(() => { if (window.__voice) { t++; try { port.postMessage({ v: 0.75 + 0.2 * Math.sin(t / 2) }); } catch { clearInterval(id); } } }, 50); port.onDisconnect.addListener(() => clearInterval(id)); }));
  const states = {
    listening: pill("listening", { partial: "please send the report to the whole team by friday afternoon".split(" ") }),
    "listening-voice": pill("listening", { partial: "see you at four".split(" ") }),
    "listening-cloud": pill("listening", { cloud: true, partial: "send it to the team".split(" ") }),
    processing: pill("processing", { phase: "cleaning" }),
    done: pill("done", { final: text }),
    error: pill("error", { error: "no-speech", detail: "Microphone: loudest -62 dBFS" }),
    "error-mic-blocked": pill("error", { error: "mic-blocked" }),
    nomodel: pill("nomodel"),
  };
  for (const scheme of SCHEMES) {
    await page.emulateMedia({ colorScheme: scheme });
    await ui.evaluate((tabId) => chrome.tabs.sendMessage(tabId, { t: "snapshot", snap: { rec: "ready", partial: [], final: "", into: "page" } }), tabId);
    const box = await page.locator("#ta").boundingBox();
    await page.mouse.move(box.x + 40, box.y + 20); await page.waitForTimeout(400); // idle: the field mic on hover
    await page.screenshot({ path: path.join(out, `${scheme}-pill-idle-fieldmic.png`) });
    await page.mouse.move(5, 5);
    for (const [name, s] of Object.entries(states)) {
      await ui.evaluate((v) => { window.__voice = v; }, name === "listening-voice");
      await ui.evaluate(([tabId, s]) => chrome.tabs.sendMessage(tabId, { t: "snapshot", snap: s }), [tabId, s]);
      await page.waitForTimeout(name === "listening-voice" ? 900 : 400);
      await page.screenshot({ path: path.join(out, `${scheme}-pill-${name}.png`) });
    }
  }
} finally {
  await ctx.close(); server.close(); fs.rmSync(profile, { recursive: true, force: true });
}
console.log("shots in", out);
