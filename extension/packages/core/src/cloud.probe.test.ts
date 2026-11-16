// Live probe of every real provider endpoint with a deliberately INVALID key (no account, no user audio: the bench
// clip only). PASS = the provider rejected the key, so URL, method, auth placement and body shape were accepted.
// Batch: transcribeCloud through real fetch. Live (browser path): the short-lived-token endpoint with the invalid key,
// then the WebSocket with a dummy token through the real mechanism (query param or subprotocol, Node's WebSocket).
// Skipped unless VOICIO_PROBE=1:  cd extension/packages/core && VOICIO_PROBE=1 pnpm vitest run src/cloud.probe.test.ts
import { describe, expect, it } from "vitest";
import fs from "node:fs";
import https from "node:https";
import crypto from "node:crypto";
import { CLOUD, CloudError, startLiveCloud, transcribeCloud, type CloudProvider } from "./index";

const KEY = "voicio-probe-invalid";
const AUTH_WORDS = ["api key", "api_key", "apikey", "api-key", "unauthor", "authenticat", "invalid key", "invalid_key", "credential", "token", "permission", "forbidden", "subscription key", "access denied", "not valid", "invalid_auth", "incorrect", "jwt"];
/** Same rule as the desktop `cloud_check::verdict`. */
function verdict(status: number, body: string): [boolean, string] {
  const aboutKey = AUTH_WORDS.some((w) => body.toLowerCase().includes(w));
  if (status === 401 || status === 403) return [true, "key rejected"];
  if ((status === 400 || status === 422) && aboutKey) return [true, "key rejected (4xx with an auth error)"];
  if (status === 404 && aboutKey) return [true, "key rejected (404 with an auth error: AssemblyAI's token endpoint)"];
  if (status === 404 || status === 405) return [false, "wrong path or method"];
  if (status === 400 || status === 415 || status === 422) return [false, "request shape rejected"];
  if (status === 0) return [false, "no HTTP answer (host, DNS, TLS or timeout)"];
  if (status >= 200 && status < 300) return [false, "accepted an invalid key?"];
  return [false, "unexpected status"];
}
const short = (s: string) => JSON.stringify(s.replace(/\s+/g, " ").slice(0, 100));

function benchClip(): Float32Array {
  const b = fs.readFileSync(new URL("../../../apps/chrome/public/bench.wav", import.meta.url));
  const pcm = new Int16Array(b.buffer.slice(b.byteOffset + 44, b.byteOffset + b.length - ((b.length - 44) % 2)));
  return Float32Array.from(pcm, (v) => v / 32768);
}

async function probeBatch(p: CloudProvider, pcm: Float32Array): Promise<[boolean, string]> {
  let last = { url: "", method: "", status: 0, body: "" };
  const rec = async (url: string, init: RequestInit) => {
    try {
      const r = await fetch(url, init);
      last = { url, method: init.method ?? "GET", status: r.status, body: await r.clone().text() };
      return r;
    } catch (e) {
      last = { url, method: init.method ?? "GET", status: 0, body: String((e as Error & { cause?: unknown }).cause ?? e) };
      throw e;
    }
  };
  await transcribeCloud(p, KEY, pcm, { region: p.needsRegion ? "westeurope" : "", language: "en", fetchFn: rec, sleep: async () => {} }).catch(() => {});
  const [ok, why] = verdict(last.status, last.body);
  return [ok, `${ok ? "PASS" : "FAIL"} ${String(last.status).padEnd(3)} ${last.method} ${last.url.replace(/^https:\/\//, "")}  ${why}  ${short(last.body)}`];
}

/** The WebSocket upgrade's HTTP status (101 = accepted) and body, read directly (browser WebSockets hide it). */
function upgradeStatus(url: string, protocols?: string[]): Promise<{ status: number; body: string }> {
  return new Promise((resolve) => {
    const req = https.request(url.replace(/^wss:/, "https:"), {
      headers: { Connection: "Upgrade", Upgrade: "websocket", "Sec-WebSocket-Version": "13", "Sec-WebSocket-Key": crypto.randomBytes(16).toString("base64"), ...(protocols ? { "Sec-WebSocket-Protocol": protocols.join(", ") } : {}) },
      timeout: 15000,
    });
    req.on("upgrade", (_res, socket) => { socket.destroy(); resolve({ status: 101, body: "" }); });
    req.on("response", (res) => { let body = ""; res.on("data", (d) => (body += d)); res.on("end", () => resolve({ status: res.statusCode ?? 0, body })); });
    req.on("timeout", () => req.destroy(new Error("timeout")));
    req.on("error", (e) => resolve({ status: 0, body: String(e) }));
    req.end();
  });
}

