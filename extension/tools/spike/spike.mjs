// Model spike: WER + real-time factor of a transformers.js ASR model on local LibriSpeech clips (Node CPU).
// usage: node spike.mjs <model-id> <dtype> [clips=20]
import { pipeline } from "@huggingface/transformers";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";

const [model, dtype = "q8", n = "20"] = process.argv.slice(2);
const dir = path.join(os.homedir(), ".cache/whispr-voice-lab/eval/librispeech-test-clean");
const clips = fs.readdirSync(dir).filter((f) => f.endsWith(".wav")).sort().filter((_, i) => i % 12 === 0).slice(0, +n);

function readWav(p) {
  const b = fs.readFileSync(p);
  let o = 12, rate = 16000, data;
  while (o < b.length - 8) {
    const id = b.toString("ascii", o, o + 4), size = b.readUInt32LE(o + 4);
    if (id === "fmt ") rate = b.readUInt32LE(o + 12);
    if (id === "data") { data = b.subarray(o + 8, o + 8 + size); break; }
    o += 8 + size;
  }
  const f = new Float32Array(data.length / 2);
  for (let i = 0; i < f.length; i++) f[i] = data.readInt16LE(i * 2) / 32768;
  if (rate !== 16000) throw new Error("need 16 kHz, got " + rate);
  return f;
}
const norm = (s) => s.toLowerCase().replace(/[^a-z' ]+/g, " ").split(/\s+/).filter(Boolean);
function wer(ref, hyp) {
  const d = Array.from({ length: ref.length + 1 }, (_, i) => [i, ...Array(hyp.length).fill(0)]);
  for (let j = 0; j <= hyp.length; j++) d[0][j] = j;
  for (let i = 1; i <= ref.length; i++) for (let j = 1; j <= hyp.length; j++)
    d[i][j] = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + (ref[i - 1] === hyp[j - 1] ? 0 : 1));
  return d[ref.length][hyp.length];
}

const t0 = Date.now();
const asr = await pipeline("automatic-speech-recognition", model, { dtype, device: "cpu" });
console.log(`loaded in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
let errs = 0, words = 0, audio = 0, took = 0;
for (const c of clips) {
  const a = readWav(path.join(dir, c));
  const ref = norm(fs.readFileSync(path.join(dir, c.replace(".wav", ".txt")), "utf8"));
  const t = Date.now();
  const out = await asr(a);
  took += (Date.now() - t) / 1000; audio += a.length / 16000;
  errs += wer(ref, norm(out.text)); words += ref.length;
}
console.log(JSON.stringify({ model, dtype, peakRssMB: Math.round(process.resourceUsage().maxRSS / 1024), peakRssMB: Math.round(process.resourceUsage().maxRSS / 1024), clips: clips.length, WER: +(100 * errs / words).toFixed(2), audioSec: +audio.toFixed(1), RTF: +(took / audio).toFixed(3) }));
