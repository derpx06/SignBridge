# Cloud STT integration (15 providers, one registry)

Offline stays the default; cloud is opt-in per provider; any failure falls back to the on-device text.

## The registry: `providers/cloud-stt.json`

One file describes every provider for all three surfaces: id, model-first `name`, `maker`, `models` and
`default_model`, `price_note`, `speed_note`, `host` (named in the consent text; Azure's has `{region}`),
`needs_region`, `slow`, `experimental` (+ `notes`), `max_seconds`, and the request: `style`, `url`, `auth`
`[header, prefix]`, extra `headers`/`query`/`fields`, `file_field`, `model_field`, `json_field`, `lang`
(`field`, `format` iso|locale|indic, `auto` value, `list`), `text` (JSON pointer to the transcript) and, for
upload-and-poll, `upload`/`create`/`status`/`result`/`cleanup`. `excluded` lists providers left out, with the
reason the UI shows. Top level: the fixed LLM `prompt` and the `locales` table (en -> en-US) for Azure.

- Desktop: `include_str!` in `engine/crates/app/src/cloud.rs`.
- Android: `core/build.gradle.kts` adds `providers/` as a resource dir, so the file is on the core classpath
  (tests) and in the APK (`cloud-stt.json` at the APK root). No copy, no hand-written table.
- Chrome: `extension/packages/core/src/cloud.ts` imports the JSON; `apps/chrome/manifest.config.ts` builds the
  CSP `connect-src` and the per-provider `optional_host_permissions` from it.

Each surface has a test that parses the registry and checks every provider uses a known style.

## Providers (docs checked 2026-11-13)

| id | name / maker | style | notes |
|---|---|---|---|
| groq | Groq Whisper / Groq | multipart | `whisper-large-v3-turbo`, $0.04/h, 10 s min |
| openai | GPT Transcribe / OpenAI | multipart | `gpt-transcribe`, `gpt-4o-mini-transcribe` |
| gemini | Gemini Flash / Google | json_audio | `generateContent`, `x-goog-api-key`, `inlineData`, temperature 0 (ignored on 3.6+) |
| deepgram | Nova-3 / Deepgram | raw | `Token` auth, `language=multi` when automatic |
| elevenlabs | Scribe / ElevenLabs | multipart | `model_id` scribe_v2/scribe_v3, `xi-api-key` |
| mistral | Voxtral Mini Transcribe / Mistral | multipart | `voxtral-mini-latest` |
| assemblyai | Universal-3.5 Pro / AssemblyAI | upload_poll | raw upload, `speech_models`, `authorization: <key>` |
| azure | Fast Transcription / Microsoft Azure | multipart | `audio` + `definition` JSON, `Ocp-Apim-Subscription-Key`, region host, api-version 2025-10-15 |
| speechmatics | Speechmatics Enhanced | multipart | `data_file` + `config`, `?wait=25&format=txt`; **experimental** |
| sarvam | Saaras v4 / Sarvam AI | multipart | `transcript` field, refused locally at 30 s |
| together | Whisper Large v3 / Together AI | multipart | `language=auto` when automatic; host `api.together.ai` |
| lemonfox, lemonfoxeu | Lemonfox Whisper (EU) | multipart | EU host `eu-api.lemonfox.ai` (+20%); two ids, two keys |
| cartesia | Ink Whisper / Cartesia | multipart | `Cartesia-Version: 2026-08-14`, Bearer |
| soniox | Soniox v6 / Soniox | upload_poll | files -> transcriptions -> poll -> transcript, then deletes both (field names from docs.soniox.com/openapi.yaml) |
| openrouter | OpenRouter audio models | json_audio | chat/completions `input_audio`; **experimental** |

Excluded: Amazon Transcribe (S3 or SigV4 streaming), Google Cloud STT (OAuth / service account), Fireworks
(docs 404), IBM Watson (legacy), Rev.ai and Gladia (async only, add nothing).

Unconfirmed, hence `experimental`: Speechmatics' `wait` cap and the waited response shape (`{status:"done", txt}`
is from the schema, not an example) and `language: "auto"`; OpenRouter's Chat Completions audio input (the docs
show it on the Responses API) and which model ids accept audio. Also unverified: Azure's regional host
(`<region>.api.cognitive.microsoft.com`; the docs' example uses the resource host, which the region field also
accepts), Cartesia's language when automatic (the field defaults to `en`, so it is left out), and prices marked
"See provider pricing". Every batch endpoint was probed live on 2026-11-13 with an invalid key (`voicio cloud-check --probe`): all 16 answered with their own auth error (401/403, Gemini 400 API_KEY_INVALID), so URL, method, auth header and body shape are accepted. Contract tests replay the docs' own example bodies (`providers/tests/contract.json`) on all three surfaces.

