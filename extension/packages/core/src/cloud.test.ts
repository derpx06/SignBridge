import { describe, expect, it } from "vitest";
import { liveModelFor } from "./cloud";
import { CLOUD, CLOUD_EXCLUDED, CloudError, LiveState, activeCloud, classifyLive, originsFor, resampleFrom16k, startLiveCloud, azureHost, cloudById, cloudHosts, encodeWav, loadCloud, mapStatus, originFor, retryAfterMs, splitAudio, testKey, tidy, timeoutFor, transcribeCloud, transcriptOnly } from "./index";
import contract from "../../../../providers/tests/contract.json";

const pcm = Float32Array.from({ length: 1600 }, (_, i) => Math.sin(i / 10) * 0.5);
interface Seen { url: string; init: RequestInit }
/** A fake fetch answering `replies` in order; records every request and checks the key never reaches a URL. */
function fake(...replies: [number, unknown][]) {
  const seen: Seen[] = [];
  const fetchFn = async (url: string, init: RequestInit) => {
    seen.push({ url, init });
    expect(url).not.toContain("SECRET");
    const [status, body] = replies.shift() ?? [500, {}];
    return new Response(status === 204 ? null : typeof body === "string" ? body : JSON.stringify(body), { status });
  };
  return { seen, fetchFn, sleep: async () => {} };
}
const h = (s: Seen) => s.init.headers as Record<string, string>;
const form = (s: Seen) => s.init.body as FormData;
const json = (s: Seen) => JSON.parse(s.init.body as string);
const run = async (id: string, replies: [number, unknown][], o: { language?: string; model?: string; region?: string } = {}) => {
  const f = fake(...replies);
  const text = await transcribeCloud(cloudById(id)!, "SECRET", pcm, { ...o, fetchFn: f.fetchFn, sleep: f.sleep });
  return { text, seen: f.seen };
};

describe("registry", () => {
  it("parses and every provider has a known style", () => {
    expect(CLOUD).toHaveLength(16); // 15 providers, Lemonfox twice (EU endpoint)
    expect(CLOUD_EXCLUDED).toHaveLength(6);
    for (const p of CLOUD) {
      expect(p.models).toContain(p.defaultModel);
      expect(["multipart", "raw", "json_audio", "upload_poll"]).toContain(p.spec.style);
      expect(p.name && p.maker && p.priceNote && p.speedNote).toBeTruthy();
    }
    expect(originFor(cloudById("azure")!, "westeurope")).toBe("https://westeurope.api.cognitive.microsoft.com/*");
    expect(cloudHosts()).toContain("https://*.api.cognitive.microsoft.com");
    expect(cloudHosts()).toContain("https://api.soniox.com");
  });
});

describe("wav", () => {
  it("writes a 16 kHz mono 16-bit header", () => {
    const w = encodeWav(pcm), v = new DataView(w.buffer);
    expect(String.fromCharCode(...w.slice(0, 4))).toBe("RIFF");
    expect(w.length).toBe(44 + 3200);
    expect(v.getUint32(24, true)).toBe(16000); expect(v.getUint16(22, true)).toBe(1); expect(v.getUint16(34, true)).toBe(16);
    expect(encodeWav(Float32Array.of(2, -2)).slice(44)).toEqual(new Uint8Array(new Int16Array([32767, -32768]).buffer));
  });
});

