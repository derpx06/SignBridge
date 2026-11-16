// Microphone edge cases in real Chromium: each situation must end in a specific message and a way out, never a silent failure.
// usage: VOICIO_OUT=dist-test VOICIO_E2E=1 pnpm build && VOICIO_OUT=dist-test node e2e/mic.mjs
import { chromium } from "playwright";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import fs from "node:fs";
import os from "node:os";

const here = path.dirname(fileURLToPath(import.meta.url));
const ext = path.join(here, "..", process.env.VOICIO_OUT || "dist");
const speech = path.join(here, "speech.wav");
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "voicio-mic-")); // removed on exit
const profile = path.join(tmp, "profile"); // one profile for every scenario, so the model downloads only once
const silent = path.join(tmp, "silent.wav");
{ // 8 s of digital silence, 16 kHz mono 16-bit
  const n = 16000 * 8, b = Buffer.alloc(44 + n * 2);
  b.write("RIFF", 0); b.writeUInt32LE(36 + n * 2, 4); b.write("WAVEfmt ", 8); b.writeUInt32LE(16, 16); b.writeUInt16LE(1, 20); b.writeUInt16LE(1, 22);
  b.writeUInt32LE(16000, 24); b.writeUInt32LE(32000, 28); b.writeUInt16LE(2, 32); b.writeUInt16LE(16, 34); b.write("data", 36); b.writeUInt32LE(n * 2, 40);
  fs.writeFileSync(silent, b);
}
const server = http.createServer((q, r) => { r.setHeader("content-type", "text/html"); r.end(`<!doctype html><meta charset=utf-8><body style="margin:0;font:16px sans-serif"><p>A page</p><textarea id=ta rows=3 cols=40></textarea>`); }).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;

let failed = 0;
const check = (name, ok, extra = "") => { console.log(`${ok ? "PASS" : "FAIL"}  ${name}${extra ? "  " + extra : ""}`); if (!ok) failed++; };

async function scenario(name, { flags = [], fakeDevice = true, fakeUi = true, audio = speech }, fn) {
  console.log(`\n# ${name}`);
  const args = [`--disable-extensions-except=${ext}`, `--load-extension=${ext}`, "--headless=new", "--no-sandbox", ...flags];
  if (fakeDevice) args.push("--use-fake-device-for-media-stream", `--use-file-for-fake-audio-capture=${audio}%noloop`);
  if (fakeUi) args.push("--use-fake-ui-for-media-stream");
  const ctx = await chromium.launchPersistentContext(profile, { executablePath: "/usr/bin/chromium", headless: false, args });
  try {
    let [sw] = ctx.serviceWorkers(); sw ??= await ctx.waitForEvent("serviceworker");
    const id = new URL(sw.url()).host;
    const ui = await ctx.newPage(); await ui.goto(`chrome-extension://${id}/src/options/index.html`);
    const send = (m) => ui.evaluate((m) => chrome.runtime.sendMessage(m), m);
    const snap = () => send({ t: "get" });
    const until = async (fn, ms, what) => { const t = Date.now(); for (;;) { const v = await fn(); if (v) return v; if (Date.now() - t > ms) throw new Error("timeout: " + what); await new Promise((r) => setTimeout(r, 200)); } };
    const site = await ctx.newPage(); await site.goto(base); await site.bringToFront(); await site.click("#ta");
    const tabId = await ui.evaluate(async (u) => (await chrome.tabs.query({ url: u + "*" }))[0].id, base);
    await fn({ ctx, id, ui, site, send, snap, until, tabId });
  } catch (e) { console.error("FAIL  ", name, e.message); failed++; }
  finally { await ctx.close(); }
}
const pillText = (site) => site.evaluate(() => document.querySelector("voicio-root")?.shadowRoot?.textContent ?? "");