## The five adapters (same on every surface)

1. **multipart**: file + model field + static fields + language field + optional JSON field (`{lang}`/`{model}`
   substituted) + optional query. 11 providers.
2. **raw**: the WAV as the body, options and language in the query (Deepgram).
3. **json_audio**: base64 WAV + the registry's "transcribe verbatim, output only the transcript" prompt at
   temperature 0, Gemini (`contents/parts/inlineData`) or chat (`messages/content/input_audio`) body; output passed
   through `tidy` (drops a code fence, a "Here is the transcript:" preamble and wrapping quotes).
4. **upload_poll**: upload, create the job, poll every 400 ms (25 s limit, cancellable), fetch the result,
   best-effort DELETE of what was uploaded (Soniox). Labelled "Slower (1-3 s)".
5. **excluded**: reason only.

Keys go only in headers, never in a URL or a log line; every test asserts no request URL contains the key.

## API used by the UIs

- Rust (`cloud.rs`): `providers() -> &'static [Provider]` (pub `id, name, maker, models, default_model,
  price_note, speed_note, host, docs, needs_region, slow, experimental, notes, max_seconds`), `excluded()`,
  `provider(id)`, `Provider::host_for(region)`, `Provider::disclosure(region)`,
  `test_key(id, key, model, region) -> Result<(), CloudError>`, `save_key/load_key/delete_key(id, ..)` (0600 file
  `<config_dir>/cloud/<id>.key`), `transcribe(..)`, `transcribe_saved(..)`. Config (`config.rs`):
  `cloud_provider`, `cloud_model` ("" = default), `cloud_mode` ("off" | "always" | "fallback"), `cloud_region`,
  `cloud_consented` (the consented provider id; `cloud_active()` needs mode on and consent == provider). Migration
  `cloud-mode`: an old chosen + consented provider becomes `cloud_mode = "always"`.
- Kotlin: `CloudProviders.all: List<CloudProvider>` (`id, name, maker, models, defaultModel, priceNote, speedNote,
  host, needsRegion, slow, experimental, notes, docs, maxSeconds`, `hostFor(region)`), `CloudProviders.excluded`,
  `CloudClient.test(provider, key, model, region): String?` (null = works), `CloudClient.configured(..)`.
  `FlowSettings.cloudProvider/cloudModel/cloudMode/cloudRegion` + the consent set; keys stay in `CloudKeyStore`.
  `CloudProvider.entries/OpenAI/label` remain as deprecated aliases until the Android UI moves over.
- TS (`@voicio/core`): `CLOUD` (same fields, camelCase), `CLOUD_EXCLUDED`, `cloudById`, `hostFor`, `originFor`,
  `cloudHosts`, `testKey(p, key, {model, region})`, `transcribeCloud(p, key, pcm, {model, region, language,
  signal})`; `CloudConfig` gains `mode`, `models` (per provider) and `region` (old configs: a chosen provider
  means "always").

## Behaviour

- `always`: on-device recognition runs as before; the whole recording also goes to the provider, and its text
  replaces the local one. `fallback`: the cloud is used only when no on-device speech model is installed.