describe("multipart", () => {
  it("OpenAI-compatible providers share multipart + Bearer", async () => {
    for (const id of ["openai", "groq", "mistral", "together"]) {
      const { text, seen } = await run(id, [[200, { text: " Hello there. " }]], { language: "hi" });
      expect(text).toBe("Hello there.");
      expect(h(seen[0]!).Authorization).toBe("Bearer SECRET");
      expect((form(seen[0]!).get("file") as File).type).toBe("audio/wav");
      expect(form(seen[0]!).get("language")).toBe("hi");
    }
    expect((await run("groq", [[200, { text: "x" }]])).seen[0]!.url).toBe("https://api.groq.com/openai/v1/audio/transcriptions");
    expect(form((await run("together", [[200, { text: "x" }]])).seen[0]!).get("language")).toBe("auto");
    expect((await run("lemonfoxeu", [[200, { text: "x" }]])).seen[0]!.url).toBe("https://eu-api.lemonfox.ai/v1/audio/transcriptions");
  });
  it("ElevenLabs, Cartesia, Sarvam, Azure, Speechmatics", async () => {
    const e = await run("elevenlabs", [[200, { text: "x" }]]);
    expect(h(e.seen[0]!)["xi-api-key"]).toBe("SECRET"); expect(form(e.seen[0]!).get("model_id")).toBe(cloudById("elevenlabs")!.defaultModel);
    const c = await run("cartesia", [[200, { text: "ink" }]]);
    expect(h(c.seen[0]!)["Cartesia-Version"]).toBe("2026-08-14"); expect(form(c.seen[0]!).get("model")).toBe("ink-whisper");
    const s = await run("sarvam", [[200, { transcript: "namaste" }]], { language: "hi" });
    expect(s.text).toBe("namaste"); expect(h(s.seen[0]!)["api-subscription-key"]).toBe("SECRET"); expect(form(s.seen[0]!).get("language_code")).toBe("hi-IN");
    // 31 s is over Sarvam's 29 s cap: two requests, texts joined
    const long = fake([200, { transcript: "one" }], [200, { transcript: " two " }]);
    expect(await transcribeCloud(cloudById("sarvam")!, "k", new Float32Array(16000 * 31), { fetchFn: long.fetchFn })).toBe("one two");
    expect(long.seen.length).toBe(2);
    const a = await run("azure", [[200, { combinedPhrases: [{ text: "Hallo Welt." }] }]], { language: "de", region: "westeurope" });
    expect(a.text).toBe("Hallo Welt.");
    expect(a.seen[0]!.url).toBe("https://westeurope.api.cognitive.microsoft.com/speechtotext/transcriptions:transcribe?api-version=2025-10-15");
    expect(h(a.seen[0]!)["Ocp-Apim-Subscription-Key"]).toBe("SECRET"); expect(form(a.seen[0]!).get("definition")).toBe('{"locales":["de-DE"]}');
    expect(form(a.seen[0]!).get("audio")).toBeInstanceOf(File);
    await expect(run("azure", [])).rejects.toMatchObject({ kind: "region" });
    await expect(run("azure", [], { region: "evil.com/x?" })).rejects.toMatchObject({ kind: "region" });
    const m = await run("speechmatics", [[201, { id: "j1", status: "done", txt: "from the job\n" }]]);
    expect(m.text).toBe("from the job"); expect(m.seen[0]!.url).toBe("https://eu1.asr.api.speechmatics.com/v2/jobs?format=txt&wait=25");
    expect(JSON.parse(form(m.seen[0]!).get("config") as string).transcription_config).toEqual({ language: "auto", operating_point: "enhanced" });
    await expect(run("speechmatics", [[201, { id: "j1", status: "created" }]])).rejects.toMatchObject({ kind: "reply" });
  });
});

describe("raw body", () => {
  it("Deepgram sends the WAV with Token auth and options in the query", async () => {
    const d = await run("deepgram", [[200, { results: { channels: [{ alternatives: [{ transcript: "good morning" }] }] } }]], { language: "en" });
    expect(d.text).toBe("good morning");
    expect(d.seen[0]!.url).toBe("https://api.deepgram.com/v1/listen?model=nova-3&smart_format=true&language=en");
    expect(h(d.seen[0]!).Authorization).toBe("Token SECRET"); expect(h(d.seen[0]!)["Content-Type"]).toBe("audio/wav"); expect(d.seen[0]!.init.body).toBeInstanceOf(Blob);
    const silent = fake([200, { results: { channels: [{ alternatives: [{ transcript: "" }] }] } }]);
    await expect(transcribeCloud(cloudById("deepgram")!, "SECRET", pcm, { fetchFn: silent.fetchFn })).rejects.toMatchObject({ kind: "no-text" });
    expect(silent.seen[0]!.url).toMatch(/&language=multi$/);
  });
});

