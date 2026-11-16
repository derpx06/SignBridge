//! OPTIONAL cloud speech-to-text (bring your own key). Off by default; offline stays the default
//! engine and the automatic fallback: every failure is an `Err`, so the caller keeps the local result.
//! The providers and their request shapes come from the shared registry `providers/cloud-stt.json`
//! (the same file Android and Chrome use); this file implements its five request styles.
//! Keys live in a 0600 file under the config dir (never in config.toml, never in a URL, never logged).
#![allow(dead_code)] // `excluded`, `provider`, `disclosure` are for the Settings UI

use serde::Deserialize;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    fmt,
    io::Write,
    path::PathBuf,
    sync::{atomic::{AtomicBool, Ordering}, OnceLock},
    time::{Duration, Instant},
};

#[derive(Debug, PartialEq)]
pub enum CloudError {
    NoKey,
    /// Azure needs the region (or resource host) of the user's Speech resource.
    NeedsRegion,
    Offline,
    Cancelled,
    KeyRejected,
    NoCredit,
    RateLimited,
    TooLong,
    BadAudio,
    Server(u16),
    BadResponse,
    /// The reply had no transcript (silence, or an LLM that answered with something else).
    NoText,
    Unsupported,
}

/// The same texts on Android (`CloudErrorKind`) and Chrome (`CLOUD_ERROR_TEXT`).
impl fmt::Display for CloudError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::NoKey => f.write_str("No API key is saved for this provider."),
            Self::NeedsRegion => f.write_str("Enter the region (for example westeurope) or the resource host of your Azure Speech resource."),
            Self::Offline => f.write_str("Could not reach the provider (offline or timed out)."),
            Self::Cancelled => f.write_str("Cancelled."),
            Self::KeyRejected => f.write_str("The provider rejected this API key."),
            Self::NoCredit => f.write_str("This account is out of credit or quota."),
            Self::RateLimited => f.write_str("Rate limited by the provider; try again shortly."),
            Self::TooLong => f.write_str("This recording is too long for the provider."),
            Self::BadAudio => f.write_str("The provider could not process the audio."),
            Self::Server(c) => write!(f, "The provider is having problems (HTTP {c})."),
            Self::BadResponse => f.write_str("The provider sent a reply Voicio could not read."),
            Self::NoText => f.write_str("The provider returned no transcript."),
            Self::Unsupported => f.write_str("This provider is not supported."),
        }
    }
}
impl std::error::Error for CloudError {}

// ---- the registry ----------------------------------------------------------------------------------

#[derive(Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
enum Style {
    /// multipart form: the audio file plus configurable fields (OpenAI-compatible, ElevenLabs, Sarvam, Azure, ...)
    Multipart,
    /// the WAV as the request body, options in the query (Deepgram)
    Raw,
    /// JSON with base64 audio and a fixed "transcribe verbatim" prompt (Gemini, OpenRouter)
    JsonAudio,
    /// upload, create a job, poll it (AssemblyAI, Soniox)
    UploadPoll,
}

#[derive(Deserialize, Debug)]
struct Lang {
    #[serde(default)]
    field: String,
    /// "iso" (en), "locale" (en-US, from the registry's table) or "indic" (hi-IN)
    format: String,
    /// What to send when the language is automatic; None = leave the field out.
    auto: Option<String>,
    #[serde(default)]
    list: bool,
}

#[derive(Deserialize, Debug)]
struct JsonField {
    name: String,
    value: String,
    value_auto: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Upload {
    url: String,
    body: String,
    id: String,
}

#[derive(Deserialize, Debug)]
struct Create {
    url: String,
    ref_field: String,
    model_field: String,
    #[serde(default)]
    model_list: bool,
    /// Sent after the chosen model, so the provider can fall back for a language it does not cover.
    model_fallback: Option<String>,
    id: String,
}

#[derive(Deserialize, Debug)]
struct PollStatus {
    url: String,
    field: String,
    done: String,
    failed: String,
}

/// One provider of the shared registry. The public fields are what the Settings UI shows.
#[derive(Deserialize, Debug)]
pub struct Provider {
    pub id: String,
    /// The model's name (model first: "Nova-3"), with `maker` as the subtitle.
    pub name: String,
    pub maker: String,
    pub models: Vec<String>,
    pub default_model: String,
    pub price_note: String,
    pub speed_note: String,
    /// Where the audio goes (named in the consent text). Azure's contains `{region}`: use `host_for`.
    pub host: String,
    pub docs: String,
    pub needs_region: bool,
    /// Upload-then-poll providers: about 1-3 s slower than the others.
    pub slow: bool,
    /// At least one request fact was not confirmed by the provider's docs (see `notes`).
    pub experimental: bool,
    #[serde(default)]
    pub notes: String,
    pub max_seconds: u32,
    style: Style,
    #[serde(default)]
    url: String,
    auth: (String, String),
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    query: BTreeMap<String, String>,
    #[serde(default)]
    file_field: String,
    #[serde(default)]
    model_field: String,
    #[serde(default)]
    fields: BTreeMap<String, String>,
    json_field: Option<JsonField>,
    lang: Lang,
    #[serde(default)]
    body: String,
    /// Gemini's `generationConfig`.
    #[serde(default)]
    generation_config: serde_json::Value,
    upload: Option<Upload>,
    create: Option<Create>,
    status: Option<PollStatus>,
    #[serde(default)]
    result: String,
    #[serde(default)]
    cleanup: Vec<String>,
    text: String,
    /// The provider's streaming WebSocket, if it has one (see `live`).
    pub live: Option<Live>,
}

/// A live (streaming) API from the registry: words arrive while the user speaks.
#[derive(Deserialize, Debug)]
pub struct Live {
    pub models: Vec<String>,
    pub default_model: String,
    pub max_seconds: u32,
    #[serde(default)]
    pub experimental: bool,
    #[serde(default)]
    pub notes: String,
    url: String,
    #[serde(default)]
    query: BTreeMap<String, String>,
    /// The chosen batch model -> its live counterpart.
    #[serde(default)]
    map: BTreeMap<String, String>,
    lang: Option<Lang>,
    /// Native header; None = the provider's batch `auth`.
    auth: Option<(String, String)>,
    /// How a browser connects (a short-lived token); only Chrome uses it, `probe` checks its endpoint.
    browser: Option<LiveBrowser>,
    #[serde(default)]
    start: String,
    start_auto: Option<String>,
    /// Hold the audio until a message matching this arrives.
    ready: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(default = "rate_16k")]
    rate: u32,
    /// "" = binary pcm_s16le frames; otherwise a JSON text template with `{audio}` (base64).
    #[serde(default)]
    audio: String,
    messages: Vec<LiveRule>,
    #[serde(default)]
    join: String,
    /// Sent when the user stops (`{seq}` = audio frames sent); None = just close.
    end: Option<String>,
    /// "final": the first final after `end` means every final has arrived.
    #[serde(default)]
    end_done: String,
}

impl Live {
    /// The live model for the chosen (batch) model: its mapped counterpart, the same id when it is also a
    /// live model, else the live default. Same rule on Android and Chrome.
    pub fn model_for<'a>(&'a self, model: &'a str) -> &'a str {
        match self.map.get(model) {
            Some(m) => m,
            None if self.models.iter().any(|m| m == model) => model,
            None => &self.default_model,
        }
    }
}

fn rate_16k() -> u32 {
    16_000
}

#[derive(Deserialize, Debug)]
struct LiveBrowser {
    token: TokenSpec,
}

#[derive(Deserialize, Debug)]
struct TokenSpec {
    url: String,
    #[serde(default)]
    body: String,
}

#[derive(Deserialize, Debug)]
struct LiveRule {
    #[serde(rename = "match")]
    when: BTreeMap<String, serde_json::Value>,
    #[serde(rename = "do")]
    action: String,
    #[serde(default)]
    text: String,
}

/// A provider left out on purpose, with the reason the UI shows.
#[derive(Deserialize, Debug)]
pub struct Excluded {
    pub id: String,
    pub name: String,
    pub reason: String,
}

#[derive(Deserialize)]
struct Registry {
    prompt: String,
    locales: HashMap<String, String>,
    /// For `lang.format` "name" (Lemonfox): "en" -> "english".
    language_names: HashMap<String, String>,
    providers: Vec<Provider>,
    excluded: Vec<Excluded>,
}

fn registry() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(|| serde_json::from_str(include_str!("../../../../providers/cloud-stt.json")).expect("providers/cloud-stt.json is valid (checked by a test)"))
}

/// Every supported provider, in the registry's order.
pub fn providers() -> &'static [Provider] {
    &registry().providers
}

/// Providers deliberately not supported, with the reason.
pub fn excluded() -> &'static [Excluded] {
    &registry().excluded
}

pub fn provider(id: &str) -> Option<&'static Provider> {
    providers().iter().find(|p| p.id == id)
}

impl Provider {
    /// The host the audio goes to; for Azure `region` is a region ("westeurope") or the resource's host
    /// or endpoint ("https://me.cognitiveservices.azure.com/"). An unusable Azure value shows as typed.
    pub fn host_for(&self, region: &str) -> String {
        if !self.needs_region {
            return self.host.clone();
        }
        azure_host(region, &self.host).unwrap_or_else(|| self.host.replace("{region}", region.trim()))
    }

    /// Show this and get an explicit opt-in before ever enabling the provider.
    pub fn disclosure(&self, region: &str) -> String {
        format!("Your audio is sent to {} ({}) over the internet for transcription, under {}'s terms. Voicio does not send audio anywhere unless you turn this on.", self.maker, self.host_for(region), self.maker)
    }

    fn model<'a>(&'a self, model: &'a str) -> &'a str {
        if model.is_empty() { &self.default_model } else { model }
    }

    /// Words can stream in while speaking (desktop: any provider with a `live` API).
    pub fn live_supported(&self) -> bool {
        self.live.is_some()
    }
}

// ---- rules shared with Android (Cloud.kt) and Chrome (cloud.ts): keep the three in step ----------

