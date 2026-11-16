//! `voicio-engine`: the model worker the `voicio` agent spawns (`voicio-engine worker`), plus dev
//! tools: the web test page (default), `--selftest`, `--eval DIR` and `--cleanup-test FILE`.

use serde_json::{json, Value};
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::{Message, WebSocket};
use voicio_engine::{memory_stats, word_errors, Engine, EngineConfig, Event, SAMPLE_RATE};

mod bake;
mod worker;

const PAGE: &str = include_str!("../web/index.html");
const MAX_FRAME_BYTES: usize = SAMPLE_RATE * 4; // one second of f32 audio

struct Args {
    config: EngineConfig,
    port: u16,
    selftest: bool,
    eval: Option<PathBuf>,
    /// `--eval` only: mix synthetic background noise into every file at this speech-to-noise ratio (dB).
    noise_snr: Option<f32>,
    cleanup_test: Option<PathBuf>,
    /// `--cleanup-test` only: a GGUF to compare, and how to prompt it (`chat` = its own template).
    test_model: Option<PathBuf>,
    test_style_chat: bool,
    /// `--eval-tsv FILE`: score the cleanup model on a category/input/reference file (`--verbose` lists misses).
    eval_tsv: Option<PathBuf>,
    verbose: bool,
    /// `--control styling,structure,context` for the evaluation (default semi-formal,prose,general).
    control: String,
    worker: bool,
}

fn parse_args() -> Result<Args, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let mut args = Args {
        config: EngineConfig::new(home.join(".cache/voicio")),
        port: 8765,
        selftest: false,
        eval: None,
        noise_snr: None,
        cleanup_test: None,
        test_model: None,
        test_style_chat: false,
        eval_tsv: None,
        verbose: false,
        control: String::new(),
        worker: false,
    };
    let mut it = std::env::args().skip(1).peekable();
    if it.peek().is_some_and(|a| a == "worker") {
        it.next();
        args.worker = true;
    }
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--models" => args.config.models_dir = value()?.into(),
            "--asr" => args.config.asr = value()?,
            "--language" => args.config.language = value()?,
            "--memory-budget" => {
                args.config.memory_budget_mb = value()?.parse().map_err(|_| "--memory-budget must be MB")?
            }
            "--threads" => {
                args.config.threads = value()?.parse().map_err(|_| "--threads must be a number")?
            }
            "--cpu-mode" => {
                args.config = args.config.with_cpu_mode(&value()?);
            }
            "--provider" => args.config.provider = value()?,
            "--port" => args.port = value()?.parse().map_err(|_| "--port must be a number")?,
            "--selftest" => args.selftest = true,
            "--cleanup" => args.config.cleanup = true,
            "--no-cleanup" => args.config.cleanup = false,
            "--no-diarize" => args.config.diarize = false,
            "--no-denoise" => args.config.denoise = false,
            "--noise-snr" => args.noise_snr = Some(value()?.parse().map_err(|_| "--noise-snr needs a number (dB)".to_string())?),
            "--max-utterance" => args.config.max_utterance_secs = value()?.parse().map_err(|_| "--max-utterance must be seconds")?,
            "--llm-threads" => {
                args.config.llm_threads = value()?
                    .parse()
                    .map_err(|_| "--llm-threads must be a number")?
            }
            "--eval" => args.eval = Some(value()?.into()),
            "--cleanup-test" => args.cleanup_test = Some(value()?.into()),
            "--tie" => { let from: PathBuf = value()?.into(); let to: PathBuf = value()?.into(); return tie(&from, &to); }
            "--eval-tsv" => args.eval_tsv = Some(value()?.into()),
            "--verbose" => args.verbose = true,
            "--control" => args.control = value()?,
            "--model" => args.test_model = Some(value()?.into()),
            "--style" => args.test_style_chat = value()? == "chat",
            "-h" | "--help" => {
                println!("voicio-engine [worker] [--models DIR] [--asr MODEL_DIR] [--memory-budget MB] [--threads N] [--llm-threads N] [--cpu-mode balanced|low|fast] [--cleanup] [--no-diarize] [--provider cpu] [--port 8765] [--selftest] [--eval DIR] [--cleanup-test FILE]");
                std::process::exit(0);
            }
            other => return Err(format!("unknown flag {other} (see --help)")),
        }
    }
    Ok(args)
}