describe("json + base64 audio", () => {
  it("Gemini and OpenRouter: verbatim prompt, temperature 0, tidied output", async () => {
    const g = await run("gemini", [[200, { candidates: [{ content: { parts: [{ text: 'Here is the transcript: "Call me at five."\n' }] } }] }]], { model: "gemini-3.8-flash", language: "en" });
    expect(g.text).toBe("Call me at five.");
    expect(g.seen[0]!.url).toBe("https://generativelanguage.googleapis.com/v1beta/models/gemini-3.8-flash:generateContent");
    expect(h(g.seen[0]!)["x-goog-api-key"]).toBe("SECRET");
    const b = json(g.seen[0]!);
    expect(b.generationConfig.temperature).toBe(0);
    expect(b.contents[0].parts[1].inlineData.mimeType).toBe("audio/wav");
    expect(b.contents[0].parts[1].inlineData.data.startsWith("UklGR")).toBe(true);
    expect(b.contents[0].parts[0].text).toContain("verbatim");
    const o = await run("openrouter", [[200, { choices: [{ message: { content: "```\nsee you soon\n```" } }] }]]);
    expect(o.text).toBe("see you soon");
    expect(o.seen[0]!.url).toBe("https://openrouter.ai/api/v1/chat/completions");
    expect(json(o.seen[0]!).messages[0].content[1].input_audio.format).toBe("wav");
  });
  it("tidy strips wrappers only", () => {
    expect(tidy('  "Hello." ')).toBe("Hello.");
    expect(tidy("Transcription: hi there")).toBe("hi there");
    expect(tidy("Sure, here's the transcript:\n“Ok.”")).toBe("Ok.");
    expect(tidy("```text\nabc\n```")).toBe("abc");
    expect(tidy("Note: buy milk")).toBe("Note: buy milk");
    expect(tidy('He said "no" twice')).toBe('He said "no" twice');
  });
});

describe("upload + poll", () => {
  it("AssemblyAI: raw upload, job with speech_models, poll to completed", async () => {
    const r = await run("assemblyai", [[200, { upload_url: "https://cdn.example/u1" }], [200, { id: "t1", status: "queued" }], [200, { id: "t1", status: "processing", text: null }], [200, { id: "t1", status: "completed", text: " done text " }]], { language: "en" });
    expect(r.text).toBe("done text");
    expect(r.seen[0]!.url).toBe("https://api.assemblyai.com/v2/upload"); expect(h(r.seen[0]!).authorization).toBe("SECRET");
    expect(json(r.seen[1]!)).toEqual({ audio_url: "https://cdn.example/u1", speech_models: [cloudById("assemblyai")!.defaultModel, "universal-2"], language_code: "en" });
    expect(r.seen.slice(2).map((s) => `${s.init.method} ${s.url}`)).toEqual(["GET https://api.assemblyai.com/v2/transcript/t1", "GET https://api.assemblyai.com/v2/transcript/t1"]);
    await expect(run("assemblyai", [[200, { upload_url: "u" }], [200, { id: "t1" }], [200, { status: "error" }]])).rejects.toMatchObject({ kind: "audio" });
  });
  it("Soniox: multipart upload, transcript fetch, then deletes what it uploaded", async () => {
    const r = await run("soniox", [[201, { id: "f-1" }], [201, { id: "tr-1", status: "queued" }], [200, { status: "completed" }], [200, { id: "tr-1", text: "soniox text", tokens: [] }], [204, ""], [204, ""]], { language: "fr" });
    expect(r.text).toBe("soniox text");
    expect(form(r.seen[0]!).get("file")).toBeInstanceOf(File);
    expect(json(r.seen[1]!)).toEqual({ file_id: "f-1", model: "stt-async-v6", language_hints: ["fr"] });
    expect(r.seen.slice(3).map((s) => `${s.init.method} ${s.url}`)).toEqual([
      "GET https://api.soniox.com/v1/transcriptions/tr-1/transcript", "DELETE https://api.soniox.com/v1/transcriptions/tr-1", "DELETE https://api.soniox.com/v1/files/f-1",
    ]);
  });
  it("a cancelled poll stops and still cleans up", async () => {
    const ctl = new AbortController(), f = fake([201, { id: "f-1" }], [201, { id: "tr-1" }], [200, { status: "processing" }], [204, ""], [204, ""]);
    const p = transcribeCloud(cloudById("soniox")!, "SECRET", pcm, { fetchFn: f.fetchFn, signal: ctl.signal, sleep: async () => { ctl.abort(); } });
    await expect(p).rejects.toMatchObject({ kind: "cancelled" });
    expect(f.seen.slice(3).map((s) => s.init.method)).toEqual(["DELETE", "DELETE"]);
  });
});

