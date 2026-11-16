//! Live checks of the cloud providers with the user's own keys (no GUI needed):
//!
//!   voicio cloud-check [--provider ID] [--clip FILE.wav] [--region R]
//!       sends the bench clip (5.4 s of English, reference in providers/tests/bench.txt; for --clip, FILE.txt
//!       next to it) through the real adapter of every provider with a stored key, and prints provider, model,
//!       latency, WER, PASS (non-empty and WER under 20 %) or FAIL with the error. Keys are never printed.
//!   voicio cloud-check --live [--provider ID] [--clip FILE.wav]
//!       the same through each provider's live WebSocket: the clip is streamed in real time (100 ms chunks),
//!       and the row also shows how many partial updates arrived and the delay after the last chunk.
//!   voicio cloud-check --probe [--provider ID]
//!       no keys needed: sends the bench clip with an invalid key to every provider's real endpoint (batch, and
//!       the live WebSocket where there is one) and checks the provider rejected the KEY (401/403 or its auth
//!       error), not the URL (404/405), the host (DNS/TLS) or the request shape (400/415/422).
//!   voicio cloud-key set ID
//!       reads the key from stdin (not echoed on a terminal) into the 0600 key file.

use crate::cloud;
use crate::config::Config;
use std::io::{BufRead, IsTerminal, Write};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

const BENCH_WAV: &[u8] = include_bytes!("../../../../extension/apps/chrome/public/bench.wav");
const BENCH_TXT: &str = include_str!("../../../../providers/tests/bench.txt");
const PASS_WER: f32 = 0.20;

/// 16-bit PCM WAV (any rate, first channel) to f32 samples and its rate.
fn pcm16(bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let (mut at, mut rate, mut channels) = (12, 0u32, 1u16);
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let body = &bytes[at + 8..(at + 8 + len).min(bytes.len())];
        if id == b"fmt " && body.len() >= 16 {
            channels = u16::from_le_bytes([body[2], body[3]]);
            rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
            if u16::from_le_bytes([body[14], body[15]]) != 16 {
                return Err("only 16-bit PCM WAV is supported".into());
            }
        } else if id == b"data" {
            let ch = channels.max(1) as usize;
            return Ok((body.chunks_exact(2 * ch).map(|f| i16::from_le_bytes([f[0], f[1]]) as f32 / 32768.0).collect(), rate));
        }
        at += 8 + len + (len & 1);
    }
    Err("no audio in the WAV file".into())
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase().chars().map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' }).collect::<String>().split_whitespace().map(str::to_string).collect()
}

/// Word error rate: word edit distance over the reference length.
fn wer(reference: &str, hypothesis: &str) -> f32 {
    let (r, h) = (words(reference), words(hypothesis));
    let mut row: Vec<usize> = (0..=h.len()).collect();
    for (i, rw) in r.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, hw) in h.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(rw != hw)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[h.len()] as f32 / r.len().max(1) as f32
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).map(|i| args.get(i + 1).cloned().unwrap_or_else(|| crate::fail(&format!("{name} needs a value"))))
}

/// PASS when the provider rejected the key; FAIL names what the rejection says was wrong instead.
pub fn verdict(status: u16, body: &str) -> (bool, &'static str) {
    let b = body.to_lowercase();
    let about_key = ["api key", "api_key", "apikey", "api-key", "unauthor", "authenticat", "invalid key", "invalid_key", "credential", "token", "permission", "forbidden", "subscription key", "access denied", "not valid", "invalid_auth", "incorrect", "jwt"].iter().any(|w| b.contains(w));
    match status {
        401 | 403 => (true, "key rejected"),
        400 | 404 | 422 if about_key => (true, "key rejected (4xx with an auth error)"), // AssemblyAI's token endpoint: 404 "Invalid API key"
        404 | 405 => (false, "wrong path or method"),
        400 | 415 | 422 => (false, "request shape rejected"),
        0 => (false, "no HTTP answer (host, DNS, TLS or timeout)"),
        200..=299 => (false, "accepted an invalid key?"),
        _ => (false, "unexpected status"),
    }
}

fn probe_all(only: Option<&str>, pcm: &[f32], region: &str) {
    println!("probe: invalid key {:?}, bench clip {:.1} s; PASS = the provider rejected the key, so URL, method, auth header and body shape were accepted", cloud::PROBE_KEY, pcm.len() as f32 / 16_000.0);
    let mut failed = 0;
    for p in cloud::providers().iter().filter(|p| only.is_none_or(|id| id == p.id)) {
        let region = if p.needs_region && region.is_empty() { "westeurope" } else { region };
        let (r, trace) = cloud::probe(&p.id, region, pcm);
        let row = match trace.last() {
            Some(t) => {
                let (pass, why) = verdict(t.status, &t.body);
                failed += usize::from(!pass);
                let path = t.url.split_once("://").map_or(t.url.as_str(), |(_, rest)| rest);
                format!("{} {:<4} {:<6} {path}  {why}  {:?}", if pass { "PASS" } else { "FAIL" }, t.status, t.method, t.body.chars().take(110).collect::<String>())
            }
            None => {
                failed += 1;
                format!("FAIL no request sent ({})", r.err().map_or_else(String::new, |e| e.to_string()))
            }
        };
        println!("{:<12} batch {row}", p.id);
        if let Some(row) = crate::cloud::probe_live(&p.id, region, pcm) {
            let (pass, line) = row;
            failed += usize::from(!pass);
            println!("{:<12} live  {line}", p.id);
        }
    }
    println!("{failed} failed");
    if failed > 0 {
        std::process::exit(1);
    }
}