try {
  await scenario("normal microphone", {}, async ({ ui, site, send, snap, until, tabId }) => {
    await send({ t: "model", op: "install", id: "moonshine-base" });
    await until(async () => (await snap()).model.state === "installed", 240000, "model download");
    await send({ t: "cmd", cmd: "start", from: "shortcut", tabId });
    await until(async () => (await snap()).rec === "listening", 15000, "listening");
    const dev = await until(async () => (await snap()).device, 5000, "device label");
    check("the microphone in use is reported", !!dev, JSON.stringify(dev));
    await send({ t: "cmd", cmd: "cancel", from: "pill" });
  });

  // The fake microphone plays its clip once per browser launch, so each live-level test gets its own launch.
  await scenario("settings: microphone picker and live test", {}, async ({ ui, id }) => {
    await ui.goto(`chrome-extension://${id}/src/options/index.html#dictate`);
    await ui.bringToFront();
    await ui.locator("select[aria-label=Microphone]").waitFor();
    const options = await ui.locator("select[aria-label=Microphone] option").count();
    check("settings list the microphones", options >= 1, `${options} option(s)`);
    await ui.getByRole("button", { name: "Test microphone" }).click();
    const heard = await ui.getByText("I can hear you").waitFor({ timeout: 10000 }).then(() => true, () => false);
    check("the mic test hears sound", heard);
  });

  await scenario("onboarding: microphone step", {}, async ({ ui, id }) => {
    await ui.goto(`chrome-extension://${id}/src/welcome/index.html`);
    await ui.bringToFront(); // a background tab does not animate, so the level meter would not run
    await ui.getByRole("button", { name: "Get started" }).click();
    await ui.getByRole("button", { name: "Continue" }).click();
    if (await ui.getByRole("button", { name: "Allow microphone" }).count()) await ui.getByRole("button", { name: "Allow microphone" }).click();
    const heard = await ui.getByText("I can hear you").waitFor({ timeout: 10000 }).then(() => true, () => false);
    check("onboarding mic step hears sound", heard, heard ? "" : (await ui.locator("[role=status]").allInnerTexts()).join(" | "));
    check("onboarding mic step offers a microphone choice", (await ui.locator("#wl-mic option").count()) >= 1);
  });

  await scenario("muted or silent microphone", { audio: silent }, async ({ site, send, snap, until, tabId }) => {
    await send({ t: "cmd", cmd: "start", from: "shortcut", tabId });
    const s = await until(async () => { const x = await snap(); return x.rec === "error" && x; }, 12000, "error for a silent mic");
    check("silence is detected as its own error", s.error === "mic-silent", s.error);
    check("it names the microphone", !!s.detail, JSON.stringify(s.detail));
    check("the pill explains it", /No sound from your microphone/.test(await pillText(site)));
    check("the pill offers a way out", /Choose microphone/.test(await pillText(site)));
  });

  await scenario("the chosen microphone is no longer connected", {}, async ({ ui, site, send, snap, until, tabId }) => {
    await ui.evaluate(() => chrome.storage.sync.set({ settings: { micId: "unplugged-device-id" } }));
    await ui.waitForTimeout(500);
    await send({ t: "cmd", cmd: "start", from: "shortcut", tabId });
    await until(async () => (await snap()).rec === "listening", 15000, "listening with the default microphone");
    check("dictation falls back to the system default", !!(await until(async () => (await snap()).device, 5000, "device")));
    await send({ t: "cmd", cmd: "cancel", from: "pill" });
    await ui.goto(ui.url().split("#")[0] + "#dictate"); await ui.reload();
    const warn = await ui.getByText("isn't connected").waitFor({ timeout: 8000 }).then(() => true, () => false);
    check("settings say the chosen microphone isn't connected", warn);
  });

  await scenario("permission not granted", { fakeUi: false, flags: ["--deny-permission-prompts"] }, async ({ ctx, id, site, send, snap, until, tabId }) => {
    const opened = [];
    ctx.on("page", (p) => opened.push(p));
    await send({ t: "cmd", cmd: "start", from: "shortcut", tabId });
    const s = await until(async () => { const x = await snap(); return x.rec === "error" && x; }, 12000, "permission error");
    check("a missing permission is reported", s.error === "mic-prompt" || s.error === "mic-blocked", s.error);
    await until(async () => opened.some((p) => p.url().includes("/permission/")), 8000, "permission page opens");
    check("the permission page opens by itself", true);
    const pg = opened.find((p) => p.url().includes("/permission/"));
    const steps = await pg.getByText("Let's turn the mic back on").waitFor({ timeout: 10000 }).then(() => true, () => false);
    check("a blocked microphone shows the fix steps", steps, steps ? "" : await pg.innerText("body").catch(() => ""));
    const before = opened.length;
    await pg.getByRole("button", { name: "Open the setting" }).click();
    await until(async () => opened.length > before || opened.some((p) => p.url().startsWith("chrome://settings")), 5000, "settings tab").catch(() => {});
    const urls = (await Promise.all(ctx.pages().map((p) => p.url())));
    check("it opens Voicio's own Chrome setting", urls.some((u) => u.startsWith("chrome://settings/content/siteDetails?site=chrome-extension")), urls.filter((u) => u.startsWith("chrome:")).join(" "));
  });
} finally {
  server.close();
  fs.rmSync(tmp, { recursive: true, force: true });
}
console.log(failed ? `\n${failed} check(s) failed` : "\nall checks passed");
process.exit(failed ? 1 : 0);
