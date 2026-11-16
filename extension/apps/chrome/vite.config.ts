import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import { crx } from "@crxjs/vite-plugin";
import { copyFileSync, mkdirSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import manifest from "./manifest.config";

// onnxruntime-web must load its wasm from the extension itself (MV3 forbids remote code).
function ortFiles(): Plugin {
  return {
    name: "voicio-ort-files",
    apply: "build",
    // transformers.js references the runtime with new URL(); we serve our own copy from ort/, so drop the duplicate.
    generateBundle(_o, bundle) {
      for (const k of Object.keys(bundle)) if (/ort-wasm-simd-threaded.*\.(wasm|mjs)$/.test(k)) delete bundle[k];
    },
    closeBundle() {
      const tjs = join(__dirname, "node_modules", "@huggingface", "transformers");
      const out = join(resolve(__dirname, process.env.VOICIO_OUT || "dist"), "ort");
      mkdirSync(out, { recursive: true });
      // The bench clip is only for the e2e/perf scripts; a release build does not ship it.
      if (!process.env.VOICIO_E2E) rmSync(join(out, "..", "bench.wav"), { force: true });
      // The web build of transformers.js ships the matching runtime next to itself.
      for (const f of ["ort-wasm-simd-threaded.jsep.wasm", "ort-wasm-simd-threaded.jsep.mjs"]) copyFileSync(join(tjs, "dist", f), join(out, f));
    },
  };
}

export default defineConfig({
  plugins: [react(), crx({ manifest }), ortFiles()],
  build: {
    outDir: process.env.VOICIO_OUT || "dist", // tests build elsewhere so the folder loaded in Chrome is never rewritten mid-use
    target: "es2022",
    chunkSizeWarningLimit: 900, // the speech worker (transformers.js, ~870 kB) is the largest chunk,
    // Pages the manifest doesn't list (opened with chrome.offscreen / chrome.tabs) still need to be built.
    rollupOptions: { input: { offscreen: "src/offscreen/index.html", permission: "src/permission/index.html", welcome: "src/welcome/index.html" } },
  },
  worker: { format: "es" },
  server: { port: 5173, strictPort: true, hmr: { port: 5173 } },
});
