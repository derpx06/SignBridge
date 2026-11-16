// Cactus Whistle runs in its own small WebAssembly engine (needle.wasm, Apache-2.0), same message protocol as asr.worker.ts.
importScripts("needle.js");
let M = null, audioPtr = 0, audioCap = 0;
const OUT_CAP = 16384;
let outPtr = 0;

onmessage = async (e) => {
  const m = e.data;
  try {
    if (m.t === "load") {
      if (!M) {
        const res = await (await caches.open(m.cache)).match(m.url);
        if (!res) throw new Error("Whistle model file is missing from the cache.");
        const bytes = new Uint8Array(await res.arrayBuffer());
        M = await createNeedle();
        const p = M._malloc(bytes.length);
        M.HEAPU8.set(bytes, p);
        let rc;
        try { rc = M._needle_load(p, BigInt(bytes.length)); } catch { rc = M._needle_load(p, bytes.length, 0); }
        if (rc < 0) throw new Error("Whistle failed to load: " + M.UTF8ToString(M._needle_last_error()));
        outPtr = M._malloc(OUT_CAP);
      }
      postMessage({ id: m.id, ok: true });
    } else if (m.t === "run") {
      if (!M) throw new Error("model not loaded");
      const n = m.audio.length;
      if (n * 4 > audioCap) {
        if (audioPtr) M._free(audioPtr);
        audioCap = n * 4;
        audioPtr = M._malloc(audioCap);
      }
      M.HEAPU8.set(new Uint8Array(m.audio.buffer, m.audio.byteOffset, n * 4), audioPtr);
      // language null = detect; the engine returns an empty text for silence
      const rc = M._needle_transcribe(audioPtr, n, 0, 0, 0, outPtr, OUT_CAP);
      if (rc < 0) throw new Error("Whistle: " + M.UTF8ToString(M._needle_last_error()));
      const j = JSON.parse(M.UTF8ToString(outPtr));
      postMessage({ id: m.id, ok: true, text: (j.text || "").trim() });
    } else {
      M = null;
      postMessage({ id: m.id, ok: true });
    }
  } catch (err) {
    postMessage({ id: m.id, ok: false, error: String(err && err.message ? err.message : err) });
  }
};