fn main() {
    let args = parse_args().unwrap_or_else(|e| fail(&e));
    if args.worker {
        worker::run(args.config);
    }
    if let Some(file) = &args.eval_tsv {
        return eval_tsv(&args.config, file, args.test_model.as_deref(), args.test_style_chat, args.verbose, &args.control);
    }
    if let Some(file) = &args.cleanup_test {
        return cleanup_test(&args.config, file, args.test_model.as_deref(), args.test_style_chat, &args.control);
    }
    let engine = Engine::load(&args.config).unwrap_or_else(|e| fail(&e));
    println!(
        "{} loaded with {} threads, rss {:.0} MB",
        engine.label, args.config.threads, engine.load_rss_mb
    );
    if args.selftest {
        return selftest(&engine, &args.config);
    }
    if let Some(dir) = &args.eval {
        return eval(&engine, dir, args.noise_snr);
    }
    serve(Arc::new(engine), args.port);
}

fn fail(message: &str) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1)
}

// ---------- server ----------

fn serve(engine: Arc<Engine>, port: u16) {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .unwrap_or_else(|e| fail(&format!("port {port}: {e}")));
    println!("open http://localhost:{port}");
    let busy = Arc::new(AtomicBool::new(false));
    for stream in listener.incoming().flatten() {
        let (engine, busy) = (engine.clone(), busy.clone());
        std::thread::spawn(move || {
            if let Err(e) = route(stream, &engine, &busy, port) {
                eprintln!("connection: {e}");
            }
        });
    }
}

fn route(
    mut stream: TcpStream,
    engine: &Engine,
    busy: &AtomicBool,
    port: u16,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let mut head = [0u8; 256];
    let n = stream.peek(&mut head).map_err(|e| e.to_string())?;
    let request_line = String::from_utf8_lossy(&head[..n]);
    let path = request_line.split_whitespace().nth(1).unwrap_or("");
    match path {
        "/ws" => websocket(stream, engine, busy, port),
        "/" | "/index.html" => {
            let mut sink = [0u8; 4096];
            let _ = stream.read(&mut sink); // consume the request before replying
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{PAGE}",
                PAGE.len()
            );
            stream
                .write_all(reply.as_bytes())
                .map_err(|e| e.to_string())
        }
        _ => stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .map_err(|e| e.to_string()),
    }
}

fn websocket(
    stream: TcpStream,
    engine: &Engine,
    busy: &AtomicBool,
    port: u16,
) -> Result<(), String> {
    let allowed = [
        format!("http://localhost:{port}"),
        format!("http://127.0.0.1:{port}"),
    ];
    let check_origin = |request: &Request, response: Response| -> Result<Response, ErrorResponse> {
        let origin = request
            .headers()
            .get("origin")
            .and_then(|o| o.to_str().ok());
        match origin {
            // Browsers always send Origin; only our own page may drive the engine.
            Some(o) if !allowed.iter().any(|a| a == o) => {
                let mut refusal = ErrorResponse::new(Some("origin not allowed".into()));
                *refusal.status_mut() = tungstenite::http::StatusCode::FORBIDDEN;
                Err(refusal)
            }
            _ => Ok(response),
        }
    };
    let mut ws = tungstenite::accept_hdr(stream, check_origin).map_err(|e| e.to_string())?;
    ws.get_mut()
        .set_read_timeout(Some(Duration::from_millis(50)))
        .map_err(|e| e.to_string())?;

    let mut owns_engine = false;
    let result = connection_loop(&mut ws, engine, busy, &mut owns_engine);
    if owns_engine {
        busy.store(false, Ordering::SeqCst);
    }
    result
}

