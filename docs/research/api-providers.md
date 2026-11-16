# BYO-key cloud STT providers (researched 2026-11-10)

Sources: each provider's own docs/pricing pages. Anything not on those pages is marked UNVERIFIED.

## Per provider (short-audio REST)

**ElevenLabs Scribe** - `POST https://api.elevenlabs.io/v1/speech-to-text`, header `xi-api-key`, multipart. Fields: `file`, `model_id=scribe_v2` (required), optional `language_code` (omit = auto). `file_format=pcm_s16le_16` (16 kHz mono raw PCM) lowers latency. Text in top-level `text`. Min 100 ms, <5 GB. Zero retention via `enable_logging=false`, enterprise only. Realtime: `wss://api.elevenlabs.io/v1/speech-to-text/realtime` (`model_id=scribe_v2_realtime`, `audio_format=pcm_16000`), send JSON `{message_type:"input_audio_chunk", audio_base_64, commit, sample_rate}`; receive `partial_transcript` / `committed_transcript`. Browser auth via single-use `token` query param (15 min). $0.22/h batch, $0.39/h realtime. Free tier: UNVERIFIED. Hindi: UNVERIFIED on page read (auto-detect exists).

**Sarvam AI** - `POST https://api.sarvam.ai/speech-to-text`, header `api-subscription-key`, multipart. Fields: `file`, `model=saaras:v4` (default; `saaras:v3` has `mode`), `language_code` (BCP-47 e.g. `hi-IN`, or `unknown` = auto). Response `transcript`, `language_code`. Accepts WAV/MP3/OGG/OPUS/FLAC/M4A/WebM/PCM; best at 16 kHz; REST intended for <30 s (longer: Batch API). English plus 22 Indic languages incl. Hindi, Tamil, Telugu, Bengali, Marathi. The page does not mention "saarika". WebSocket: `wss://api.sarvam.ai/speech-to-text/ws` (header `Api-Subscription-Key`, query `language-code`, `sample_rate=16000`, `input_audio_codec=pcm_s16le`); send `{audio:{data:<b64>,sample_rate,encoding:"audio/wav"}}` and `{type:"flush"}`; receive `type:"data"` with `transcript`. Interim partials: UNVERIFIED. Price Rs 29.40/h (about Rs 0.49/min); Rs 100 free credit at signup. Retention: UNVERIFIED.

**Deepgram** - `POST https://api.deepgram.com/v1/listen?model=nova-3&smart_format=true&language=..`, header `Authorization: Token <key>`, raw body with `Content-Type: audio/wav` (no multipart). Text at `results.channels[0].alternatives[0].transcript`. Max 2 GB. `language=multi` for mixed; auto-detect not described on the page read. Hindi on Nova-3: UNVERIFIED. Streaming exists (`is_final`, `speech_final`, interim results), but URL/auth/sample-rate params were not on the page read (SDK-wrapped). Price $0.0043/min pre-recorded, $0.0077/min streaming (monolingual); $200 free credit, no card.

**OpenAI** - `POST https://api.openai.com/v1/audio/transcriptions`, `Authorization: Bearer`, multipart. Formats mp3, mp4, mpeg, mpga, m4a, wav, webm (FLAC/Opus not listed). 25 MB. The docs now recommend `gpt-transcribe` with a `languages` array (replaces `language`); `whisper-1`, `gpt-4o-transcribe` and `gpt-4o-mini-transcribe` are deprecated, shutting down 2027-02-26. Response `text` (plus detected `languages`). `stream=true` gives SSE `transcript.text.delta` / `.done`. Realtime WS URL and price per minute: UNVERIFIED (pricing page returned 403). Hindi: UNVERIFIED.

**Groq** - `POST https://api.groq.com/openai/v1/audio/transcriptions` (OpenAI-compatible, Bearer, multipart; `/openai/v1/` path confirmed in docs). Models `whisper-large-v3-turbo` ($0.04/h), `whisper-large-v3` ($0.111/h). flac/mp3/mp4/mpeg/mpga/m4a/ogg/wav/webm, 25 MB free tier / 100 MB dev. Optional ISO-639-1 `language`. Response `text` (default `json`). Billed minimum 10 s per request. No realtime WS in docs read. Latency: UNVERIFIED (reputedly very fast).