/// Azure: a region name, or a host under one of Azure's two Speech domains (scheme and "/" ignored).
/// Anything else (another domain, a path, a port) is refused, so audio can only go to Azure.
fn azure_host(region: &str, template: &str) -> Option<String> {
    let r = region.trim().to_ascii_lowercase();
    let r = r.strip_prefix("https://").unwrap_or(&r).trim_end_matches('/');
    let label = |s: &str| !s.is_empty() && s.len() <= 63 && !s.starts_with('-') && !s.ends_with('-') && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if label(r) {
        return Some(template.replace("{region}", r));
    }
    [".cognitiveservices.azure.com", ".api.cognitive.microsoft.com"].iter().find_map(|d| r.strip_suffix(d).filter(|n| label(n)).map(|_| r.to_string()))
}

/// An HTTP failure to the error the user sees. Bodies are only searched, never shown or logged.
fn classify(status: u16, body: &str) -> CloudError {
    let b = body.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| b.contains(w));
    let credit = has(&["insufficient_quota", "insufficient_credit", "credit", "balance", "payment", "billing_hard_limit", "quota_exceeded"]);
    let bad_key = has(&["api_key_invalid", "api key not valid", "invalid api key", "invalid_api_key", "incorrect api key", "invalid_auth", "unauthorized"]);
    match status {
        402 => CloudError::NoCredit,
        401 | 403 if credit => CloudError::NoCredit,
        401 | 403 => CloudError::KeyRejected,
        429 if credit => CloudError::NoCredit,
        429 => CloudError::RateLimited,
        413 => CloudError::TooLong,
        400 if bad_key => CloudError::KeyRejected, // Gemini: 400 API_KEY_INVALID
        408 => CloudError::Offline,
        498 | 500..=599 => CloudError::Server(status), // 498: Groq flex tier at capacity
        _ => CloudError::BadAudio,
    }
}

/// Retry once after this long (Retry-After when it is a few seconds, else 0.5-1.5 s of jitter), or not at
/// all: only rate limits, server errors and dropped connections are retried; a long Retry-After is not waited.
fn retry_after(e: &CloudError, header: Option<&str>) -> Option<Duration> {
    if !matches!(e, CloudError::RateLimited | CloudError::Server(_) | CloudError::Offline) {
        return None;
    }
    match header.and_then(|h| h.trim().parse::<u64>().ok()) {
        Some(s) if s <= MAX_RETRY_AFTER_S => Some(Duration::from_secs(s)),
        Some(_) => None,
        None => {
            use std::hash::{BuildHasher, Hasher};
            Some(Duration::from_millis(500 + std::collections::hash_map::RandomState::new().build_hasher().finish() % 1000))
        }
    }
}

/// Per request: 30 s plus 2 s per second of audio (covers a slow upload of the WAV, 32 kB per second).
fn timeout_for(samples: usize) -> Duration {
    Duration::from_millis(30_000 + samples as u64 * 2_000 / 16_000)
}

/// Splits a recording longer than `max` samples at its quietest 100 ms in the second half of each
/// stretch, so no word is cut in two when a provider caps the length (Sarvam: 30 s per request).
pub fn split(pcm: &[f32], max: usize) -> Vec<std::ops::Range<usize>> {
    const W: usize = 1_600; // 100 ms
    const STEP: usize = 800;
    let (mut out, mut start) = (Vec::new(), 0);
    while max > W && pcm.len() - start > max {
        let energy = |at: usize| pcm[at..at + W].iter().map(|x| x * x).sum::<f32>();
        let best = (start + max / 2..=start + max - W).step_by(STEP).fold((f32::MAX, start + max - W), |(e, at), i| {
            let ei = energy(i);
            if ei <= e { (ei, i) } else { (e, at) } // ties: the latest, for fewer requests
        });
        let cut = best.1 + W / 2;
        out.push(start..cut);
        start = cut;
    }
    out.push(start..pcm.len());
    out
}

/// LLM transcribers (Gemini, OpenRouter): `tidy`, then refuse an answer that is about the audio rather
/// than a transcript of it ("I'm sorry, I can't transcribe this audio", "[silence]") -> "" (no text).
pub fn transcript_only(s: &str) -> String {
    let t = tidy(s);
    let l = t.to_lowercase();
    let marker = t.len() < 40 && (t.starts_with('[') && t.ends_with(']') || t.starts_with('(') && t.ends_with(')') || t.starts_with('<') && t.ends_with('>'));
    let opener = ["i'm sorry", "i am sorry", "sorry", "i cannot", "i can't", "i can not", "i'm unable", "i am unable", "unfortunately", "as an ai", "there is no", "there's no", "no speech", "the audio", "this audio", "the recording", "this recording", "the provided audio"];
    let about_audio = ["audio", "transcri", "speech", "recording"].iter().any(|w| l.contains(w));
    if marker || (about_audio && opener.iter().any(|o| l.starts_with(o))) { String::new() } else { t }
}

// ---- public entry points ---------------------------------------------------------------------------

/// Mono PCM at `rate` to 16 kHz (box filter: the mean of each output sample's span, a cheap low-pass).
/// ponytail: not a polyphase resampler; fine for speech recognition, replace if accuracy ever matters here.
pub fn resample_16k(pcm: &[f32], rate: u32) -> Vec<f32> {
    if rate == 16_000 || rate == 0 {
        return pcm.to_vec();
    }
    let ratio = rate as f64 / 16_000.0;
    let n = (pcm.len() as f64 / ratio).floor() as usize;
    (0..n)
        .map(|i| {
            let a = (i as f64 * ratio) as usize;
            let b = (((i + 1) as f64 * ratio) as usize).clamp(a + 1, pcm.len());
            pcm[a..b].iter().sum::<f32>() / (b - a) as f32
        })
        .collect()
}

/// One transcription with `key`. `model` "" = the provider's default. `cancel` stops it between
/// requests, polls and retry waits (the agent ignores a late answer anyway). Longer than the provider's
/// per-request cap: split at quiet points, sent in order, texts joined.
pub fn transcribe(id: &str, key: &str, model: &str, region: &str, pcm16k: &[f32], language: Option<&str>, cancel: &AtomicBool) -> Result<String, CloudError> {
    let p = provider(id).ok_or(CloudError::Unsupported)?;
    Call::new(p, key.trim(), p.model(model), region.trim(), None, cancel).run(pcm16k, language)
}

/// For the agent: the saved key, the transcription, a friendly error text (never containing the key).
pub fn transcribe_saved(id: &str, model: &str, region: &str, pcm16k: &[f32], language: Option<&str>, cancel: &AtomicBool) -> Result<String, String> {
    let key = load_key(id).ok_or_else(|| CloudError::NoKey.to_string())?;
    transcribe(id, &key, model, region, pcm16k, language, cancel).map_err(|e| e.to_string())
}

/// Sends 1 s of silence; Ok means the key works. A provider that accepts the key but not the silent
/// audio (400/422), or answers without text, still counts as a working key.
pub fn test_key(id: &str, key: &str, model: &str, region: &str) -> Result<(), CloudError> {
    match transcribe(id, key, model, region, &vec![0.0; 16_000], None, &AtomicBool::new(false)) {
        Ok(_) | Err(CloudError::BadResponse | CloudError::BadAudio | CloudError::NoText) => Ok(()),
        Err(e) => Err(e),
    }
}