fn send(ws: &mut WebSocket<TcpStream>, value: Value) -> Result<(), String> {
    ws.send(Message::text(value.to_string()))
        .map_err(|e| e.to_string())
}

fn event_json(event: Event) -> Value {
    match event {
        Event::Segment {
            id,
            text,
            start_ms,
            end_ms,
            is_final,
        } => {
            json!({"type": "segment", "id": id, "text": text, "startMs": start_ms, "endMs": end_ms, "final": is_final})
        }
        Event::Speakers(assignments) => {
            json!({"type": "speakers", "assignments": assignments.into_iter().map(|(id, s)| (id, Value::from(s))).collect::<serde_json::Map<_, _>>()})
        }
        Event::Done(text) => json!({"type": "done", "text": text}),
        Event::Error(message) => json!({"type": "error", "message": message}),
    }
}

fn connection_loop(
    ws: &mut WebSocket<TcpStream>,
    engine: &Engine,
    busy: &AtomicBool,
    owns_engine: &mut bool,
) -> Result<(), String> {
    let mut session: Option<voicio_engine::Session> = None;
    let mut events: Option<Receiver<Event>> = None;
    let mut last_stats = Instant::now() - Duration::from_secs(1);
    loop {
        match ws.read() {
            Ok(Message::Binary(bytes)) => {
                if let Some(active) = session.as_mut() {
                    if bytes.len() % 4 != 0 || bytes.len() > MAX_FRAME_BYTES {
                        send(
                            ws,
                            json!({"type": "error", "message": "invalid audio frame"}),
                        )?;
                    } else {
                        let samples: Vec<f32> = bytes
                            .chunks_exact(4)
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect();
                        if let Err(limit) = active.feed(&samples) {
                            send(ws, json!({"type": "error", "message": limit}))?;
                            active.stop();
                        }
                    }
                }
            }
            Ok(Message::Text(text)) => {
                let command: Value = serde_json::from_str(text.as_str()).unwrap_or_default();
                match command["type"].as_str() {
                    Some("start") if events.is_none() => {
                        if busy.swap(true, Ordering::SeqCst) {
                            send(
                                ws,
                                json!({"type": "error", "message": "another recording is already running in a different tab"}),
                            )?;
                        } else {
                            *owns_engine = true;
                            let (new_session, rx) = engine.session()?;
                            session = Some(new_session);
                            events = Some(rx);
                            send(ws, json!({"type": "started"}))?;
                        }
                    }
                    Some("stop") => {
                        if let Some(active) = session.as_mut() {
                            active.stop();
                        }
                    }
                    _ => {}
                }
            }
            Ok(Message::Close(_)) => return Ok(()),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Ok(())
            }
            Err(e) => return Err(e.to_string()),
        }

        if let Some(rx) = &events {
            let mut finished = false;
            while let Ok(event) = rx.try_recv() {
                finished |= matches!(event, Event::Done(_));
                send(ws, event_json(event))?;
            }
            if finished {
                session = None;
                events = None;
                busy.store(false, Ordering::SeqCst);
                *owns_engine = false;
            }
        }

        if last_stats.elapsed() >= Duration::from_secs(1) {
            last_stats = Instant::now();
            let (rss, peak) = memory_stats().unwrap_or((0.0, 0.0));
            send(
                ws,
                json!({
                    "type": "stats",
                    "rssMb": rss.round(), "peakRssMb": peak.round(), "loadRssMb": engine.load_rss_mb.round(),
                    "model": engine.label,
                    "audioSec": session.as_ref().map_or(0.0, |s| (s.audio_seconds() * 10.0).round() / 10.0),
                    "decodeMs": engine.stats.decode_ms.load(Ordering::Relaxed),
                    "rtf": engine.stats.rtf_milli.load(Ordering::Relaxed) as f64 / 1000.0,
                }),
            )?;
        }
    }
}

