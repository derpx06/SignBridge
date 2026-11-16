// Speech recognition in a worker so the offscreen page never blocks audio capture.
import { env, pipeline } from "@huggingface/transformers";
import { byId, cacheName } from "../shared/models";

type In = { id: number; t: "load"; model: string } | { id: number; t: "run"; audio: Float32Array } | { id: number; t: "unload" };
let asr: ((a: Float32Array) => Promise<{ text: string } | { text: string }[]>) | null = null;

let activeCache = "";
// transformers.js needs one of the two enabled. Remote stays off so only files verified at download time are used;
// the local path points at a folder that does not exist, so a cache miss is a clear error, never a network fetch.
env.allowLocalModels = true;
env.localModelPath = "/voicio-no-local-models/";
env.allowRemoteModels = false;
env.useBrowserCache = false;
env.useCustomCache = true;
env.customCache = {
  match: async (key: string) => (await (await caches.open(activeCache)).match(key)) ?? undefined,
  put: async () => {},
} as never;
const wasm = env.backends.onnx.wasm!;
wasm.wasmPaths = `${self.location.origin}/ort/`;
wasm.numThreads = Math.max(1, Math.min(4, Math.floor((navigator.hardwareConcurrency || 2) / 2))); // measured: see docs; the extension pages are cross-origin isolated

self.onmessage = async (e: MessageEvent<In>) => {
  const m = e.data;
  try {
    if (m.t === "load") {
      if (!asr) {
        const entry = byId(m.model);
        activeCache = cacheName(entry.id);
        asr = (await pipeline("automatic-speech-recognition", entry.repo, { revision: entry.revision, dtype: entry.dtype as "q8", device: "wasm", session_options: { enableCpuMemArena: false, enableMemPattern: false, graphOptimizationLevel: "basic" } })) as never;
      }
      postMessage({ id: m.id, ok: true });
    } else if (m.t === "run") {
      if (!asr) throw new Error("model not loaded");
      const out = await asr(m.audio);
      postMessage({ id: m.id, ok: true, text: (Array.isArray(out) ? out[0]?.text : out.text)?.trim() ?? "" });
    } else {
      asr = null;
      postMessage({ id: m.id, ok: true });
    }
  } catch (err) {
    postMessage({ id: m.id, ok: false, error: String(err instanceof Error ? err.message : err) });
  }
};