- Cloud text takes the same path as local text: desktop sends it to the worker (`ToWorker::Clean`), which applies
  the spoken directive ("make it formal", "as a list") and the cleanup model or rules exactly like a dictation,
  then the dictionary; Android runs `Pipeline.process` (cleanup, voice commands, dictionary, optional AI polish);
  Chrome runs `runPipeline`. A cloud-only dictation on desktop (no worker) gets the dictionary only.
- Any cloud error keeps the on-device text. With no on-device model (cloud only) the error is shown instead.
- Chrome sends from the offscreen document, after the user grants the provider's host permission, so CORS does
  not apply.

## Robustness (identical rules on all three surfaces)

- Errors from status + body: 401/403 key rejected (or no credit when the body says quota/credit/balance), 402 no
  credit, 429 rate limited (no credit for `insufficient_quota`/credit), 413 too long, 400 with an API-key error
  (Gemini) key rejected, other 4xx bad audio, 5xx/498 provider down, network/timeout offline, plus no text,
  cancelled, needs region. Same user-facing texts (`CloudError` Display / `CloudErrorKind.message` / `CLOUD_ERROR_TEXT`).
- One retry with 0.5-1.5 s jitter on 429/5xx/dropped connection; `Retry-After` up to 10 s is honoured, longer is not
  waited; never on 4xx auth or a timeout. Per-request deadline 30 s + 2 s per audio second; polls the same.
- `max_seconds` is the per-request cap (duration limit, or upload size at 32 kB/s); longer audio is split at the
  quietest 100 ms in the second half of each stretch and the texts joined (Sarvam: 29 s pieces).
- Azure region: a region name or a host under `.cognitiveservices.azure.com` / `.api.cognitive.microsoft.com`
  (https:// and / stripped); anything else is refused.
- LLM transcribers: `tidy`, then an answer about the audio ("I'm sorry, I can't transcribe...", "[silence]") is no
  text, so the on-device text is used.
- Cancel stops between requests, retry waits and polls (Android also cuts an in-flight request; Chrome aborts fetch).

## Live (streaming) transcription

`live` in the registry (9 providers: Deepgram, AssemblyAI, ElevenLabs, OpenAI (experimental), Gemini (experimental,
desktop/Android only), Soniox, Speechmatics, Cartesia, Mistral). Sarvam, Together and Azure have realtime APIs but no
short-lived browser token or a documented raw protocol, so they are left out; Groq, Lemonfox, OpenRouter have none.
One adapter per surface drives it: connect (native: key in a header; Chrome: the provider's short-lived token,
in the query or a subprotocol, never the key), optional start message and ready gate, 100 ms PCM16 frames (binary or
a JSON template with base64), message rules (partial / delta / final / tokens / result / error / done), the end
message (`{seq}` = frames sent), then the provider's done signal, 2 s of quiet or 10 s. Partials feed the same live
words as on-device partials; the final goes through the normal cleanup path; any failure keeps the on-device text
(cloud-only: the kept recording is sent whole).

UI fields: Rust `Provider::live_supported()`, `provider.live` (`models`, `default_model`, `experimental`, `notes`),
config `cloud_live: Vec<String>` (ids with Live on; `Config::cloud_live_on()`). Kotlin `CloudProvider.liveSupported`,
`FlowSettings.cloudLive`. TS `CloudProvider.liveSupported`, `CloudConfig.live: string[]`, `originsFor(p, region)`.

## Checks

- `voicio cloud-check [--provider ID] [--clip F.wav] [--region R]` with stored keys; `--live` streams the clip in real
  time; `--probe` needs no key. `voicio cloud-key set ID` stores a key from stdin.
- Android: `VOICIO_KEY_<ID>=... ./gradlew :core:test --tests '*CloudLiveTest*'`; `VOICIO_PROBE=1 ... '*CloudProbeTest*'`.
- Chrome: `VOICIO_KEY_<ID>=... node e2e/cloud-live.mjs` (built with `VOICIO_E2E=1`); `VOICIO_PROBE=1 pnpm vitest run
  src/cloud.probe.test.ts` in packages/core.