// ---------- offline checks ----------

/// Streams `audio` through a session in 100 ms chunks, as the page does; returns events and wall time.
fn run(engine: &Engine, audio: &[f32]) -> (Vec<Event>, Duration) {
    let began = Instant::now();
    let (mut session, rx) = engine.session().unwrap_or_else(|e| fail(&e));
    for chunk in audio.chunks(SAMPLE_RATE / 10) {
        session.feed(chunk).unwrap_or_else(|e| fail(&e));
    }
    session.stop();
    let mut events = Vec::new();
    for event in rx.iter() {
        let done = matches!(event, Event::Done(_));
        events.push(event);
        if done {
            break;
        }
    }
    (events, began.elapsed())
}

fn finals(events: &[Event]) -> Vec<(&str, &str)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Segment {
                id,
                text,
                is_final: true,
                ..
            } => Some((id.as_str(), text.as_str())),
            _ => None,
        })
        .collect()
}

fn read_wav(path: &Path) -> Vec<f32> {
    let wave = sherpa_onnx::Wave::read(&path.to_string_lossy())
        .unwrap_or_else(|| fail(&format!("cannot read {}", path.display())));
    if wave.sample_rate() as usize != SAMPLE_RATE {
        fail(&format!("{} must be 16 kHz mono", path.display()));
    }
    wave.samples().to_vec()
}

fn selftest(engine: &Engine, config: &EngineConfig) {
    let wavs = engine_test_wavs(&config.models_dir);
    let silence = vec![0.0f32; SAMPLE_RATE];
    let audio: Vec<f32> = wavs
        .iter()
        .flat_map(|w| read_wav(w).into_iter().chain(silence.iter().copied()))
        .collect();
    let (events, wall) = run(engine, &audio);

    let finals = finals(&events);
    for (id, text) in &finals {
        println!("[{id}] {text}");
    }
    let speakers: Vec<&(String, String)> = events
        .iter()
        .filter_map(|e| {
            if let Event::Speakers(s) = e {
                Some(s)
            } else {
                None
            }
        })
        .flatten()
        .collect();
    println!("speakers: {speakers:?}");
    let partials = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                Event::Segment {
                    is_final: false,
                    ..
                }
            )
        })
        .count();
    let (rss, peak) = memory_stats().unwrap_or((0.0, 0.0));
    let audio_secs = audio.len() as f64 / SAMPLE_RATE as f64;
    println!(
        "{audio_secs:.1}s audio in {:.2}s ({partials} partials) | rss after load {:.0} MB, now {rss:.0} MB, peak {peak:.0} MB",
        wall.as_secs_f64(),
        engine.load_rss_mb
    );

    assert!(
        !finals.is_empty() && finals.iter().all(|(_, t)| !t.is_empty()),
        "no transcript produced"
    );
    if config.diarize {
        assert!(
            finals
                .iter()
                .all(|(id, _)| speakers.iter().any(|(s, _)| s == id)),
            "a final segment has no speaker"
        );
    }
    if let Some(Event::Done(text)) = events.last() {
        println!("final text: {text}");
        assert!(!text.is_empty(), "empty final transcript");
    }
    assert!(
        matches!(events.last(), Some(Event::Done(_))),
        "session did not finish"
    );
    assert!(
        !events.iter().any(|e| matches!(e, Event::Error(_))),
        "engine reported an error"
    );
    assert!(
        peak < 1024.0,
        "peak RSS {peak:.0} MB is over the 1 GB budget"
    );
    let (errors, words) = word_errors(
        &expected_selftest_text(),
        &finals.iter().map(|(_, t)| *t).collect::<Vec<_>>().join(" "),
    );
    println!(
        "WER {:.1}% ({errors}/{words})",
        100.0 * errors as f64 / words as f64
    );
    println!("selftest passed");
}