**AssemblyAI** - async only for REST: `POST /v2/upload` (raw bytes), `POST /v2/transcript {audio_url, language_detection:true}`, poll `GET /v2/transcript/{id}` until `completed`; header `authorization: <key>` (no Bearer); text in `text`. Slow for dictation (upload + job + poll). Streaming is the better fit: WebSocket, `sample_rate=16000`, `speech_model` param; URL/message schema UNVERIFIED. $0.21/h (Universal-3.5 Pro), $0.15/h streaming multilingual (EN, ES, DE, FR, PT, IT only); $50 free credit. Hindi: UNVERIFIED.

**Mistral Voxtral** - `POST https://api.mistral.ai/v1/audio/transcriptions`, model `voxtral-mini-latest`, multipart `file`, optional `language`, response `text`. 13 languages incl. Hindi; up to 3 h per request. Formats and price UNVERIFIED (pricing page has no per-minute figure). Voxtral Realtime exists (sub-200 ms configurable); endpoint UNVERIFIED.

Not researched: Google/Gemini, Azure, Speechmatics, Soniox. Dropped for relevance and effort (Google/Azure need heavier auth/region setup).

## Cross-cutting (all UNVERIFIED unless noted)
- CORS: no provider page read states it. Plan: Chrome extension calls from the service worker with `host_permissions` for the API origin (bypasses CORS). Never call from a content script (page origin, leaks key).
- Latency: no primary-source numbers collected.
- Key storage: desktop = `keyring` crate (OS keyring); Android = Keystore-backed EncryptedSharedPreferences; Chrome = `chrome.storage.local` only, never `.sync`, never exposed to content scripts. Never log keys or put them in URLs.
- Sending 16 kHz mono: WAV is accepted by every provider above, so use WAV (16-bit PCM) everywhere. Skip FLAC/Opus (OpenAI does not list them).

## Recommended abstraction
```rust
trait CloudTranscriber {
    fn id(&self) -> &'static str;
    async fn transcribe(&self, wav_16k_mono: &[u8], lang: Option<&str>) -> Result<String>; // None = auto
    fn streaming(&self) -> Option<&dyn StreamingTranscriber> { None } // optional
}
trait StreamingTranscriber { // push PCM16 chunks, get Partial(String)|Final(String) events
    async fn open(&self, lang: Option<&str>) -> Result<Session>;
}
```
Mirror in Kotlin (`interface CloudTranscriber`) and TS. Map language to each provider's code format (ISO-639-1 vs BCP-47 `hi-IN`).

## Minimal first release
REST only, WAV upload, one request per utterance. Three request shapes cover all: OpenAI-compatible multipart (OpenAI, Groq, Mistral), ElevenLabs multipart, Sarvam multipart, Deepgram raw body. Implement OpenAI-compatible with a base URL and model, so Groq/Mistral cost almost nothing. Add Deepgram, ElevenLabs and Sarvam (Indic) next. Streaming later (ElevenLabs/Sarvam WS are straightforward); skip AssemblyAI REST.

## Table

| Provider | Endpoint (short audio) | Auth | Body | Text field | Streaming WS | Price | Hindi |
|---|---|---|---|---|---|---|---|
| ElevenLabs | /v1/speech-to-text | xi-api-key | multipart | text | yes | $0.22/h | unverified |
| Sarvam | /speech-to-text | api-subscription-key | multipart | transcript | yes | Rs 29.4/h | yes |
| Deepgram | /v1/listen | Token | raw | results.channels[0].alternatives[0].transcript | yes | $0.0043/min | unverified |
| OpenAI | /v1/audio/transcriptions | Bearer | multipart | text | SSE; WS unverified | unverified | unverified |
| Groq | /openai/v1/audio/transcriptions | Bearer | multipart | text | no | $0.04/h turbo | whisper (unverified) |
| AssemblyAI | /v2/transcript (async) | authorization | JSON | text | yes (details unverified) | $0.15-0.21/h | unverified |
| Mistral | /v1/audio/transcriptions | Bearer | multipart | text | yes (unverified) | unverified | yes |