/// 16 kHz mono 16-bit PCM WAV, in memory.
pub fn encode_wav(pcm: &[f32]) -> Vec<u8> {
    let data = (pcm.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend(b"RIFF");
    w.extend((36 + data).to_le_bytes());
    w.extend(b"WAVEfmt ");
    w.extend(16u32.to_le_bytes());
    w.extend(1u16.to_le_bytes()); // PCM
    w.extend(1u16.to_le_bytes()); // mono
    w.extend(16_000u32.to_le_bytes());
    w.extend(32_000u32.to_le_bytes());
    w.extend(2u16.to_le_bytes());
    w.extend(16u16.to_le_bytes());
    w.extend(b"data");
    w.extend(data.to_le_bytes());
    for s in pcm {
        w.extend(((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    w
}

/// LLM transcribers (Gemini, OpenRouter) sometimes wrap the text: drop a code fence, a
/// "Here is the transcript:" preamble and surrounding quotes. Same rules on Android and Chrome.
pub fn tidy(s: &str) -> String {
    let mut t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        t = rest.split_once('\n').map_or("", |(_, body)| body).trim_end();
        t = t.strip_suffix("```").unwrap_or(t).trim();
    }
    if let Some(i) = t.find(':').filter(|&i| i < 80) {
        let head = t[..i].to_lowercase();
        if !head.contains('\n') && (head.contains("transcri") || head.starts_with("here is") || head.starts_with("here's")) {
            t = t[i + 1..].trim();
        }
    }
    for (a, b) in [("\"", "\""), ("\u{201c}", "\u{201d}")] {
        if t.len() >= a.len() + b.len() && t.starts_with(a) && t.ends_with(b) {
            t = t[a.len()..t.len() - b.len()].trim();
            break;
        }
    }
    t.to_string()
}

// ---- the five styles -------------------------------------------------------------------------------

const BOUNDARY: &str = "----voicioBoundary7MA4YWxkTrZu0gW";
const POLL_EVERY: Duration = Duration::from_millis(400);
/// A Retry-After longer than this is not waited for: the dictation falls back to the on-device text.
const MAX_RETRY_AFTER_S: u64 = 10;

struct Call<'a> {
    p: &'a Provider,
    key: &'a str,
    model: &'a str,
    region: &'a str,
    /// Tests: replace every URL's scheme and host with this, and skip retry waits.
    base: Option<&'a str>,
    cancel: &'a AtomicBool,
    /// Per-request deadline, sized to the audio being sent.
    timeout: Cell<Duration>,
    /// `cloud-check --probe`: every exchange, for the table (None in normal use).
    trace: Option<RefCell<Vec<Trace>>>,
}

/// One HTTP exchange seen by `probe`: never holds a key (keys are only in headers).
#[derive(Debug, Clone)]
pub struct Trace {
    pub method: String,
    pub url: String,
    /// 0 = no HTTP response (DNS, TLS, connect or timeout: see `body`).
    pub status: u16,
    pub body: String,
}

/// The invalid key the probe sends: real URL, method, headers and body shape, no account involved.
pub const PROBE_KEY: &str = "voicio-probe-invalid";

/// Sends `pcm` (the bench clip) to provider `id` with `PROBE_KEY` and returns every exchange, so a caller can
/// check the provider rejected the key (401/403) rather than the URL (404/405) or the request shape (400/415/422).
pub fn probe(id: &str, region: &str, pcm16k: &[f32]) -> (Result<String, CloudError>, Vec<Trace>) {
    let Some(p) = provider(id) else { return (Err(CloudError::Unsupported), Vec::new()) };
    let cancel = AtomicBool::new(false);
    let mut call = Call::new(p, PROBE_KEY, p.model(""), region, None, &cancel);
    call.trace = Some(RefCell::new(Vec::new()));
    let r = call.run(pcm16k, Some("en"));
    (r, call.trace.map(RefCell::into_inner).unwrap_or_default())
}

/// One failed request: the error, and how long to wait before the single retry (None = do not retry).
struct Failure(CloudError, Option<Duration>);

fn multipart(fields: &[(String, String)], file_field: &str, file: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    for (k, v) in fields {
        let _ = write!(b, "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n");
    }
    let _ = write!(b, "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{file_field}\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n");
    b.extend(file);
    let _ = write!(b, "\r\n--{BOUNDARY}--\r\n");
    b
}

/// Percent-encodes a query value (language codes and model ids are plain, but never trust that).
fn enc(v: &str) -> String {
    v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn agent(timeout: Duration) -> ureq::Agent {
    let b = ureq::Agent::config_builder();
    #[cfg(not(target_os = "linux"))]
    let b = b.tls_config(ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).build());
    b.timeout_connect(Some(Duration::from_secs(10))).timeout_global(Some(timeout)).http_status_as_error(false).build().into()
}

fn pointer_str(v: &serde_json::Value, ptr: &str) -> Result<String, CloudError> {
    v.pointer(ptr).and_then(|t| t.as_str()).map(str::to_string).ok_or(CloudError::BadResponse)
}

impl<'a> Call<'a> {
    fn new(p: &'a Provider, key: &'a str, model: &'a str, region: &'a str, base: Option<&'a str>, cancel: &'a AtomicBool) -> Self {
        Call { p, key, model, region, base, cancel, timeout: Cell::new(timeout_for(0)), trace: None }
    }

    /// The language value to send, already in the provider's format; None = leave it out.
    fn lang(&self, language: Option<&str>) -> Option<String> {
        let l = &self.p.lang;
        let chosen = language.map(str::trim).filter(|v| !v.is_empty() && *v != "auto" && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        let value = chosen.and_then(|v| {
            let base = v.split('-').next().unwrap_or(v).to_ascii_lowercase();
            match l.format.as_str() {
                "locale" if v.contains('-') => Some(v.to_string()),
                "locale" => registry().locales.get(&base).cloned(),
                "indic" if v.contains('-') => Some(v.to_string()),
                "indic" => Some(format!("{base}-IN")),
                "name" => registry().language_names.get(&base).cloned(),
                _ => Some(base), // ISO 639-1: "en-US" -> "en"
            }
        });
        value.or_else(|| l.auto.clone())
    }

    fn url(&self, template: &str, id: &str, file: &str) -> Result<String, CloudError> {
        let p = self.p;
        let mut url = template.replace("{model}", self.model).replace("{id}", &enc(id)).replace("{file}", &enc(file));
        if p.needs_region {
            let host = azure_host(self.region, &p.host).ok_or(CloudError::NeedsRegion)?;
            url = url.replace(&p.host, &host);
        }
        Ok(match self.base {
            Some(base) => format!("{base}{}", url.splitn(4, '/').nth(3).map_or(String::new(), |path| format!("/{path}"))),
            None => url,
        })
    }

    /// Waits `d` unless cancelled meanwhile (false).
    fn pause(&self, d: Duration) -> bool {
        let d = if self.base.is_some() { Duration::ZERO } else { d };
        let start = Instant::now();
        while start.elapsed() < d {
            if self.cancel.load(Ordering::Relaxed) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        !self.cancel.load(Ordering::Relaxed)
    }

    /// One request, retried once on a rate limit, server error or dropped connection (not on a timeout,
    /// and never on a rejected key). Clean-up DELETEs are not retried and are sent even when cancelled.
    fn send(&self, method: &str, url: &str, content_type: Option<&str>, body: &[u8]) -> Result<serde_json::Value, CloudError> {
        let mut retried = false;
        loop {
            if method != "DELETE" && self.cancel.load(Ordering::Relaxed) {
                return Err(CloudError::Cancelled);
            }
            match self.send_once(method, url, content_type, body) {
                Ok(v) => return Ok(v),
                Err(Failure(_, Some(wait))) if !retried && method != "DELETE" => {
                    retried = true;
                    if !self.pause(wait) {
                        return Err(CloudError::Cancelled);
                    }
                }
                Err(Failure(e, _)) => return Err(e),
            }
        }
    }

    fn send_once(&self, method: &str, url: &str, content_type: Option<&str>, body: &[u8]) -> Result<serde_json::Value, Failure> {
        let auth = (self.p.auth.0.as_str(), format!("{}{}", self.p.auth.1, self.key));
        let agent = agent(self.timeout.get());
        // never include ureq's error text anywhere: it can echo the URL or headers
        let trace = |status: u16, body: &str| {
            if let Some(t) = &self.trace {
                let body: String = body.replace(self.key, "<key>").chars().take(300).collect();
                t.borrow_mut().push(Trace { method: method.into(), url: url.into(), status, body });
            }
        };
        let net = |e: ureq::Error| {
            trace(0, &e.to_string());
            match e {
            ureq::Error::Timeout(_) => Failure(CloudError::Offline, None),
            _ => {
                let e = CloudError::Offline;
                let wait = retry_after(&e, None);
                Failure(e, wait)
            }
        }};
        let mut resp = if method == "POST" {
            let mut r = agent.post(url).header(auth.0, &auth.1);
            for (k, v) in &self.p.headers {
                r = r.header(k, v);
            }
            if let Some(ct) = content_type {
                r = r.header("Content-Type", ct);
            }
            r.send(body).map_err(net)?
        } else {
            let mut r = if method == "DELETE" { agent.delete(url) } else { agent.get(url) }.header(auth.0, &auth.1);
            for (k, v) in &self.p.headers {
                r = r.header(k, v);
            }
            r.call().map_err(net)?
        };
        let code = resp.status().as_u16();
        let header = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).map(str::to_string);
        let text = resp.body_mut().with_config().limit(1 << 20).read_to_string();
        trace(code, text.as_deref().unwrap_or(""));
        if !(200..300).contains(&code) {
            let e = classify(code, text.as_deref().unwrap_or(""));
            let wait = retry_after(&e, header.as_deref());
            return Err(Failure(e, wait));
        }
        let text = text.map_err(|_| Failure(CloudError::Offline, None))?;
        if method == "DELETE" || text.trim().is_empty() {
            return Ok(serde_json::Value::Null);
        }
        serde_json::from_str(&text).map_err(|_| Failure(CloudError::BadResponse, None))
    }

    fn run(&self, pcm: &[f32], language: Option<&str>) -> Result<String, CloudError> {
        if self.key.is_empty() {
            return Err(CloudError::NoKey);
        }
        let mut texts = Vec::new();
        for part in split(pcm, self.p.max_seconds as usize * 16_000) {
            self.timeout.set(timeout_for(part.len()));
            texts.push(self.run_one(&pcm[part], language)?);
        }
        let text = texts.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
        if text.is_empty() { Err(CloudError::NoText) } else { Ok(text) }
    }

    fn run_one(&self, pcm: &[f32], language: Option<&str>) -> Result<String, CloudError> {
        let p = self.p;
        let wav = encode_wav(pcm);
        let lang = self.lang(language);
        match p.style {
            Style::Multipart => {
                let mut fields: Vec<(String, String)> = Vec::new();
                if !p.model_field.is_empty() {
                    fields.push((p.model_field.clone(), self.model.to_string()));
                }
                fields.extend(p.fields.iter().map(|(k, v)| (k.clone(), v.clone())));
                if let (Some(l), false) = (&lang, p.lang.field.is_empty()) {
                    fields.push((p.lang.field.clone(), l.clone()));
                }
                if let Some(j) = &p.json_field {
                    let value = match (&lang, &j.value_auto) {
                        (None, Some(auto)) => auto.clone(),
                        _ => j.value.replace("{lang}", lang.as_deref().unwrap_or("")),
                    };
                    fields.push((j.name.clone(), value.replace("{model}", self.model)));
                }
                let mut url = self.url(&p.url, "", "")?;
                for (i, (k, v)) in p.query.iter().enumerate() {
                    url.push_str(&format!("{}{k}={}", if i == 0 && !url.contains('?') { '?' } else { '&' }, enc(v)));
                }
                let v = self.send("POST", &url, Some(&format!("multipart/form-data; boundary={BOUNDARY}")), &multipart(&fields, &p.file_field, &wav))?;
                pointer_str(&v, &p.text).map(|t| t.trim().to_string())
            }
            Style::Raw => {
                let mut url = self.url(&p.url, "", "")?;
                let mut q: Vec<(String, String)> = p.query.iter().map(|(k, v)| (k.clone(), v.replace("{model}", self.model))).collect();
                if let Some(l) = &lang {
                    q.push((p.lang.field.clone(), l.clone()));
                }
                for (i, (k, v)) in q.iter().enumerate() {
                    url.push_str(&format!("{}{k}={}", if i == 0 { '?' } else { '&' }, enc(v)));
                }
                let v = self.send("POST", &url, Some("audio/wav"), &wav)?;
                pointer_str(&v, &p.text).map(|t| t.trim().to_string())
            }
            Style::JsonAudio => {
                use base64::Engine as _;
                let data = base64::engine::general_purpose::STANDARD.encode(&wav);
                let mut prompt = registry().prompt.clone();
                if let Some(l) = &lang {
                    prompt.push_str(&format!(" The speech is in the language with code {l}."));
                }
                let body = if p.body == "gemini" {
                    serde_json::json!({
                        "contents": [{ "role": "user", "parts": [{ "text": prompt }, { "inlineData": { "mimeType": "audio/wav", "data": data } }] }],
                        "generationConfig": p.generation_config,
                    })
                } else {
                    serde_json::json!({
                        "model": self.model,
                        "temperature": 0,
                        "messages": [{ "role": "user", "content": [{ "type": "text", "text": prompt }, { "type": "input_audio", "input_audio": { "data": data, "format": "wav" } }] }]
                    })
                };
                let v = self.send("POST", &self.url(&p.url, "", "")?, Some("application/json"), body.to_string().as_bytes())?;
                pointer_str(&v, &p.text).map(|t| transcript_only(&t))
            }
            Style::UploadPoll => {
                let (Some(up), Some(create), Some(status)) = (&p.upload, &p.create, &p.status) else { return Err(CloudError::Unsupported) };
                let uploaded = if up.body == "raw" {
                    self.send("POST", &self.url(&up.url, "", "")?, Some("application/octet-stream"), &wav)?
                } else {
                    self.send("POST", &self.url(&up.url, "", "")?, Some(&format!("multipart/form-data; boundary={BOUNDARY}")), &multipart(&[], "file", &wav))?
                };
                let file = pointer_str(&uploaded, &up.id)?;
                let job = (|| {
                    let mut body = serde_json::Map::new();
                    body.insert(create.ref_field.clone(), file.clone().into());
                    let mut models = vec![self.model];
                    if let Some(f) = create.model_fallback.as_deref().filter(|f| *f != self.model) {
                        models.push(f);
                    }
                    body.insert(create.model_field.clone(), if create.model_list { serde_json::json!(models) } else { self.model.into() });
                    if let Some(l) = &lang {
                        body.insert(p.lang.field.clone(), if p.lang.list { serde_json::json!([l]) } else { l.clone().into() });
                    }
                    let created = self.send("POST", &self.url(&create.url, "", "")?, Some("application/json"), serde_json::Value::Object(body).to_string().as_bytes())?;
                    pointer_str(&created, &create.id)
                })();
                let outcome = match &job {
                    Ok(id) => self.poll(status, id, &file),
                    Err(e) => Err(if *e == CloudError::Cancelled { CloudError::Cancelled } else { CloudError::BadResponse }),
                };
                // Best effort: the provider keeps nothing once the text is back (or the job failed).
                self.timeout.set(timeout_for(0));
                for c in p.cleanup.iter().filter(|c| job.is_ok() || !c.contains("{id}")) {
                    if let Ok(url) = self.url(c, job.as_deref().unwrap_or(""), &file) {
                        let _ = self.send("DELETE", &url, None, &[]);
                    }
                }
                job?;
                outcome
            }
        }
    }

    /// Polls every 400 ms until the job is done, failed or cancelled, or the deadline (30 s plus the
    /// audio's length) has passed.
    fn poll(&self, status: &PollStatus, id: &str, file: &str) -> Result<String, CloudError> {
        let limit = self.timeout.get();
        let start = Instant::now();
        loop {
            let v = self.send("GET", &self.url(&status.url, id, file)?, None, &[])?;
            let state = pointer_str(&v, &status.field)?;
            if state == status.done {
                let v = if self.p.result.is_empty() { v } else { self.send("GET", &self.url(&self.p.result, id, file)?, None, &[])? };
                return pointer_str(&v, &self.p.text).map(|t| t.trim().to_string());
            }
            if state == status.failed {
                return Err(CloudError::BadAudio);
            }
            if start.elapsed() > limit {
                return Err(CloudError::Offline);
            }
            if !self.pause(POLL_EVERY) {
                return Err(CloudError::Cancelled);
            }
        }
    }
}

// ---- live (streaming) transcription -----------------------------------------------------------------

/// What one streamed dictation has heard so far. The same rules on Android and Chrome.
#[derive(Default)]
struct LiveState {
    finals: String,
    partial: String,
    ready: bool,
    end_sent: bool,
    done: bool,
    error: Option<String>,
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn matches(v: &serde_json::Value, when: &BTreeMap<String, serde_json::Value>) -> bool {
    when.iter().all(|(ptr, want)| match v.pointer(ptr) {
        None | Some(serde_json::Value::Null) => false,
        Some(got) => want == "*" || got == want,
    })
}

impl LiveState {
    fn append(&mut self, join: &str, t: &str) {
        if !self.finals.is_empty() && !t.is_empty() {
            self.finals.push_str(join);
        }
        self.finals.push_str(t);
    }

    /// One server message. True = the text shown changed.
    fn on(&mut self, live: &Live, msg: &str) -> bool {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(msg) else { return false };
        let before = self.text(live);
        if let Some(r) = &live.ready {
            self.ready |= matches(&v, r);
        }
        let Some(rule) = live.messages.iter().find(|r| matches(&v, &r.when)) else { return false };
        let text = || v.pointer(&rule.text).map(|t| t.as_str().map_or_else(|| t.to_string(), str::to_string)).unwrap_or_default();
        match rule.action.as_str() {
            "partial" => self.partial = text(),
            "delta" => self.partial.push_str(&text()),
            "final" => {
                self.append(&live.join, &text());
                self.partial.clear();
                self.done |= self.end_sent && live.end_done == "final";
            }
            "tokens" => {
                let mut partial = String::new();
                for t in v.pointer(&rule.text).and_then(|t| t.as_array()).into_iter().flatten() {
                    let s = t["text"].as_str().unwrap_or("");
                    if s.starts_with('<') && s.ends_with('>') {
                        continue; // <fin>, <end> markers
                    }
                    if t["is_final"] == true { self.finals.push_str(s) } else { partial.push_str(s) }
                }
                self.partial = partial;
            }
            "result" => {
                self.finals = text();
                self.partial.clear();
                self.done = true;
            }
            "done" => self.done = true,
            "error" => self.error = Some(if rule.text.is_empty() { msg.to_string() } else { text() }),
            _ => {}
        }
        self.text(live) != before
    }

    fn text(&self, live: &Live) -> String {
        let sep = if self.finals.is_empty() || self.partial.is_empty() { "" } else { live.join.as_str() };
        collapse(&format!("{}{sep}{}", self.finals, self.partial))
    }
}

/// An error message from a live stream to the error the user sees.
fn classify_live(msg: &str) -> CloudError {
    let auth = ["authenticat", "not_authorised", "not authorized", "invalid token", "1008", "4001"].iter().any(|w| msg.to_lowercase().contains(w));
    match classify(400, msg) {
        CloudError::KeyRejected => CloudError::KeyRejected,
        _ if auth => CloudError::KeyRejected,
        _ if ["quota", "credit", "balance", "insufficient", "payment"].iter().any(|w| msg.to_lowercase().contains(w)) => CloudError::NoCredit,
        _ if msg.to_lowercase().contains("rate") => CloudError::RateLimited,
        _ => CloudError::BadResponse,
    }
}

/// 16 kHz to `rate` (24 kHz for OpenAI) by linear interpolation.
fn resample_from_16k(pcm: &[f32], rate: u32) -> Vec<f32> {
    if rate == 16_000 || pcm.is_empty() {
        return pcm.to_vec();
    }
    let n = pcm.len() * rate as usize / 16_000;
    (0..n).map(|i| {
        let x = i as f64 * 16_000.0 / rate as f64;
        let (a, f) = (x as usize, (x - x.floor()) as f32);
        pcm[a] * (1.0 - f) + pcm[(a + 1).min(pcm.len() - 1)] * f
    }).collect()
}

trait Rw: std::io::Read + std::io::Write {}
impl<T: std::io::Read + std::io::Write> Rw for T {}

/// A failed live session: the error, and what the server said (for `probe`; never contains the key).
struct LiveFail(CloudError, String);

/// Streams 16 kHz mono audio from `audio` (closed by the caller when the user stops) to provider `id` and
/// returns the final text; `on_text` gets the words so far whenever they change. Runs on the caller's thread.
pub fn live(id: &str, key: &str, model: &str, region: &str, language: Option<&str>, audio: std::sync::mpsc::Receiver<Vec<f32>>, cancel: &AtomicBool, on_text: impl FnMut(&str)) -> Result<String, CloudError> {
    let p = provider(id).ok_or(CloudError::Unsupported)?;
    live_run(p, key.trim(), model, region.trim(), language, None, audio, cancel, on_text).map_err(|f| f.0)
}

#[allow(clippy::too_many_arguments)]
fn live_run(p: &Provider, key: &str, model: &str, region: &str, language: Option<&str>, base: Option<&str>, audio: std::sync::mpsc::Receiver<Vec<f32>>, cancel: &AtomicBool, mut on_text: impl FnMut(&str)) -> Result<String, LiveFail> {
    use std::sync::mpsc::TryRecvError;
    use tungstenite::{client::IntoClientRequest, Message};
    let fail = |e: CloudError, s: &str| LiveFail(e, s.replace(key, "<key>"));
    let live = p.live.as_ref().ok_or(fail(CloudError::Unsupported, "no live API"))?;
    if key.is_empty() {
        return Err(fail(CloudError::NoKey, ""));
    }
    let model = live.model_for(model);
    // the language, in the provider's format (same rules as batch)
    let lang = live.lang.as_ref().and_then(|l| {
        let chosen = language.map(str::trim).filter(|v| !v.is_empty() && *v != "auto" && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        chosen.map(|v| v.split('-').next().unwrap_or(v).to_ascii_lowercase()).or_else(|| l.auto.clone()).map(|v| (l.field.clone(), v))
    });
    let mut url = live.url.replace("{model}", model);
    let mut q: Vec<(String, String)> = live.query.iter().map(|(k, v)| (k.clone(), v.replace("{model}", model))).collect();
    if let Some((field, v)) = lang.as_ref().filter(|(f, _)| !f.is_empty()) {
        q.push((field.clone(), v.clone()));
    }
    for (i, (k, v)) in q.iter().enumerate() {
        url.push_str(&format!("{}{k}={}", if i == 0 && !url.contains('?') { '?' } else { '&' }, enc(v)));
    }
    if p.needs_region {
        url = url.replace(&p.host, &azure_host(region, &p.host).ok_or(fail(CloudError::NeedsRegion, ""))?);
    }
    if let Some(b) = base {
        url = format!("{b}{}", url.splitn(4, '/').nth(3).map_or(String::new(), |path| format!("/{path}")));
    }
    let lang_v = lang.as_ref().map(|(_, v)| v.as_str());
    let start = match (lang_v, &live.start_auto) {
        (None, Some(auto)) => auto.clone(),
        _ => live.start.replace("{lang}", lang_v.unwrap_or("")),
    }
    .replace("{model}", model);

    // connect: TCP (+ rustls for wss), then the WebSocket handshake with the key in a header
    let mut req = url.as_str().into_client_request().map_err(|e| fail(CloudError::Unsupported, &e.to_string()))?;
    let (h, prefix) = live.auth.as_ref().unwrap_or(&p.auth);
    req.headers_mut().insert(tungstenite::http::HeaderName::from_bytes(h.as_bytes()).map_err(|_| fail(CloudError::Unsupported, "header"))?, format!("{prefix}{key}").parse().map_err(|_| fail(CloudError::KeyRejected, "key is not a valid header value"))?);
    let uri = req.uri().clone();
    let host = uri.host().unwrap_or("").to_string();
    let tls = uri.scheme_str() == Some("wss");
    let port = uri.port_u16().unwrap_or(if tls { 443 } else { 80 });
    let addr = std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), port)).ok().and_then(|mut a| a.next()).ok_or(fail(CloudError::Offline, "DNS lookup failed"))?;
    let tcp = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(10)).map_err(|e| fail(CloudError::Offline, &e.to_string()))?;
    let handle = tcp.try_clone().map_err(|e| fail(CloudError::Offline, &e.to_string()))?;
    handle.set_read_timeout(Some(Duration::from_secs(15))).ok();
    let _ = handle.set_nodelay(true);
    let stream: Box<dyn Rw + Send> = if tls {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|e| fail(CloudError::Offline, &e.to_string()))?
            .with_root_certificates(roots)
            .with_no_client_auth();
        let name = rustls::pki_types::ServerName::try_from(host.clone()).map_err(|e| fail(CloudError::Offline, &e.to_string()))?;
        let conn = rustls::ClientConnection::new(std::sync::Arc::new(config), name).map_err(|e| fail(CloudError::Offline, &e.to_string()))?;
        Box::new(rustls::StreamOwned::new(conn, tcp))
    } else {
        Box::new(tcp)
    };
    let (mut ws, _) = match tungstenite::client::client(req, stream) {
        Ok(ok) => ok,
        Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(resp))) => {
            let status = resp.status().as_u16();
            let body = resp.body().as_deref().map(String::from_utf8_lossy).unwrap_or_default().to_string();
            return Err(fail(classify(status, &body), &format!("HTTP {status} {body}")));
        }
        Err(e) => return Err(fail(CloudError::Offline, &e.to_string())),
    };
    handle.set_read_timeout(Some(Duration::from_millis(20))).ok();

    let mut st = LiveState { ready: live.ready.is_none(), ..LiveState::default() };
    let text_frame = |s: String| Message::Text(s.into());
    if !start.is_empty() {
        ws.send(text_frame(start)).map_err(|e| fail(CloudError::Offline, &e.to_string()))?;
    }
    let frame = live.rate as usize / 10; // 100 ms
    let mut pending: Vec<i16> = Vec::with_capacity(frame * 2);
    let mut held: std::collections::VecDeque<Vec<f32>> = Default::default(); // until `ready`, at most ~2 s
    let (mut seq, mut stopped, mut last_msg) = (0u64, false, Instant::now());
    let mut end_at: Option<Instant> = None;
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = ws.close(None);
            return Err(fail(CloudError::Cancelled, ""));
        }
        // audio in -> 100 ms frames out
        loop {
            match audio.try_recv() {
                Ok(chunk) => held.push_back(chunk),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    stopped = true;
                    break;
                }
            }
        }
        while held.len() > 40 && !st.ready {
            held.pop_front(); // a server that never says "ready" must not grow memory
        }
        if st.ready {
            while let Some(chunk) = held.pop_front() {
                pending.extend(resample_from_16k(&chunk, live.rate).iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0).round() as i16));
                while pending.len() >= frame || (stopped && held.is_empty() && !pending.is_empty()) {
                    let take = pending.len().min(frame);
                    let bytes: Vec<u8> = pending.drain(..take).flat_map(|s| s.to_le_bytes()).collect();
                    let msg = if live.audio.is_empty() {
                        Message::Binary(bytes.into())
                    } else {
                        use base64::Engine as _;
                        text_frame(live.audio.replace("{audio}", &base64::engine::general_purpose::STANDARD.encode(&bytes)))
                    };
                    ws.send(msg).map_err(|e| fail(CloudError::Offline, &e.to_string()))?;
                    seq += 1;
                }
            }
            if stopped && end_at.is_none() && held.is_empty() {
                match &live.end {
                    Some(end) => ws.send(text_frame(end.replace("{seq}", &seq.to_string()))).map_err(|e| fail(CloudError::Offline, &e.to_string()))?,
                    None => {
                        let _ = ws.close(None);
                    }
                }
                st.end_sent = true;
                end_at = Some(Instant::now());
            }
        }
        // messages in
        match ws.read() {
            Ok(Message::Text(t)) => {
                last_msg = Instant::now();
                if st.on(live, t.as_str()) {
                    on_text(&st.text(live));
                }
            }
            Ok(Message::Close(frame)) => {
                if st.error.is_none() && !st.end_sent {
                    let why = frame.map(|f| format!("closed {} {}", u16::from(f.code), f.reason)).unwrap_or_else(|| "closed".into());
                    return Err(fail(classify_live(&why), &why));
                }
                break;
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) if st.end_sent => break,
            Err(e) => return Err(fail(CloudError::Offline, &e.to_string())),
        }
        if let Some(e) = st.error.take() {
            let _ = ws.close(None);
            return Err(fail(classify_live(&e), &e));
        }
        if st.done {
            break;
        }
        if let Some(at) = end_at {
            // all finals: the provider said so, or it went quiet for 2 s, or 10 s passed
            if at.elapsed() > Duration::from_secs(10) || last_msg.elapsed() > Duration::from_secs(2) && at.elapsed() > Duration::from_secs(2) {
                break;
            }
        } else if !st.ready && started.elapsed() > Duration::from_secs(10) {
            return Err(fail(CloudError::Offline, "the provider never became ready"));
        }
    }
    let _ = ws.close(None);
    let _ = ws.flush();
    let text = collapse(&st.text(live));
    if text.is_empty() { Err(fail(CloudError::NoText, "")) } else { Ok(text) }
}