fn engine_test_wavs(models_dir: &Path) -> Vec<PathBuf> {
    let wavs = models_dir.join("test_wavs");
    vec![wavs.join("0.wav"), wavs.join("1.wav")]
}

fn expected_selftest_text() -> String {
    "after early nightfall the yellow lamps would light up here and there the squalid quarter of the brothels \
     god as a direct consequence of the sin which man thus punished had given her a lovely child whose place was \
     on that same dishonored bosom to connect her parent for ever with the race and descent of mortals and to be \
     finally a blessed soul in heaven"
        .into()
}

/// `DIR/*.wav` with matching `DIR/*.txt` references: prints per-file and total WER, RTF and peak RSS.
/// Mixes deterministic household-style noise into `audio`: pink noise with a slowly breathing level
/// (a fan), mains hum and its harmonics, at `snr_db` below the speech's RMS.
fn add_noise(audio: &mut [f32], snr_db: f32) {
    let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt();
    let mut state = 0x2545_F491u32;
    let mut white = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    let (mut b0, mut b1, mut b2) = (0.0f32, 0.0f32, 0.0f32);
    let noise: Vec<f32> = (0..audio.len())
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let w = white();
            b0 = 0.99765 * b0 + w * 0.099046;
            b1 = 0.963 * b1 + w * 0.2965164;
            b2 = 0.57 * b2 + w * 1.0526913;
            let pink = (b0 + b1 + b2 + w * 0.1848) * (1.0 + 0.3 * (std::f32::consts::TAU * 0.3 * t).sin());
            let hum = 0.35 * (std::f32::consts::TAU * 50.0 * t).sin() + 0.15 * (std::f32::consts::TAU * 150.0 * t).sin();
            pink + hum
        })
        .collect();
    let scale = rms(audio) / 10f32.powf(snr_db / 20.0) / rms(&noise).max(1e-9);
    for (a, n) in audio.iter_mut().zip(&noise) {
        *a = (*a + n * scale).clamp(-1.0, 1.0);
    }
}

fn eval(engine: &Engine, dir: &Path, noise_snr: Option<f32>) {
    let mut wavs: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| fail(&format!("{}: {e}", dir.display())))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "wav") && p.with_extension("txt").is_file())
        .collect();
    wavs.sort();
    if wavs.is_empty() {
        fail("no .wav files with matching .txt references");
    }
    let (mut errors, mut words, mut audio_secs, mut wall) = (0, 0, 0.0, 0.0);
    for wav in &wavs {
        let mut audio = read_wav(wav);
        if let Some(snr) = noise_snr {
            add_noise(&mut audio, snr);
        }
        let reference = std::fs::read_to_string(wav.with_extension("txt")).unwrap_or_default();
        let (events, took) = run(engine, &audio);
        let hypothesis = finals(&events)
            .iter()
            .map(|(_, t)| *t)
            .collect::<Vec<_>>()
            .join(" ");
        let (e, w) = word_errors(&reference, &hypothesis);
        let secs = audio.len() as f64 / SAMPLE_RATE as f64;
        println!(
            "{:>6.1}% WER  {secs:>5.1}s  {}",
            100.0 * e as f64 / w.max(1) as f64,
            wav.file_name().unwrap_or_default().to_string_lossy()
        );
        if e > 0 {
            println!(
                "        ref: {}\n        hyp: {hypothesis}",
                reference.trim()
            );
        }
        (errors, words, audio_secs, wall) = (
            errors + e,
            words + w,
            audio_secs + secs,
            wall + took.as_secs_f64(),
        );
    }
    let (_, peak) = memory_stats().unwrap_or((0.0, 0.0));
    println!(
        "TOTAL {:.2}% WER ({errors}/{words} words) over {} files, {audio_secs:.0}s audio, RTF {:.3} incl. diarization, peak RSS {peak:.0} MB",
        100.0 * errors as f64 / words.max(1) as f64,
        wavs.len(),
        wall / audio_secs
    );
}