async function probeLive(p: CloudProvider, pcm: Float32Array): Promise<[boolean, string]> {
  const live = p.live!, browser = live.browser!;
  // 1. the token endpoint with the invalid key (batch auth header, as startLiveCloud sends it)
  const tr = await fetch(browser.token.url, { method: "POST", headers: { [p.spec.auth[0]!]: `${p.spec.auth[1]}${KEY}`, ...p.spec.headers, "Content-Type": "application/json" }, body: (browser.token.body ?? "").replaceAll("{model}", live.default_model) })
    .then(async (r) => ({ status: r.status, body: await r.text() }), (e) => ({ status: 0, body: String(e.cause ?? e) }));
  const [tokOk, tokWhy] = verdict(tr.status, tr.body);
  // 2. the WebSocket with a dummy token, through the real browser mechanism
  const fakeToken: Record<string, unknown> = {};
  browser.token.text.split("/").slice(1).reduce<Record<string, unknown>>((o, k, i, a) => (o[k] = i === a.length - 1 ? KEY : {}) as Record<string, unknown>, fakeToken);
  let seen = { url: "", protocols: undefined as string[] | undefined, text: "" };
  class RecWS extends WebSocket {
    constructor(url: string, protocols?: string[]) {
      super(url, protocols);
      seen = { url, protocols, text: "" };
      this.addEventListener("message", (e) => { if (typeof e.data === "string") seen.text += ` ${e.data}`; });
      this.addEventListener("close", (e) => { seen.text += ` closed ${e.code} ${e.reason}`; });
    }
  }
  const s = startLiveCloud(p, KEY, { language: "en", fetchFn: async () => new Response(JSON.stringify(fakeToken)), WebSocketImpl: RecWS as unknown as typeof WebSocket });
  for (let i = 0; i < 10; i++) s.feed(pcm.subarray(i * 1600, (i + 1) * 1600));
  const err = await s.finish().then((t) => `accepted? ${t}`, (e: CloudError) => e.kind);
  let wsOk: boolean, wsWhy: string;
  if (err === "auth") [wsOk, wsWhy] = [true, `auth error: ${short(seen.text)}`];
  else {
    const up = await upgradeStatus(seen.url, seen.protocols);
    if (up.status === 101) {
      wsOk = AUTH_WORDS.some((w) => seen.text.toLowerCase().includes(w)) || /closed (1008|400[13])/.test(seen.text);
      wsWhy = `101, then ${err}: ${short(seen.text)}`;
    } else [wsOk, wsWhy] = [verdict(up.status, up.body)[0], `HTTP ${up.status} ${verdict(up.status, up.body)[1]} ${short(up.body)}`];
  }
  const ok = tokOk && wsOk;
  return [ok, `${ok ? "PASS" : "FAIL"} token ${tokOk ? "PASS" : "FAIL"} ${tr.status} ${tokWhy} ${short(tr.body)} | ws ${wsOk ? "PASS" : "FAIL"} ${new URL(live.url).host} ${wsWhy}`];
}

describe.skipIf(process.env.VOICIO_PROBE !== "1")("probe every real endpoint with an invalid key", () => {
  it("batch and live", async () => {
    const pcm = benchClip(), rows: string[] = [];
    let failed = 0;
    for (const p of CLOUD) {
      const [ok, row] = await probeBatch(p, pcm);
      failed += ok ? 0 : 1; rows.push(`${p.id.padEnd(12)} batch ${row}`);
      if (p.liveSupported) {
        const [lok, lrow] = await probeLive(p, pcm);
        failed += lok ? 0 : 1; rows.push(`${p.id.padEnd(12)} live  ${lrow}`);
      } else if (p.live) rows.push(`${p.id.padEnd(12)} live  (no browser path: desktop/Android only)`);
    }
    console.log(["probe (TS, real fetch + Node WebSocket), invalid key " + JSON.stringify(KEY), ...rows, `${failed} failed`].join("\n"));
    expect(failed).toBe(0);
  }, 600000);
});