pub fn run(args: &[String]) {
    let only = flag(args, "--provider");
    if let Some(id) = only.as_deref().filter(|id| cloud::provider(id).is_none()) {
        crate::fail(&format!("unknown provider {id:?}"));
    }
    let (wav, reference) = match flag(args, "--clip") {
        Some(path) => {
            let wav = std::fs::read(&path).unwrap_or_else(|e| crate::fail(&format!("cannot read {path}: {e}")));
            let txt = std::path::Path::new(&path).with_extension("txt");
            (wav, std::fs::read_to_string(txt).unwrap_or_default())
        }
        None => (BENCH_WAV.to_vec(), BENCH_TXT.to_string()),
    };
    let (pcm, rate) = pcm16(&wav).unwrap_or_else(|e| crate::fail(&e));
    let pcm = cloud::resample_16k(&pcm, rate);
    let config = Config::load();
    let region = flag(args, "--region").unwrap_or_else(|| config.cloud_region.clone());
    if args.iter().any(|a| a == "--probe") {
        return probe_all(only.as_deref(), &pcm, &region);
    }
    let reference = reference.trim();
    println!("clip {:.1} s; reference: {}", pcm.len() as f32 / 16_000.0, if reference.is_empty() { "(none: pass = any text)" } else { reference });
    let (mut called, mut failed) = (0, 0);
    let live = args.iter().any(|a| a == "--live");
    for p in cloud::providers().iter().filter(|p| only.as_deref().is_none_or(|id| id == p.id) && (!live || p.live_supported())) {
        let Some(key) = cloud::load_key(&p.id) else {
            println!("{:<12} not called (no key stored: voicio cloud-key set {})", p.id, p.id);
            failed += usize::from(only.is_some());
            continue;
        };
        called += 1;
        let (model, result, ms, extra) = if live {
            let l = p.live.as_ref().unwrap();
            let model = l.default_model.as_str();
            let (tx, rx) = std::sync::mpsc::channel();
            let clip = pcm.clone();
            let feeder = std::thread::spawn(move || {
                for c in clip.chunks(1_600) {
                    let _ = tx.send(c.to_vec());
                    std::thread::sleep(std::time::Duration::from_millis(100)); // real time, like a microphone
                }
                Instant::now()
            });
            let mut updates = 0;
            let result = cloud::live(&p.id, &key, model, &region, Some("en"), rx, &AtomicBool::new(false), |_| updates += 1);
            let audio_end = feeder.join().unwrap_or_else(|_| Instant::now());
            (model, result, audio_end.elapsed().as_millis(), format!("  {updates} live updates"))
        } else {
            let model = if config.cloud_provider == p.id && p.models.contains(&config.cloud_model) { config.cloud_model.as_str() } else { p.default_model.as_str() };
            let started = Instant::now();
            let result = cloud::transcribe(&p.id, &key, model, &region, &pcm, Some("en"), &AtomicBool::new(false));
            (model, result, started.elapsed().as_millis(), String::new())
        };
        match result {
            Ok(text) => {
                let w = if reference.is_empty() { 0.0 } else { wer(reference, &text) };
                let pass = !text.trim().is_empty() && w < PASS_WER;
                failed += usize::from(!pass);
                println!("{:<12} {:<26} {} {:>6} ms  WER {:>5.1} %  {:?}{extra}", p.id, model, if pass { "PASS" } else { "FAIL" }, ms, w * 100.0, text.trim());
            }
            Err(e) => {
                failed += 1;
                println!("{:<12} {:<26} FAIL {:>6} ms  {e}", p.id, model, ms);
            }
        }
    }
    println!("{called} provider(s) called, {failed} failed");
    if failed > 0 {
        std::process::exit(1);
    }
}

/// `voicio cloud-key set ID`: the key comes from stdin, never from argv (shell history, `ps`).
pub fn key(args: &[String]) {
    let (Some("set"), Some(id)) = (args.first().map(String::as_str), args.get(1)) else {
        crate::fail("usage: voicio cloud-key set PROVIDER-ID   (then paste the key and press Enter)")
    };
    if cloud::provider(id).is_none() {
        crate::fail(&format!("unknown provider {id:?}; known: {}", cloud::providers().iter().map(|p| p.id.as_str()).collect::<Vec<_>>().join(", ")));
    }
    let tty = std::io::stdin().is_terminal();
    let stty = |arg: &str| {
        #[cfg(unix)]
        let _ = std::process::Command::new("stty").arg(arg).stdin(std::process::Stdio::inherit()).status();
        #[cfg(not(unix))]
        let _ = arg;
    };
    if tty {
        eprint!("API key for {id} (not shown): ");
        let _ = std::io::stderr().flush();
        stty("-echo");
    }
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line);
    if tty {
        stty("echo");
        eprintln!();
    }
    let key = line.trim();
    if read.is_err() || key.is_empty() {
        crate::fail("no key read from stdin");
    }
    cloud::save_key(id, key).unwrap_or_else(|e| crate::fail(&format!("could not store the key: {e}")));
    println!("Stored the {id} key (0600 file). Check it with: voicio cloud-check --provider {id}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bench_clip_reads_and_word_error_rate_counts_edits() {
        let (pcm, rate) = pcm16(BENCH_WAV).unwrap();
        assert_eq!(rate, 16_000);
        assert!((5.0..6.0).contains(&(pcm.len() as f32 / 16_000.0)));
        assert!(words(BENCH_TXT).len() > 5);
        assert_eq!(wer("the feet of harts", "The feet of harts."), 0.0);
        assert_eq!(wer("the feet of harts", "the feet of hearts"), 0.25);
        assert_eq!(wer("a b", ""), 1.0);
    }
}