/// One raw dictation per line: prints cleaned text, guardrail decision, latency and RAM.
fn cleanup_test(config: &EngineConfig, file: &Path, model: Option<&Path>, chat: bool, control: &str) {
    use voicio_engine::cleanup::{Cleaner, Style, LABEL};
    let began = Instant::now();
    let cleaner = match model {
        Some(path) => Cleaner::load_file(path, config.threads, if chat { Style::Chat } else { Style::S1 }),
        None => Cleaner::load(&config.models_dir, config.threads),
    }
    .unwrap_or_else(|e| fail(&e));
    voicio_engine::release_memory();
    let (rss, _) = memory_stats().unwrap_or((0.0, 0.0));
    println!(
        "{LABEL} loaded in {:.2}s, rss {rss:.0} MB",
        began.elapsed().as_secs_f64()
    );
    let lines =
        std::fs::read_to_string(file).unwrap_or_else(|e| fail(&format!("{}: {e}", file.display())));
    let (mut total, mut count, mut fallbacks) = (Duration::ZERO, 0, 0);
    for raw in lines.lines().filter(|l| !l.trim().is_empty()) {
        let t = Instant::now();
        let (cleaned, said) = cleaner.clean_traced(raw, &voicio_engine::style::CleanupStyle::decode(control).resolve(raw));
        let took = t.elapsed();
        (total, count) = (total + took, count + 1);
        fallbacks += usize::from(!cleaned.used_model);
        println!(
            "{:>5}ms {} {raw}\n        -> {}",
            took.as_millis(),
            if cleaned.used_model { "model" } else { "RULES" },
            cleaned.text
        );
        if !cleaned.used_model && !said.is_empty() {
            println!("        (model said, guardrail refused: {})", said.join(" | "));
        }
    }
    let (rss, peak) = memory_stats().unwrap_or((0.0, 0.0));
    println!("{count} lines, mean {:.0} ms, {fallbacks} guardrail fallbacks, rss {rss:.0} MB, peak {peak:.0} MB", total.as_secs_f64() * 1000.0 / count.max(1) as f64);
}

fn eval_tsv(config: &EngineConfig, file: &Path, model: Option<&Path>, chat: bool, verbose: bool, control: &str) {
    use voicio_engine::cleanup::{Cleaner, Style};
    let began = Instant::now();
    let cleaner = match model {
        Some(path) => Cleaner::load_file(path, config.threads, if chat { Style::Chat } else { Style::S1 }),
        None => Cleaner::load(&config.models_dir, config.threads),
    }
    .unwrap_or_else(|e| fail(&e));
    voicio_engine::release_memory();
    let (rss, _) = memory_stats().unwrap_or((0.0, 0.0));
    let name = model.map(|m| m.file_name().unwrap_or_default().to_string_lossy().into_owned()).unwrap_or_else(|| "s1-mini".into());
    println!("{name} loaded in {:.2}s, rss {rss:.0} MB", began.elapsed().as_secs_f64());
    bake::run(file, Some((&name, &cleaner)), verbose, voicio_engine::style::CleanupStyle::decode(control));
    let (rss, peak) = memory_stats().unwrap_or((0.0, 0.0));
    println!("rss after run {rss:.0} MB, peak {peak:.0} MB");
}

/// `--tie IN OUT`: rewrite a tied-embedding GGUF without its duplicate `output.weight` (about 127 MB smaller).
fn tie(from: &Path, to: &Path) -> Result<Args, String> {
    match voicio_engine::gguf::drop_tensor(from, to, "output.weight") {
        Ok(true) => println!("wrote {} ({} MB)", to.display(), std::fs::metadata(to).map(|m| m.len() / 1_000_000).unwrap_or(0)),
        Ok(false) => println!("{} has no output.weight: nothing to drop", from.display()),
        Err(e) => return Err(format!("{}: {e}", from.display())),
    }
    std::process::exit(0)
}