describe("errors and config", () => {
  it("maps statuses and network failures", async () => {
    expect([401, 403, 402, 429, 413, 503, 400].map((st) => mapStatus(st))).toEqual(["auth", "auth", "quota", "rate", "too-long", "server", "audio"]);
    await expect(run("groq", [[401, {}]])).rejects.toMatchObject({ kind: "auth" });
    await expect(transcribeCloud(cloudById("groq")!, "k", pcm, { fetchFn: async () => { throw new TypeError("x"); }, sleep: async () => {} })).rejects.toBeInstanceOf(CloudError);
  });
  it("test key: bad key fails, audio rejection still means the key works", async () => {
    expect(await testKey(cloudById("openai")!, "k", { fetchFn: fake([401, {}]).fetchFn })).toEqual({ ok: false, error: "auth" });
    expect(await testKey(cloudById("openai")!, "k", { fetchFn: fake([400, {}]).fetchFn })).toEqual({ ok: true });
  });
  it("is off until mode, key and consent exist; old configs keep working", () => {
    expect(activeCloud(loadCloud(undefined))).toBeUndefined();
    expect(activeCloud(loadCloud({ provider: "groq", keys: { groq: "k" } }))).toBeUndefined();
    expect(activeCloud(loadCloud({ provider: "groq", keys: { groq: " k " }, consented: ["groq"] }))).toMatchObject({ key: "k", mode: "always", model: "whisper-large-v3-turbo" });
    expect(activeCloud(loadCloud({ provider: "groq", keys: { groq: "k" }, consented: ["groq"], mode: "off" }))).toBeUndefined();
    expect(activeCloud(loadCloud({ provider: "groq", keys: { groq: "k" }, consented: ["groq"], mode: "fallback", models: { groq: "whisper-large-v3" } }))?.model).toBe("whisper-large-v3");
  });
});

