// Model download manager. Files go to OPFS as `.part` (resumable with Range), are SHA-256 checked, and only then
// copied into the Cache API under the exact URL transformers.js will ask for. A wrong or truncated file is deleted.
import type { ModelStatus } from "@voicio/core";
import { cacheName, fileUrl, totalBytes, type ModelEntry, type ModelFile } from "../shared/models";

const dirName = (m: ModelEntry) => `voicio-model-${m.id}`;
type Report = (s: ModelStatus) => void;

async function dir(m: ModelEntry) {
  return (await navigator.storage.getDirectory()).getDirectoryHandle(dirName(m), { create: true });
}
const partName = (f: ModelFile) => f.path.replace(/\//g, "__") + ".part";

async function sha256(buf: ArrayBuffer): Promise<string> {
  const d = new Uint8Array(await crypto.subtle.digest("SHA-256", buf));
  return [...d].map((b) => b.toString(16).padStart(2, "0")).join("");
}

export async function isInstalled(m: ModelEntry): Promise<boolean> {
  const cache = await caches.open(cacheName(m.id));
  for (const f of m.files) {
    const r = await cache.match(fileUrl(m, f.path));
    if (!r || Number(r.headers.get("content-length")) !== f.size) return false;
  }
  return true;
}

export async function remove(m: ModelEntry): Promise<void> {
  await caches.delete(cacheName(m.id));
  await (await navigator.storage.getDirectory()).removeEntry(dirName(m), { recursive: true }).catch(() => {});
}

export async function enoughSpace(m: ModelEntry): Promise<boolean> {
  const est = await navigator.storage.estimate();
  return est.quota === undefined || est.usage === undefined || est.quota - est.usage > totalBytes(m) * 2;
}

/** Returns "done", "paused" (aborted via signal) or throws with a readable message. */
export async function install(m: ModelEntry, report: Report, signal: AbortSignal, keepPartial: () => boolean): Promise<"done" | "paused"> {
  const total = totalBytes(m);
  if (!(await enoughSpace(m))) throw new Error("Not enough free disk space for the speech model.");
  const root = await dir(m);
  const cache = await caches.open(cacheName(m.id));
  let done = 0;
  for (const f of m.files) {
    const url = fileUrl(m, f.path);
    if ((await cache.match(url))?.headers.get("content-length") === String(f.size)) { done += f.size; continue; }
    const handle = await root.getFileHandle(partName(f), { create: true });
    let have = (await handle.getFile()).size;
    if (have > f.size) { have = 0; await (await handle.createWritable()).close(); }
    const base = done;
    let last = performance.now(), lastBytes = base + have, speed = 0;
    const tick = (n: number) => {
      const now = performance.now();
      if (now - last > 400) {
        speed = speed ? speed * 0.6 + ((n - lastBytes) / ((now - last) / 1000)) * 0.4 : (n - lastBytes) / ((now - last) / 1000);
        last = now; lastBytes = n;
      }
      report({ state: "downloading", bytes: n, total, speed });
    };
    if (have < f.size) {
      let res: Response;
      try {
        res = await fetch(url, { headers: have ? { Range: `bytes=${have}-` } : {}, signal });
      } catch (e) {
        if (signal.aborted) { if (!keepPartial()) await root.removeEntry(partName(f)).catch(() => {}); return "paused"; }
        console.error("voicio: fetch failed", url, e);
        throw new Error("Couldn't reach Hugging Face. Check your connection and try again.");
      }
      if (res.status === 200 && have) have = 0; // server ignored Range: start over
      else if (res.status !== 200 && res.status !== 206) throw new Error(`Download failed (HTTP ${res.status}).`);
      const w = await handle.createWritable({ keepExistingData: have > 0 });
      await w.seek(have);
      const reader = res.body!.getReader();
      let got = have;
      try {
        for (;;) {
          const { done: end, value } = await reader.read();
          if (end) break;
          await w.write(value);
          got += value.byteLength;
          tick(base + got);
        }
        await w.close();
      } catch (e) {
        await w.close().catch(() => {});
        if (signal.aborted) { if (!keepPartial()) await root.removeEntry(partName(f)).catch(() => {}); return "paused"; }
        console.error("voicio: download interrupted", f.path, e);
        throw new Error("The download was interrupted. Press Resume to continue.");
      }
    }
    const buf = await (await handle.getFile()).arrayBuffer();
    if (buf.byteLength !== f.size || (await sha256(buf)) !== f.sha256) {
      await root.removeEntry(partName(f)).catch(() => {});
      throw new Error("Checksum didn't match, so the file was removed. Nothing was installed.");
    }
    await cache.put(url, new Response(buf, { headers: { "content-length": String(f.size), "content-type": f.path.endsWith(".json") ? "application/json" : "application/octet-stream" } }));
    await root.removeEntry(partName(f)).catch(() => {});
    done += f.size;
    report({ state: "downloading", bytes: done, total, speed });
  }
  return "done";
}

/** Bytes already on disk for a paused download, to show in the UI. */
export async function partialBytes(m: ModelEntry): Promise<number> {
  const root = await dir(m);
  let n = 0;
  for (const f of m.files) n += await root.getFileHandle(partName(f)).then(async (h) => (await h.getFile()).size, () => 0);
  return n;
}