/// Live (WebSocket) probe row for `cloud-check --probe`: the browser token endpoint (if any) and the native
/// WebSocket with `PROBE_KEY`; None = the provider has no live API.
pub fn probe_live(id: &str, region: &str, pcm16k: &[f32]) -> Option<(bool, String)> {
    let p = provider(id)?;
    let live = p.live.as_ref()?;
    let mut out = Vec::new();
    let mut ok = true;
    if let Some(b) = &live.browser {
        // the token request carries the key in the batch auth header (Cartesia's live header differs)
        let (h, prefix) = &p.auth;
        let resp = agent(Duration::from_secs(20)).post(&b.token.url).header(h, &format!("{prefix}{PROBE_KEY}")).header("Content-Type", "application/json");
        let resp = p.headers.iter().fold(resp, |r, (k, v)| r.header(k, v));
        let (status, body) = match resp.send(b.token.body.replace("{model}", &live.default_model).as_bytes()) {
            Ok(mut r) => (r.status().as_u16(), r.body_mut().read_to_string().unwrap_or_default()),
            Err(e) => (0, e.to_string()),
        };
        let (pass, why) = crate::cloud_check::verdict(status, &body);
        ok &= pass;
        out.push(format!("token {} {status} {why} {:?}", if pass { "PASS" } else { "FAIL" }, body.chars().take(80).collect::<String>()));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = tx.send(pcm16k[..pcm16k.len().min(16_000)].to_vec());
    drop(tx);
    let cancel = AtomicBool::new(false);
    let r = live_run(p, PROBE_KEY, "", region, Some("en"), None, rx, &cancel, |_| {});
    let (pass, detail) = match r {
        Ok(t) => (false, format!("accepted an invalid key? {t:?}")),
        Err(LiveFail(CloudError::KeyRejected, d)) => (true, format!("key rejected: {d}")),
        Err(LiveFail(e, d)) => {
            let (pass, why) = d.strip_prefix("HTTP ").and_then(|s| s.split_once(' ')).and_then(|(c, b)| c.parse::<u16>().ok().map(|c| crate::cloud_check::verdict(c, b))).unwrap_or((false, ""));
            (pass, format!("{} {why}: {d}", e))
        }
    };
    ok &= pass;
    out.push(format!("ws {} {}", if pass { "PASS" } else { "FAIL" }, detail.chars().take(160).collect::<String>()));
    Some((ok, format!("{} {}  {}", if ok { "PASS" } else { "FAIL" }, live.url, out.join("  |  "))))
}

// ---- key storage: 0600 file under the config dir (no keyring crate in the lockfile) ----

fn key_path(id: &str) -> Option<PathBuf> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) { return None; }
    directories::ProjectDirs::from("io.github", "voicio", "voicio").map(|d| d.config_dir().join("cloud").join(format!("{id}.key")))
}