describe("shared rules (same as desktop and Android)", () => {
  it("classifies errors from status and body", () => {
    expect(mapStatus(429, '{"error":{"type":"insufficient_quota"}}')).toBe("quota");
    expect(mapStatus(429, '{"error":{"code":"rate_limit_exceeded"}}')).toBe("rate");
    expect(mapStatus(400, '{"error":{"details":[{"reason":"API_KEY_INVALID"}]}}')).toBe("auth");
    expect(mapStatus(401, '{"detail":{"status":"quota_exceeded"}}')).toBe("quota");
    expect([422, 498].map((s) => mapStatus(s))).toEqual(["audio", "server"]);
  });
  it("retries once on rate limit / server / dropped connection, honours a short Retry-After, never retries a bad key", async () => {
    expect(retryAfterMs("rate", "3")).toBe(3000); expect(retryAfterMs("rate", "60")).toBeUndefined();
    expect(retryAfterMs("server", null, () => 999)).toBe(1499); expect(retryAfterMs("auth", "1")).toBeUndefined();
    expect(timeoutFor(60 * 16000)).toBe(150000);
    for (const [st, kind, n] of [[401, "auth", 1], [402, "quota", 1], [400, "audio", 1], [429, "rate", 2], [503, "server", 2]] as const) {
      const f = fake([st, {}], [st, {}]);
      await expect(transcribeCloud(cloudById("groq")!, "k", pcm, { fetchFn: f.fetchFn, sleep: f.sleep })).rejects.toMatchObject({ kind });
      expect(f.seen.length).toBe(n);
    }
    const waits: number[] = []; let calls = 0;
    const flaky = async () => calls++ === 0 ? new Response("{}", { status: 429, headers: { "Retry-After": "2" } }) : new Response('{"text":"ok"}');
    expect(await transcribeCloud(cloudById("groq")!, "k", pcm, { fetchFn: flaky, sleep: async (ms) => { waits.push(ms); } })).toBe("ok");
    expect(waits).toEqual([2000]);
    calls = 0;
    const reset = async () => { if (calls++ === 0) throw new TypeError("reset"); return new Response('{"text":"ok"}'); };
    expect(await transcribeCloud(cloudById("groq")!, "k", pcm, { fetchFn: reset, sleep: async () => {} })).toBe("ok");
    const ctl = new AbortController(); ctl.abort();
    await expect(transcribeCloud(cloudById("groq")!, "k", pcm, { fetchFn: fake().fetchFn, signal: ctl.signal })).rejects.toMatchObject({ kind: "cancelled" });
    await expect(run("gemini", [[200, { candidates: [{ content: { parts: [{ text: "I'm sorry, I cannot transcribe this audio." }] } }] }]])).rejects.toMatchObject({ kind: "no-text" });
  });
  it("validates Azure hosts, refuses non-transcripts, splits at quiet points", () => {
    const t = "{region}.api.cognitive.microsoft.com";
    expect(azureHost(" WestEurope ", t)).toBe("westeurope.api.cognitive.microsoft.com");
    expect(azureHost("https://my-res.cognitiveservices.azure.com/", t)).toBe("my-res.cognitiveservices.azure.com");
    for (const bad of ["", "evil.com", "x.cognitiveservices.azure.com.evil.com", "a/b", "me.cognitiveservices.azure.com:444", "-x"]) expect(azureHost(bad, t)).toBeUndefined();
    expect(transcriptOnly("I'm sorry, but I can't transcribe this audio.")).toBe("");
    expect(transcriptOnly("[silence]")).toBe("");
    expect(transcriptOnly("There is no speech in the audio.")).toBe("");
    expect(transcriptOnly("I'm sorry I can't make it tonight.")).toBe("I'm sorry I can't make it tonight.");
    expect(transcriptOnly('Here is the transcript: "Call me."')).toBe("Call me.");
    const a = new Float32Array(50 * 16000).fill(0.3); a.fill(0, 22 * 16000, 22 * 16000 + 3200);
    const parts = splitAudio(a, 29 * 16000);
    expect(parts.length).toBe(2); expect(parts[0]![1]).toBeGreaterThanOrEqual(22 * 16000); expect(parts[0]![1]).toBeLessThan(22 * 16000 + 3200);
    expect(splitAudio(new Float32Array(100 * 16000).fill(0.1), 29 * 16000).length).toBe(4);
    expect(splitAudio(new Float32Array(16000), 29 * 16000)).toEqual([[0, 16000]]);
  });
});

describe("contract cases from the docs (providers/tests/contract.json)", () => {
  interface Req { method: string; path: string; header: string[]; form?: Record<string, string | null>; query?: Record<string, string>; json?: Record<string, unknown>; raw?: string }
  it("has one case per provider", () => expect(contract.cases.length).toBe(CLOUD.length));
  for (const c of contract.cases) {
    it(c.provider, async () => {
      const f = fake(...(c.replies as [number, unknown][]));
      const text = await transcribeCloud(cloudById(c.provider)!, "SECRET", new Float32Array(16000).fill(0.1), { fetchFn: f.fetchFn, sleep: f.sleep, language: c.lang, region: (c as { region?: string }).region });
      expect(text).toBe(c.text);
      const want = c.requests as Req[];
      expect(f.seen.length).toBe(want.length);
      want.forEach((w, i) => {
        const r = f.seen[i]!, u = new URL(r.url), hs = h(r);
        expect(`${r.init.method} ${u.pathname}`).toBe(`${w.method} ${w.path}`);
        const [hk, hv] = w.header;
        expect(Object.entries(hs).find(([k]) => k.toLowerCase() === hk!.toLowerCase())?.[1]).toBe(hv!.replace("KEY", "SECRET"));
        for (const [k, v] of Object.entries(w.query ?? {})) expect(u.searchParams.get(k)).toBe(v);
        for (const [k, v] of Object.entries(w.form ?? {})) v === null ? expect(form(r).has(k)).toBe(true) : expect(form(r).get(k)).toBe(v);
        for (const [k, v] of Object.entries(w.json ?? {})) expect(json(r)[k]).toEqual(v);
        if (w.raw) { expect(hs["Content-Type"]).toBe(w.raw); expect(r.init.body).toBeInstanceOf(Blob); }
      });
    });
  }
});

