// Live cloud check through the real extension: background -> offscreen -> provider, with the user's own keys.
// For every provider with a key in the environment (VOICIO_KEY_GROQ, VOICIO_KEY_OPENAI, ... = the provider id
// upper-cased; Azure also VOICIO_AZURE_REGION), a fresh headless profile gets the key in storage.local (cloud mode
// on, consented, no on-device model, so the cloud takes the dictation), the fake microphone plays the bench clip,
// and the script prints provider, model, latency, WER against providers/tests/bench.txt and PASS/FAIL. Keys are
// never printed, and each profile (which holds the key) is deleted afterwards.
// VOICIO_LIVE=1 turns on each provider's Live toggle, so the streaming path (short-lived token + WebSocket) runs instead;
// providers without a browser live API are then listed as "no live API".
// usage: VOICIO_OUT=dist-test VOICIO_E2E=1 pnpm build && VOICIO_KEY_GROQ=... [VOICIO_LIVE=1] VOICIO_OUT=dist-test node e2e/cloud-live.mjs
import { chromium } from "playwright";
import http from "node:http";
import path from "node:path";
import fs from "node:fs";
import os from "node:os";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const ext = path.resolve(here, "..", process.env.VOICIO_OUT || "dist");
const clip = path.join(here, "speech.wav"); // the same 5.4 s LibriSpeech clip as public/bench.wav
const registry = JSON.parse(fs.readFileSync(path.join(here, "../../../../providers/cloud-stt.json"), "utf8"));
const reference = fs.readFileSync(path.join(here, "../../../../providers/tests/bench.txt"), "utf8").trim();

const words = (s) => s.toLowerCase().replace(/[^\p{L}\p{N}']+/gu, " ").split(" ").filter(Boolean);
function wer(ref, hyp) {
  const r = words(ref), h = words(hyp);
  let row = h.map((_, j) => j + 1); row.unshift(0);
  for (let i = 0; i < r.length; i++) {
    const next = [i + 1];
    for (let j = 0; j < h.length; j++) next.push(Math.min(row[j] + (r[i] === h[j] ? 0 : 1), row[j + 1] + 1, next[j] + 1));
    row = next;
  }
  return row[h.length] / Math.max(1, r.length);
}

const server = http.createServer((q, r) => { r.setHeader("content-type", "text/html"); r.end("<!doctype html><meta charset=utf-8><textarea id=ta rows=3 cols=60></textarea>"); }).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;
const until = async (fn, ms, what) => { const t = Date.now(); for (;;) { const v = await fn(); if (v) return v; if (Date.now() - t > ms) throw new Error("timeout: " + what); await new Promise((r) => setTimeout(r, 200)); } };

const liveRun = process.env.VOICIO_LIVE === "1";
console.log(`clip ${clip}; reference: ${reference}${liveRun ? "; live streaming" : ""}`);
let called = 0, failed = 0;
for (const p of registry.providers) {
  const key = process.env[`VOICIO_KEY_${p.id.toUpperCase()}`]?.trim();
  if (!key) { console.log(`${p.id.padEnd(12)} not called (no VOICIO_KEY_${p.id.toUpperCase()})`); continue; }
  if (liveRun && !p.live?.browser) { console.log(`${p.id.padEnd(12)} no live API in the browser`); continue; }
  called++;
  const model = liveRun ? p.live.default_model : p.models.includes(process.env[`VOICIO_MODEL_${p.id.toUpperCase()}`]) ? process.env[`VOICIO_MODEL_${p.id.toUpperCase()}`] : p.default_model;
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), "voicio-cloud-"));
  const args = [`--disable-extensions-except=${ext}`, `--load-extension=${ext}`, "--headless=new", "--no-sandbox",
    "--use-fake-device-for-media-stream", `--use-file-for-fake-audio-capture=${clip}%noloop`, "--use-fake-ui-for-media-stream"];
  const ctx = await chromium.launchPersistentContext(profile, { executablePath: "/usr/bin/chromium", headless: false, args });
  let line;
  try {
    let [sw] = ctx.serviceWorkers(); sw ??= await ctx.waitForEvent("serviceworker");
    const id = new URL(sw.url()).host;
    const ui = await ctx.newPage(); await ui.goto(`chrome-extension://${id}/src/options/index.html`);
    const send = (m) => ui.evaluate((m) => chrome.runtime.sendMessage(m), m);
    const snap = () => send({ t: "get" });
    await ui.evaluate((c) => chrome.storage.local.set({ cloud: c }), {
      provider: p.id, keys: { [p.id]: key }, consented: [p.id], mode: "always", models: liveRun ? {} : { [p.id]: model }, live: liveRun ? [p.id] : [], region: process.env.VOICIO_AZURE_REGION ?? "",
    });
    const site = await ctx.newPage(); await site.goto(base); await site.bringToFront(); await site.click("#ta");
    const tabId = await ui.evaluate(async (u) => (await chrome.tabs.query({ url: u + "*" }))[0].id, base);
    await send({ t: "cmd", cmd: "start", from: "shortcut", tabId });
    await until(async () => (await snap()).rec === "listening", 15000, "listening");
    await new Promise((r) => setTimeout(r, 6500)); // the clip is 5.4 s
    const t0 = Date.now();
    if ((await snap()).rec === "listening") await send({ t: "cmd", cmd: "stop", from: "shortcut" });
    const s = await until(async () => { const x = await snap(); return x.rec !== "listening" && x.rec !== "processing" ? x : null; }, 120000, "result");
    const ms = Date.now() - t0, text = (s.final ?? "").trim();
    if (text && s.cloud) {
      const w = wer(reference, text), pass = w < 0.2;
      if (!pass) failed++;
      line = `${p.id.padEnd(12)} ${model.padEnd(26)} ${pass ? "PASS" : "FAIL"} ${String(ms).padStart(6)} ms  WER ${(w * 100).toFixed(1).padStart(5)} %  ${JSON.stringify(text)}`;
    } else {
      failed++;
      line = `${p.id.padEnd(12)} ${model.padEnd(26)} FAIL ${String(ms).padStart(6)} ms  ${s.detail ?? s.cloudError ?? s.error ?? `state ${s.rec}, cloud ${s.cloud}`}`;
    }
  } catch (e) {
    failed++;
    line = `${p.id.padEnd(12)} ${model.padEnd(26)} FAIL  ${String(e.message ?? e).replaceAll(key, "<key>")}`;
  } finally {
    await ctx.close();
    fs.rmSync(profile, { recursive: true, force: true }); // the profile holds the key
  }
  console.log(line);
}
console.log(`${called} provider(s) called, ${failed} failed`);
server.close();
process.exit(failed ? 1 : 0);