pub fn save_key(id: &str, key: &str) -> std::io::Result<()> {
    let p = key_path(id).ok_or(std::io::ErrorKind::InvalidInput)?;
    std::fs::create_dir_all(p.parent().unwrap())?;
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(p)?.write_all(key.trim().as_bytes())
}

pub fn load_key(id: &str) -> Option<String> {
    std::fs::read_to_string(key_path(id)?).ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

pub fn delete_key(id: &str) {
    if let Some(p) = key_path(id) { let _ = std::fs::remove_file(p); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::{Read, Write}, net::TcpListener};

    /// Mock server answering one connection per reply, in order: (base_url, handle yielding the raw requests).
    fn mock(replies: Vec<(u16, String, &'static str)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let h = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (status, reply, extra) in replies {
                let (mut c, _) = l.accept().unwrap();
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                loop {
                    let n = c.read(&mut tmp).unwrap();
                    buf.extend(&tmp[..n]);
                    let s = String::from_utf8_lossy(&buf).to_lowercase();
                    if let Some(i) = s.find("\r\n\r\n") {
                        let cl = s.lines().find_map(|l| l.strip_prefix("content-length: ")).map_or(0, |v| v.trim().parse::<usize>().unwrap());
                        if buf.len() >= i + 4 + cl { break; }
                    }
                    if n == 0 { break; }
                }
                let _ = write!(c, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len());
                seen.push(String::from_utf8_lossy(&buf).into_owned());
            }
            seen
        });
        (base, h)
    }

    fn run_with(id: &str, model: &str, region: &str, replies: &[(u16, &str)], lang: Option<&str>) -> (Result<String, CloudError>, Vec<String>) {
        let (base, h) = mock(replies.iter().map(|(s, r)| (*s, r.to_string(), "")).collect());
        let p = provider(id).unwrap();
        let cancel = AtomicBool::new(false);
        let r = Call::new(p, "SECRET", p.model(model), region, Some(&base), &cancel).run(&[0.1; 16_000], lang);
        let seen = h.join().unwrap();
        for req in &seen {
            let first = req.lines().next().unwrap_or("");
            assert!(!first.contains("SECRET"), "{id}: key in the URL: {first}");
        }
        (r, seen)
    }

    fn run(id: &str, reply: &str, lang: Option<&str>) -> (Result<String, CloudError>, String) {
        let (r, mut seen) = run_with(id, "", "", &[(200, reply)], lang);
        (r, seen.remove(0))
    }

    #[test]
    fn registry_parses_and_every_provider_is_usable() {
        assert_eq!(providers().len(), 16, "15 providers, Lemonfox twice (EU)");
        assert_eq!(excluded().len(), 6);
        for p in providers() {
            assert!(p.models.contains(&p.default_model), "{}", p.id);
            assert!(key_path(&p.id).is_some(), "{}: ids must be plain for the key file", p.id);
            assert!(!p.name.is_empty() && !p.maker.is_empty() && !p.price_note.is_empty() && !p.speed_note.is_empty());
            assert!(p.text.starts_with('/'), "{}", p.id);
            match p.style {
                Style::Multipart => assert!(!p.file_field.is_empty() && !p.url.is_empty()),
                Style::Raw | Style::JsonAudio => assert!(!p.url.is_empty()),
                Style::UploadPoll => assert!(p.upload.is_some() && p.create.is_some() && p.status.is_some()),
            }
            assert!(p.disclosure("westeurope").contains(&p.maker));
        }
        assert_eq!(provider("azure").unwrap().host_for("westeurope"), "westeurope.api.cognitive.microsoft.com");
        assert_eq!(provider("azure").unwrap().host_for("me.cognitiveservices.azure.com"), "me.cognitiveservices.azure.com");
        assert!(provider("nope").is_none());
    }

    #[test]
    fn resampling_to_16k_keeps_duration_and_level() {
        let pcm = vec![0.5f32; 48_000];
        let out = resample_16k(&pcm, 48_000);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-6));
        assert_eq!(resample_16k(&pcm, 16_000).len(), 48_000);
        assert_eq!(resample_16k(&[], 44_100).len(), 0);
    }

    #[test]
    fn wav_header_and_samples() {
        let w = encode_wav(&[0.0, 1.0, -1.0, 2.0]);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()), 36 + 8);
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 16_000);
        assert_eq!(u16::from_le_bytes(w[34..36].try_into().unwrap()), 16);
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 8);
        let s: Vec<i16> = w[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        assert_eq!(s, [0, 32767, -32767, 32767]);
    }

    #[test]
    fn multipart_openai_compatible() {
        for (id, path) in [("groq", "POST /openai/v1/audio/transcriptions "), ("openai", "POST /v1/audio/transcriptions "), ("mistral", "POST /v1/audio/transcriptions "), ("together", "POST /v1/audio/transcriptions ")] {
            let (r, req) = run(id, r#"{"text":" hello world ","languages":["en"]}"#, Some("hi"));
            assert_eq!(r.unwrap(), "hello world", "{id}");
            assert!(req.starts_with(path), "{id}: {req}");
            let low = req.to_lowercase();
            assert!(low.contains("authorization: bearer secret") && low.contains("multipart/form-data; boundary="), "{id}");
            assert!(req.contains("name=\"file\"; filename=\"audio.wav\"") && req.contains("name=\"language\"\r\n\r\nhi\r\n"), "{id}");
        }
        let (_, req) = run("together", r#"{"text":"x"}"#, None);
        assert!(req.contains("name=\"language\"\r\n\r\nauto\r\n") && req.contains("openai/whisper-large-v3"));
        let (_, req) = run("lemonfox", r#"{"text":"x"}"#, None);
        assert!(!req.contains("name=\"model\"") && !req.contains("name=\"language\""));
    }

    #[test]
    fn multipart_variants() {
        let (r, req) = run("elevenlabs", r#"{"language_code":"eng","text":"hi there","words":[]}"#, None);
        assert_eq!(r.unwrap(), "hi there");
        assert!(req.starts_with("POST /v1/speech-to-text ") && req.to_lowercase().contains("xi-api-key: secret"));
        assert!(req.contains(&format!("name=\"model_id\"\r\n\r\n{}\r\n", provider("elevenlabs").unwrap().default_model)) && !req.contains("language_code"));

        let (r, req) = run("cartesia", r#"{"type":"transcript","text":"ink"}"#, Some("de"));
        assert_eq!(r.unwrap(), "ink");
        assert!(req.starts_with("POST /stt ") && req.to_lowercase().contains("cartesia-version: 2026-08-14") && req.contains("ink-whisper"));

        let (r, req) = run("sarvam", r#"{"request_id":"x","transcript":"namaste","language_code":"hi-IN"}"#, Some("hi"));
        assert_eq!(r.unwrap(), "namaste");
        assert!(req.starts_with("POST /speech-to-text ") && req.to_lowercase().contains("api-subscription-key: secret"));
        assert!(req.contains("saaras:v4") && req.contains("\r\nhi-IN\r\n"));
        let (_, req) = run("sarvam", r#"{"transcript":"x"}"#, None);
        assert!(req.contains("\r\nunknown\r\n"));

        let (r, mut seen) = run_with("azure", "", "westeurope", &[(200, r#"{"durationMilliseconds":900,"combinedPhrases":[{"text":"Hallo Welt."}],"phrases":[]}"#)], Some("de"));
        assert_eq!(r.unwrap(), "Hallo Welt.");
        let req = seen.remove(0);
        assert!(req.starts_with("POST /speechtotext/transcriptions:transcribe?api-version=2025-10-15 "), "{req}");
        assert!(req.to_lowercase().contains("ocp-apim-subscription-key: secret") && req.contains("name=\"audio\"; filename=") && req.contains("{\"locales\":[\"de-DE\"]}"));
        let (r, _) = run_with("azure", "", "", &[], None);
        assert_eq!(r, Err(CloudError::NeedsRegion));
        let (r, _) = run_with("azure", "", "evil.com/x?", &[], None);
        assert_eq!(r, Err(CloudError::NeedsRegion));

        let (r, mut seen) = run_with("speechmatics", "", "", &[(201, r#"{"id":"j1","status":"done","txt":"from the job\n"}"#)], None);
        assert_eq!(r.unwrap(), "from the job");
        let req = seen.remove(0);
        assert!(req.starts_with("POST /v2/jobs?format=txt&wait=25 "), "{req}");
        assert!(req.contains("name=\"data_file\"; filename=") && req.contains(r#""language":"auto","operating_point":"enhanced""#));
        assert_eq!(run_with("speechmatics", "", "", &[(201, r#"{"id":"j1","status":"created"}"#)], None).0, Err(CloudError::BadResponse));
    }

    #[test]
    fn raw_body_deepgram() {
        let (r, req) = run("deepgram", r#"{"results":{"channels":[{"alternatives":[{"transcript":"good morning","confidence":0.99}]}]}}"#, Some("en"));
        assert_eq!(r.unwrap(), "good morning");
        assert!(req.starts_with("POST /v1/listen?model=nova-3&smart_format=true&language=en "), "{req}");
        let low = req.to_lowercase();
        assert!(low.contains("authorization: token secret") && low.contains("content-type: audio/wav"));
        assert!(req.contains("RIFF") && !req.contains("multipart"));
        let (_, req) = run("deepgram", r#"{"results":{"channels":[{"alternatives":[{"transcript":""}]}]}}"#, None);
        assert!(req.contains("&language=multi "));
    }

    #[test]
    fn json_audio_gemini_and_openrouter() {
        let (r, mut seen) = run_with("gemini", "gemini-3.8-flash", "", &[(200, r#"{"candidates":[{"content":{"parts":[{"text":"Here is the transcript: \"Call me at five.\"\n"}]}}]}"#)], Some("en"));
        assert_eq!(r.unwrap(), "Call me at five.");
        let req = seen.remove(0);
        assert!(req.starts_with("POST /v1beta/models/gemini-3.8-flash:generateContent "), "{req}");
        assert!(req.to_lowercase().contains("x-goog-api-key: secret"));
        let body: serde_json::Value = serde_json::from_str(req.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["generationConfig"]["temperature"], 0);
        assert_eq!(body["contents"][0]["parts"][1]["inlineData"]["mimeType"], "audio/wav");
        assert!(body["contents"][0]["parts"][1]["inlineData"]["data"].as_str().unwrap().starts_with("UklGR")); // "RIFF"
        assert!(body["contents"][0]["parts"][0]["text"].as_str().unwrap().contains("verbatim") && body["contents"][0]["parts"][0]["text"].as_str().unwrap().contains("code en"));

        let (r, mut seen) = run_with("openrouter", "", "", &[(200, r#"{"choices":[{"message":{"role":"assistant","content":"```\nsee you soon\n```"}}]}"#)], None);
        assert_eq!(r.unwrap(), "see you soon");
        let req = seen.remove(0);
        assert!(req.starts_with("POST /api/v1/chat/completions ") && req.to_lowercase().contains("authorization: bearer secret"));
        let body: serde_json::Value = serde_json::from_str(req.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["model"], provider("openrouter").unwrap().default_model.as_str());
        assert_eq!(body["messages"][0]["content"][1]["input_audio"]["format"], "wav");
    }

    #[test]
    fn tidy_strips_wrappers_only() {
        assert_eq!(tidy("  \"Hello.\" "), "Hello.");
        assert_eq!(tidy("Transcription: hi there"), "hi there");
        assert_eq!(tidy("Sure, here's the transcript:\n“Ok.”"), "Ok.");
        assert_eq!(tidy("```text\nabc\n```"), "abc");
        assert_eq!(tidy("Note: buy milk"), "Note: buy milk");
        assert_eq!(tidy("He said \"no\" twice"), "He said \"no\" twice");
    }

    #[test]
    fn upload_and_poll_assemblyai() {
        let (r, seen) = run_with("assemblyai", "", "", &[
            (200, r#"{"upload_url":"https://cdn.example/u1"}"#),
            (200, r#"{"id":"t1","status":"queued"}"#),
            (200, r#"{"id":"t1","status":"processing","text":null}"#),
            (200, r#"{"id":"t1","status":"completed","text":" done text "}"#),
        ], Some("en"));
        assert_eq!(r.unwrap(), "done text");
        assert!(seen[0].starts_with("POST /v2/upload ") && seen[0].to_lowercase().contains("authorization: secret") && seen[0].contains("RIFF"));
        assert!(seen[1].starts_with("POST /v2/transcript "));
        let body: serde_json::Value = serde_json::from_str(seen[1].split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body, serde_json::json!({"audio_url":"https://cdn.example/u1","speech_models":[provider("assemblyai").unwrap().default_model, "universal-2"],"language_code":"en"}));
        assert!(seen[2].starts_with("GET /v2/transcript/t1 ") && seen[3].starts_with("GET /v2/transcript/t1 "));
        let (r, _) = run_with("assemblyai", "", "", &[(200, r#"{"upload_url":"u"}"#), (200, r#"{"id":"t1"}"#), (200, r#"{"status":"error","error":"bad audio"}"#)], None);
        assert_eq!(r, Err(CloudError::BadAudio));
    }

    #[test]
    fn upload_and_poll_soniox_cleans_up() {
        let (r, seen) = run_with("soniox", "", "", &[
            (201, r#"{"id":"f-1","filename":"audio.wav"}"#),
            (201, r#"{"id":"tr-1","status":"queued"}"#),
            (200, r#"{"id":"tr-1","status":"completed"}"#),
            (200, r#"{"id":"tr-1","text":"soniox text","tokens":[]}"#),
            (204, ""),
            (204, ""),
        ], Some("fr"));
        assert_eq!(r.unwrap(), "soniox text");
        assert!(seen[0].starts_with("POST /v1/files ") && seen[0].contains("name=\"file\"; filename=") && seen[0].to_lowercase().contains("authorization: bearer secret"));
        let body: serde_json::Value = serde_json::from_str(seen[1].split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body, serde_json::json!({"file_id":"f-1","model":"stt-async-v6","language_hints":["fr"]}));
        assert!(seen[2].starts_with("GET /v1/transcriptions/tr-1 ") && seen[3].starts_with("GET /v1/transcriptions/tr-1/transcript "));
        assert!(seen[4].starts_with("DELETE /v1/transcriptions/tr-1 ") && seen[5].starts_with("DELETE /v1/files/f-1 "));
    }

    #[test]
    fn errors_map_and_never_leak_key() {
        for (status, want) in [(401, CloudError::KeyRejected), (403, CloudError::KeyRejected), (402, CloudError::NoCredit), (400, CloudError::BadAudio)] {
            let (r, seen) = run_with("groq", "", "", &[(status, "{}")], None);
            assert_eq!((r, seen.len()), (Err(want), 1), "{status}: never retried");
        }
        // rate limits and server errors are retried once, then reported
        for (status, want) in [(429, CloudError::RateLimited), (503, CloudError::Server(503))] {
            let (r, seen) = run_with("groq", "", "", &[(status, "{}"), (status, "{}")], None);
            assert_eq!((r, seen.len()), (Err(want), 2));
        }
        assert_eq!(run("groq", "not json", None).0, Err(CloudError::BadResponse));
        // closed port => offline, and the message never carries the key
        let cancel = AtomicBool::new(false);
        let p = provider("groq").unwrap();
        let e = Call::new(p, "SECRET", p.model(""), "", Some("http://127.0.0.1:1"), &cancel).run(&[0.0; 160], None).unwrap_err();
        assert_eq!(e, CloudError::Offline);
        assert!(!e.to_string().contains("SECRET"));
        assert_eq!(transcribe("groq", "", "", "", &[0.0], None, &cancel), Err(CloudError::NoKey));
        assert_eq!(transcribe("nope", "k", "", "", &[0.0], None, &cancel), Err(CloudError::Unsupported));
        assert!(key_path("../x").is_none() && key_path("").is_none());
    }

    #[test]
    fn contract_cases_from_the_docs() {
        let doc: serde_json::Value = serde_json::from_str(include_str!("../../../../providers/tests/contract.json")).unwrap();
        let cases = doc["cases"].as_array().unwrap();
        assert_eq!(cases.len(), providers().len(), "one contract case per provider");
        for c in cases {
            let id = c["provider"].as_str().unwrap();
            let replies: Vec<(u16, String, &str)> = c["replies"].as_array().unwrap().iter().map(|r| (r[0].as_u64().unwrap() as u16, r[1].as_str().map_or_else(|| r[1].to_string(), str::to_string), "")).collect();
            let (base, h) = mock(replies);
            let p = provider(id).unwrap();
            let cancel = AtomicBool::new(false);
            let got = Call::new(p, "SECRET", p.model(""), c["region"].as_str().unwrap_or(""), Some(&base), &cancel).run(&[0.1; 16_000], c["lang"].as_str());
            let seen = h.join().unwrap();
            assert_eq!(got.as_deref(), Ok(c["text"].as_str().unwrap()), "{id}");
            let want = c["requests"].as_array().unwrap();
            assert_eq!(seen.len(), want.len(), "{id}: request count");
            for (req, w) in seen.iter().zip(want) {
                let (head, body) = req.split_once("\r\n\r\n").unwrap();
                let line = head.lines().next().unwrap();
                let target = line.split(' ').nth(1).unwrap();
                let (path, query) = target.split_once('?').unwrap_or((target, ""));
                assert!(line.starts_with(&format!("{} ", w["method"].as_str().unwrap())) && path == w["path"].as_str().unwrap(), "{id}: {line}");
                assert!(!line.contains("SECRET"), "{id}: key in the URL");
                let hdr = w["header"].as_array().unwrap();
                let want_h = format!("{}: {}", hdr[0].as_str().unwrap(), hdr[1].as_str().unwrap().replace("KEY", "SECRET")).to_lowercase();
                assert!(head.to_lowercase().lines().any(|l| l == want_h), "{id}: missing {want_h}");
                for (k, v) in w["query"].as_object().into_iter().flatten() {
                    assert!(query.split('&').any(|kv| kv == format!("{k}={}", enc(v.as_str().unwrap()))), "{id}: query {k} in {query}");
                }
                for (k, v) in w["form"].as_object().into_iter().flatten() {
                    match v.as_str() {
                        Some(v) => assert!(body.contains(&format!("name=\"{k}\"\r\n\r\n{v}\r\n")), "{id}: form {k}={v}"),
                        None => assert!(body.contains(&format!("name=\"{k}\"")), "{id}: form {k}"),
                    }
                }
                if let Some(json) = w["json"].as_object() {
                    let sent: serde_json::Value = serde_json::from_str(body).unwrap();
                    for (k, v) in json {
                        assert_eq!(&sent[k], v, "{id}: json {k}");
                    }
                }
                if let Some(ct) = w["raw"].as_str() {
                    assert!(head.to_lowercase().contains(&format!("content-type: {ct}")) && body.starts_with("RIFF"), "{id}: raw body");
                }
            }
        }
    }

    #[test]
    fn shared_rules() {
        // errors
        assert_eq!(classify(429, r#"{"error":{"type":"insufficient_quota"}}"#), CloudError::NoCredit);
        assert_eq!(classify(429, r#"{"error":{"code":"rate_limit_exceeded"}}"#), CloudError::RateLimited);
        assert_eq!(classify(400, r#"{"error":{"status":"INVALID_ARGUMENT","details":[{"reason":"API_KEY_INVALID"}]}}"#), CloudError::KeyRejected);
        assert_eq!(classify(401, r#"{"detail":{"status":"quota_exceeded"}}"#), CloudError::NoCredit);
        assert_eq!(classify(422, "{}"), CloudError::BadAudio);
        assert_eq!(classify(498, ""), CloudError::Server(498));
        // retries
        assert_eq!(retry_after(&CloudError::RateLimited, Some("3")), Some(Duration::from_secs(3)));
        assert_eq!(retry_after(&CloudError::RateLimited, Some("60")), None);
        assert!(retry_after(&CloudError::Server(503), None).is_some_and(|d| (500..1500).contains(&(d.as_millis() as u64))));
        assert_eq!(retry_after(&CloudError::KeyRejected, Some("1")), None);
        assert_eq!(timeout_for(16_000 * 60), Duration::from_secs(150));
        // Azure hosts
        let t = "{region}.api.cognitive.microsoft.com";
        assert_eq!(azure_host(" WestEurope ", t).as_deref(), Some("westeurope.api.cognitive.microsoft.com"));
        assert_eq!(azure_host("https://my-res.cognitiveservices.azure.com/", t).as_deref(), Some("my-res.cognitiveservices.azure.com"));
        for bad in ["", "evil.com", "x.cognitiveservices.azure.com.evil.com", "a/b", "me.cognitiveservices.azure.com:444", "-x"] {
            assert_eq!(azure_host(bad, t), None, "{bad}");
        }
        // LLM answers that are not transcripts
        assert_eq!(transcript_only("I'm sorry, but I can't transcribe this audio."), "");
        assert_eq!(transcript_only("[silence]"), "");
        assert_eq!(transcript_only("There is no speech in the audio."), "");
        assert_eq!(transcript_only("I'm sorry I can't make it tonight."), "I'm sorry I can't make it tonight.");
        assert_eq!(transcript_only("Here is the transcript: \"Call me.\""), "Call me.");
    }

    #[test]
    fn long_audio_splits_at_the_quietest_point_and_joins() {
        // 50 s of tone with a quiet gap at 22-22.2 s; Sarvam caps a request at 29 s
        let mut pcm = vec![0.3f32; 50 * 16_000];
        pcm[22 * 16_000..22 * 16_000 + 3_200].fill(0.0);
        let parts = split(&pcm, 29 * 16_000);
        assert_eq!(parts.len(), 2);
        assert!((22 * 16_000..22 * 16_000 + 3_200).contains(&parts[0].end), "{parts:?}");
        assert_eq!(parts[1].end, pcm.len());
        assert_eq!(split(&pcm[..16_000], 29 * 16_000), vec![0..16_000]);
        let parts = split(&vec![0.1; 100 * 16_000], 29 * 16_000);
        assert!(parts.iter().all(|r| r.len() <= 29 * 16_000) && parts.len() == 4, "{parts:?}");

        let (base, h) = mock(vec![(200, r#"{"transcript":"one"}"#.into(), ""), (200, r#"{"transcript":" two "}"#.into(), "")]);
        let p = provider("sarvam").unwrap();
        let cancel = AtomicBool::new(false);
        let r = Call::new(p, "k", p.model(""), "", Some(&base), &cancel).run(&pcm, Some("en"));
        assert_eq!(r.unwrap(), "one two");
        assert_eq!(h.join().unwrap().len(), 2);
    }

    #[test]
    fn retry_after_is_respected_and_cancel_stops() {
        let (base, h) = mock(vec![(429, "{}".into(), "Retry-After: 0\r\n"), (200, r#"{"text":"ok"}"#.into(), "")]);
        let p = provider("groq").unwrap();
        let cancel = AtomicBool::new(false);
        assert_eq!(Call::new(p, "k", p.model(""), "", Some(&base), &cancel).run(&[0.1; 16_000], None).unwrap(), "ok");
        assert_eq!(h.join().unwrap().len(), 2);
        let (base, h) = mock(vec![(429, "{}".into(), "Retry-After: 120\r\n")]);
        assert_eq!(Call::new(p, "k", p.model(""), "", Some(&base), &cancel).run(&[0.1; 16_000], None), Err(CloudError::RateLimited));
        assert_eq!(h.join().unwrap().len(), 1, "a long Retry-After is not waited for");
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(transcribe("groq", "k", "", "", &[0.1; 16_000], None, &cancel), Err(CloudError::Cancelled));
        // LLM refusal -> no text (the caller keeps the on-device text)
        let (r, _) = run_with("gemini", "", "", &[(200, r#"{"candidates":[{"content":{"parts":[{"text":"I'm sorry, I cannot transcribe this audio."}]}}]}"#)], None);
        assert_eq!(r, Err(CloudError::NoText));
    }

    fn feed(id: &str, msgs: &[&str]) -> LiveState {
        let live = provider(id).unwrap().live.as_ref().unwrap();
        let mut st = LiveState { ready: true, ..LiveState::default() };
        for m in msgs {
            st.on(live, m);
        }
        st
    }

    #[test]
    fn live_messages_follow_each_providers_rules() {
        let live = |id: &str| provider(id).unwrap().live.as_ref().unwrap();
        let st = feed("deepgram", &[r#"{"type":"Results","is_final":true,"channel":{"alternatives":[{"transcript":"yeah so my"}]}}"#, r#"{"type":"Results","is_final":false,"channel":{"alternatives":[{"transcript":"credit card"}]}}"#]);
        assert_eq!(st.text(live("deepgram")), "yeah so my credit card");
        let st = feed("assemblyai", &[r#"{"type":"Turn","end_of_turn":false,"transcript":"my number is five"}"#, r#"{"type":"Turn","end_of_turn":true,"transcript":"My number is 555-0134."}"#, r#"{"type":"Turn","end_of_turn":false,"transcript":"thanks"}"#]);
        assert_eq!(st.text(live("assemblyai")), "My number is 555-0134. thanks");
        let st = feed("soniox", &[r#"{"tokens":[{"text":"How","is_final":true},{"text":"'re","is_final":false}]}"#, r#"{"tokens":[{"text":"'re","is_final":true},{"text":" you","is_final":true},{"text":"<fin>","is_final":true}]}"#, r#"{"tokens":[],"finished":true}"#]);
        assert_eq!((st.text(live("soniox")).as_str(), st.done), ("How're you", true));
        let st = feed("mistral", &[r#"{"type":"transcription.text.delta","text":"Hel"}"#, r#"{"type":"transcription.text.delta","text":"lo there"}"#]);
        assert_eq!(st.text(live("mistral")), "Hello there");
        let st = feed("mistral", &[r#"{"type":"transcription.done","text":"Hello there."}"#]);
        assert_eq!((st.text(live("mistral")).as_str(), st.done), ("Hello there.", true));
        let mut st = feed("openai", &[r#"{"type":"conversation.item.input_audio_transcription.delta","item_id":"item_003","delta":"Hello,"}"#]);
        assert_eq!(st.text(live("openai")), "Hello,");
        st.end_sent = true;
        st.on(live("openai"), r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"item_003","transcript":"Hello, how are you?"}"#);
        assert_eq!((st.text(live("openai")).as_str(), st.done), ("Hello, how are you?", true));
        let st = feed("speechmatics", &[r#"{"message":"AddTranscript","metadata":{"transcript":"Hello world."}}"#, r#"{"message":"AddPartialTranscript","metadata":{"transcript":"how"}}"#, r#"{"message":"EndOfTranscript"}"#]);
        assert_eq!((st.text(live("speechmatics")).as_str(), st.done), ("Hello world. how", true));
        let st = feed("cartesia", &[r#"{"type":"transcript","is_final":true,"text":"How are"}"#, r#"{"type":"transcript","is_final":true,"text":" you doing today?"}"#, r#"{"type":"done"}"#]);
        assert_eq!((st.text(live("cartesia")).as_str(), st.done), ("How are you doing today?", true));
        let st = feed("elevenlabs", &[r#"{"message_type":"auth_error","error":"Invalid API key"}"#]);
        assert_eq!(classify_live(st.error.as_deref().unwrap()), CloudError::KeyRejected);
        assert!(providers().iter().filter(|p| p.live_supported()).count() >= 8);
        // live models: mapped counterpart, same id, or the live default; every one is a live model
        assert_eq!(live("elevenlabs").model_for("scribe_v3"), "scribe_v3_live");
        assert_eq!(live("openai").model_for("gpt-transcribe"), "gpt-live-transcribe");
        assert_eq!(live("deepgram").model_for("nova-3"), "nova-3");
        assert_eq!(live("soniox").model_for("stt-async-v6"), "stt-rt-v6");
        assert_eq!(live("assemblyai").model_for(""), "universal-3-6-pro");
        for p in providers().iter().filter(|p| p.live_supported()) {
            let l = p.live.as_ref().unwrap();
            assert!(l.models.contains(&l.default_model), "{}", p.id);
            for m in p.models.iter().chain(l.map.values()) {
                assert!(l.models.iter().any(|x| x == l.model_for(m)), "{}: {m}", p.id);
            }
        }
        assert_eq!(resample_from_16k(&[0.0, 1.0], 24_000).len(), 3);
    }

    /// A fake streaming provider: checks the handshake, collects the audio, answers like Deepgram.
    #[test]
    fn live_stream_end_to_end_against_a_fake_server() {
        use tungstenite::Message;
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("ws://{}", l.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (c, _) = l.accept().unwrap();
            let mut seen = String::new();
            let mut ws = tungstenite::accept_hdr(c, |req: &tungstenite::handshake::server::Request, resp| {
                seen = format!("{} auth={:?}", req.uri(), req.headers().get("authorization"));
                Ok(resp)
            }).unwrap();
            let (mut bytes, mut frames) = (0, 0);
            ws.send(Message::Text(r#"{"type":"Results","is_final":false,"channel":{"alternatives":[{"transcript":"hel"}]}}"#.into())).unwrap();
            loop {
                match ws.read().unwrap() {
                    Message::Binary(b) => { bytes += b.len(); frames += 1; }
                    Message::Text(t) => {
                        assert_eq!(t.as_str(), r#"{"type":"CloseStream"}"#);
                        ws.send(Message::Text(r#"{"type":"Results","is_final":true,"channel":{"alternatives":[{"transcript":"hello world"}]}}"#.into())).unwrap();
                        ws.send(Message::Text(r#"{"type":"Metadata"}"#.into())).unwrap();
                        break;
                    }
                    _ => {}
                }
            }
            let _ = ws.close(None);
            (seen, bytes, frames)
        });
        let (tx, rx) = std::sync::mpsc::channel();
        for _ in 0..10 {
            tx.send(vec![0.1f32; 1_600]).unwrap(); // 1 s in 100 ms chunks
        }
        drop(tx);
        let cancel = AtomicBool::new(false);
        let mut shown = Vec::new();
        let r = live_run(provider("deepgram").unwrap(), "SECRET", "", "", Some("en"), Some(&base), rx, &cancel, |t| shown.push(t.to_string()));
        assert_eq!(r.map_err(|f| f.1).unwrap(), "hello world");
        assert!(shown.contains(&"hel".to_string()));
        let (seen, bytes, frames) = server.join().unwrap();
        assert!(seen.starts_with("/v1/listen?") && seen.contains("model=nova-3") && seen.contains("encoding=linear16") && seen.contains("sample_rate=16000") && seen.contains("language=en"), "{seen}");
        assert!(seen.contains("auth=Some(\"Token SECRET\")") && !seen.split(' ').next().unwrap().contains("SECRET"), "{seen}");
        assert_eq!((bytes, frames), (32_000, 10));
    }

    /// Speechmatics: no audio before RecognitionStarted; EndOfStream carries the number of frames sent.
    #[test]
    fn live_waits_for_ready_and_counts_frames() {
        use tungstenite::Message;
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("ws://{}", l.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let mut ws = tungstenite::accept(l.accept().unwrap().0).unwrap();
            let start = ws.read().unwrap().into_text().unwrap().to_string();
            std::thread::sleep(Duration::from_millis(150));
            assert!(ws.can_read());
            ws.send(Message::Text(r#"{"message":"RecognitionStarted"}"#.into())).unwrap();
            let mut frames = 0;
            let end = loop {
                match ws.read().unwrap() {
                    Message::Binary(_) => frames += 1,
                    Message::Text(t) => break t.to_string(),
                    _ => {}
                }
            };
            ws.send(Message::Text(r#"{"message":"AddTranscript","metadata":{"transcript":"Hello world."}}"#.into())).unwrap();
            ws.send(Message::Text(r#"{"message":"EndOfTranscript"}"#.into())).unwrap();
            (start, frames, end)
        });
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(vec![0.1f32; 4_000]).unwrap();
        drop(tx);
        let cancel = AtomicBool::new(false);
        let r = live_run(provider("speechmatics").unwrap(), "SECRET", "", "", None, Some(&base), rx, &cancel, |_| {});
        assert_eq!(r.map_err(|f| f.1).unwrap(), "Hello world.");
        let (start, frames, end) = server.join().unwrap();
        assert!(start.contains(r#""language":"en""#) && start.contains(r#""model":"enhanced""#), "{start}");
        assert_eq!((frames, end.as_str()), (3, r#"{"message":"EndOfStream","last_seq_no":3}"#));
    }
}