describe("live streaming (same rules as desktop and Android)", () => {
  it("maps the chosen batch model to a live model", () => {
    expect(liveModelFor(cloudById("elevenlabs")!, "scribe_v3")).toBe("scribe_v3_live");
    expect(liveModelFor(cloudById("openai")!, "gpt-transcribe")).toBe("gpt-live-transcribe");
    expect(liveModelFor(cloudById("deepgram")!, "nova-3")).toBe("nova-3");
    expect(liveModelFor(cloudById("soniox")!, "stt-async-v6")).toBe("stt-rt-v6");
    for (const p of CLOUD.filter((p) => p.live)) for (const m of [...p.models, ""]) expect(p.live!.models).toContain(liveModelFor(p, m));
  });
  const feed = (id: string, msgs: string[], endSent = false) => { const st = new LiveState(cloudById(id)!.live!); st.ready = true; st.endSent = endSent; for (const m of msgs) st.on(m); return st; };
  it("follows each provider's message rules", () => {
    expect(feed("deepgram", ['{"type":"Results","is_final":true,"channel":{"alternatives":[{"transcript":"yeah so my"}]}}', '{"type":"Results","is_final":false,"channel":{"alternatives":[{"transcript":"credit card"}]}}']).text()).toBe("yeah so my credit card");
    expect(feed("assemblyai", ['{"type":"Turn","end_of_turn":false,"transcript":"my number is five"}', '{"type":"Turn","end_of_turn":true,"transcript":"My number is 555-0134."}', '{"type":"Turn","end_of_turn":false,"transcript":"thanks"}']).text()).toBe("My number is 555-0134. thanks");
    const so = feed("soniox", ['{"tokens":[{"text":"How","is_final":true},{"text":"\'re","is_final":false}]}', '{"tokens":[{"text":"\'re","is_final":true},{"text":" you","is_final":true},{"text":"<fin>","is_final":true}]}', '{"tokens":[],"finished":true}']);
    expect(so.text()).toBe("How're you"); // `finished` ends it too once the registry lists the done rule before tokens (Soniox also closes the socket)
    expect(feed("mistral", ['{"type":"transcription.text.delta","text":"Hel"}', '{"type":"transcription.text.delta","text":"lo there"}']).text()).toBe("Hello there");
    const mi = feed("mistral", ['{"type":"transcription.done","text":"Hello there."}']);
    expect([mi.text(), mi.done]).toEqual(["Hello there.", true]);
    const oa = feed("openai", ['{"type":"conversation.item.input_audio_transcription.delta","item_id":"item_003","delta":"Hello,"}'], true);
    expect(oa.text()).toBe("Hello,");
    oa.on('{"type":"conversation.item.input_audio_transcription.completed","item_id":"item_003","transcript":"Hello, how are you?"}');
    expect([oa.text(), oa.done]).toEqual(["Hello, how are you?", true]);
    const sm = feed("speechmatics", ['{"message":"AddTranscript","metadata":{"transcript":"Hello world."}}', '{"message":"AddPartialTranscript","metadata":{"transcript":"how"}}', '{"message":"EndOfTranscript"}']);
    expect([sm.text(), sm.done]).toEqual(["Hello world. how", true]);
    const ca = feed("cartesia", ['{"type":"transcript","is_final":true,"text":"How are"}', '{"type":"transcript","is_final":true,"text":" you doing today?"}', '{"type":"done"}']);
    expect([ca.text(), ca.done]).toEqual(["How are you doing today?", true]);
    expect(classifyLive(feed("elevenlabs", ['{"message_type":"auth_error","error":"Invalid API key"}']).error!)).toBe("auth");
    expect(CLOUD.filter((p) => p.liveSupported).length).toBeGreaterThanOrEqual(8);
    expect(cloudById("gemini")!.liveSupported).toBe(false); // no browser token path
    expect(resampleFrom16k(new Float32Array([0, 1]), 24000).length).toBe(3);
    expect(originsFor(cloudById("assemblyai")!)).toEqual(["https://api.assemblyai.com/*", "https://token.assemblyai.com/*", "https://streaming.assemblyai.com/*"]);
    expect(activeCloud(loadCloud({ provider: "deepgram", keys: { deepgram: "k" }, consented: ["deepgram"], live: ["deepgram"] }))?.live).toBe(true);
    expect(activeCloud(loadCloud({ provider: "gemini", keys: { gemini: "k" }, consented: ["gemini"], live: ["gemini"] }))?.live).toBe(false);
  });

  /** A fake streaming provider in the browser: answers like Deepgram after CloseStream. */
  class FakeWS {
    static last: FakeWS;
    readyState = 0; binaryType = ""; sent: (string | Uint8Array)[] = [];
    onopen?: () => void; onmessage?: (e: { data: unknown }) => void; onclose?: (e: { code: number; reason: string }) => void; onerror?: () => void;
    constructor(public url: string, public protocols?: string[]) { FakeWS.last = this; setTimeout(() => { this.readyState = 1; this.onopen?.(); this.emit('{"type":"Results","is_final":false,"channel":{"alternatives":[{"transcript":"hel"}]}}'); }, 0); }
    emit(data: string) { this.onmessage?.({ data }); }
    send(d: string | Uint8Array) {
      this.sent.push(d);
      if (d === '{"type":"CloseStream"}') setTimeout(() => { this.emit('{"type":"Results","is_final":true,"channel":{"alternatives":[{"transcript":"hello world"}]}}'); this.emit('{"type":"Metadata"}'); }, 0);
    }
    close() { this.readyState = 3; }
  }

  it("streams through a short-lived token, never the key, in 100 ms frames", async () => {
    const calls: { url: string; init: RequestInit }[] = [];
    const fetchFn = async (url: string, init: RequestInit) => { calls.push({ url, init }); return new Response('{"access_token":"TMP","expires_in":30}'); };
    const shown: string[] = [];
    const live = startLiveCloud(cloudById("deepgram")!, "SECRET", { language: "en", onText: (t) => shown.push(t), fetchFn, WebSocketImpl: FakeWS as unknown as typeof WebSocket });
    for (let i = 0; i < 10; i++) live.feed(new Float32Array(1600).fill(0.1)); // 1 s, fed before the socket opens
    expect(await live.finish()).toBe("hello world");
    expect(shown).toContain("hel");
    expect(calls[0]!.url).toBe("https://api.deepgram.com/v1/auth/grant");
    expect((calls[0]!.init.headers as Record<string, string>).Authorization).toBe("Token SECRET");
    const ws = FakeWS.last;
    expect(ws.protocols).toEqual(["bearer", "TMP"]);
    expect(ws.url).toMatch(/^wss:\/\/api\.deepgram\.com\/v1\/listen\?.*model=nova-3.*encoding=linear16.*sample_rate=16000.*language=en/);
    expect(ws.url + ws.protocols!.join()).not.toContain("SECRET");
    const frames = ws.sent.filter((d) => typeof d !== "string") as Uint8Array[];
    expect(frames.map((f) => f.byteLength)).toEqual(Array(10).fill(3200));
    expect(ws.sent.at(-1)).toBe('{"type":"CloseStream"}');
  });

  it("query tokens, auth errors and cancel", async () => {
    const tok = async () => new Response('{"token":"TMP"}');
    const a = startLiveCloud(cloudById("assemblyai")!, "SECRET", { fetchFn: tok, WebSocketImpl: FakeWS as unknown as typeof WebSocket });
    await new Promise((r) => setTimeout(r, 5));
    expect(FakeWS.last.url).toContain("token=TMP"); expect(FakeWS.last.url).not.toContain("SECRET");
    a.cancel();
    await expect(a.finish()).rejects.toMatchObject({ kind: "cancelled" });
    const bad = startLiveCloud(cloudById("assemblyai")!, "SECRET", { fetchFn: async () => new Response('{"error":"Invalid API key"}', { status: 401 }) });
    await expect(bad.finish()).rejects.toMatchObject({ kind: "auth" });
  });
});
