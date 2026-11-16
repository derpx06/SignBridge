//! `voicio-engine worker`: the model host the Voicio agent spawns on demand.
//!
//! Speaks `voicio_ipc` frames on stdin/stdout. It exits on `Shutdown`, on stdin EOF, and (Linux)
//! when the agent dies, so models never outlive their owner; the agent unloads them by ending this
//! process, which returns every byte to the OS.

use std::io::{BufReader, BufWriter, Write};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};
use sherpa_onnx::LinearResampler;
use voicio_engine::{cleanup, memory_stats, Engine, EngineConfig, Event, SAMPLE_RATE};
use voicio_ipc::{read_frame, write_control, Frame, FromWorker, ToWorker};

pub fn run(mut config: EngineConfig) -> ! {
    voicio_ipc::heap::pin_mmap_threshold();
    die_with_parent();
    lower_priority();
    let mut out = BufWriter::new(take_stdout_for_ipc());
    // Dictation profile: no speaker labels; the cleanup LLM loads per session, on demand.
    config.diarize = false;
    config.cleanup = false;
    let engine = match Engine::load(&config) {
        Ok(engine) => engine,
        Err(message) => {
            let _ = write_control(&mut out, &FromWorker::Error { message });
            std::process::exit(1)
        }
    };
    send(&mut out, &FromWorker::Ready {
        model: format!("{} + {}", engine.label, cleanup::LABEL),
        rss_mb: engine.load_rss_mb.round(),
    });

    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("ipc-in".into())
        .spawn(move || {
            let mut stdin = BufReader::new(std::io::stdin().lock());
            loop {
                match read_frame::<_, ToWorker>(&mut stdin) {
                    Ok(Some(frame)) => {
                        if tx.send(frame).is_err() {
                            return;
                        }
                    }
                    // EOF or a corrupt stream: the agent is gone or broken either way.
                    Ok(None) | Err(_) => {
                        let _ = tx.send(Frame::Control(ToWorker::Shutdown));
                        return;
                    }
                }
            }
        })
        .expect("spawn ipc reader");

    let mut session = None;
    let mut session_id = 0u64;
    let mut resampler: Option<LinearResampler> = None;
    let mut last_stats = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(Frame::Control(ToWorker::Start { cleanup, sample_rate, session: id, denoise, style, partials })) => {
                session_id = id;
                session = None; // a new Start supersedes any unfinished dictation
                resampler = None;
                if sample_rate as usize != SAMPLE_RATE {
                    resampler = LinearResampler::create(sample_rate as i32, SAMPLE_RATE as i32);
                    if resampler.is_none() {
                        let message = format!("cannot resample {sample_rate} Hz audio");
                        send(&mut out, &FromWorker::Error { message });
                        continue;
                    }
                }
                match engine.session_styled(cleanup, denoise, voicio_engine::style::CleanupStyle::decode(&style)) {
                    Ok(mut s) => {
                        s.0.set_partials(partials);
                        session = Some(s)
                    }
                    Err(message) => send(&mut out, &FromWorker::Error { message }),
                }
            }
            Ok(Frame::Audio(samples)) => {
                let samples = match &resampler {
                    Some(r) => r.resample(&samples, false),
                    None => samples,
                };
                if let Some((s, _)) = session.as_mut() {
                    if let Err(message) = s.feed(&samples) {
                        send(&mut out, &FromWorker::Error { message });
                        s.stop();
                    }
                }
            }
            Ok(Frame::Control(ToWorker::Stop)) => {
                match session.as_mut() {
                    Some((s, _)) => s.stop(),
                    // The Start failed (reported as an Error): never leave the agent waiting.
                    None => send(&mut out, &FromWorker::Done { text: String::new(), session: session_id }),
                }
            }
            Ok(Frame::Control(ToWorker::Cancel)) => session = None,
            Ok(Frame::Control(ToWorker::Trim)) => engine.trim(),
            Ok(Frame::Control(ToWorker::Clean { text, session: id, cleanup, style })) => {
                // ponytail: blocks this loop for the cleanup (about a second); no audio flows meanwhile
                // because the dictation has already stopped.
                session = None;
                let text = engine.clean_text(text, cleanup, voicio_engine::style::CleanupStyle::decode(&style));
                send(&mut out, &FromWorker::Done { text, session: id });
            }
            Ok(Frame::Control(ToWorker::Shutdown)) | Err(RecvTimeoutError::Disconnected) => {
                // Skip destructors: the OS reclaims model memory faster than freeing it piecemeal.
                let _ = out.flush();
                std::process::exit(0)
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        let mut finished = false;
        if let Some((_, events)) = &session {
            while let Ok(event) = events.try_recv() {
                let message = match event {
                    Event::Segment { id, text, start_ms, end_ms, is_final } => {
                        FromWorker::Segment { id, text, start_ms, end_ms, is_final }
                    }
                    Event::Done(text) => {
                        finished = true;
                        FromWorker::Done { text, session: session_id }
                    }
                    Event::Error(message) => FromWorker::Error { message },
                    Event::Speakers(_) => continue,
                };
                send(&mut out, &message);
            }
        }
        if finished {
            session = None;
            voicio_ipc::heap::trim();
        }
        if last_stats.elapsed() >= Duration::from_secs(2) {
            last_stats = Instant::now();
            let (rss_mb, peak_mb) = memory_stats().unwrap_or((0.0, 0.0));
            send(&mut out, &FromWorker::Stats { rss_mb: rss_mb.round(), peak_mb: peak_mb.round() });
        }
    }
}

/// A failed write means the agent is gone: exit rather than keep models alive for nobody.
fn send(out: &mut impl Write, message: &FromWorker) {
    if write_control(out, message).is_err() {
        std::process::exit(0);
    }
}

/// Dictation is latency-tolerant background work: nice +5 so it never competes with the user's apps.
/// Threads spawned afterwards inherit it.
#[cfg(unix)]
fn lower_priority() {
    extern "C" {
        fn setpriority(which: i32, who: u32, prio: i32) -> i32;
    }
    unsafe {
        setpriority(0, 0, 5); // PRIO_PROCESS, calling thread (Linux) / process
    }
}

#[cfg(not(unix))]
fn lower_priority() {}

/// Linux: the kernel kills us when the agent's spawning thread dies (PR_SET_PDEATHSIG).
#[cfg(target_os = "linux")]
fn die_with_parent() {
    extern "C" {
        fn prctl(option: i32, ...) -> i32;
        fn getppid() -> i32;
    }
    const PR_SET_PDEATHSIG: i32 = 1;
    const SIGKILL: std::ffi::c_ulong = 9;
    unsafe {
        prctl(PR_SET_PDEATHSIG, SIGKILL);
    }
    // If the agent died before prctl took effect, no signal will ever come.
    let expected = std::env::var("VOICIO_PARENT_PID").ok().and_then(|p| p.parse::<i32>().ok());
    if expected.is_some_and(|pid| pid != unsafe { getppid() }) {
        std::process::exit(0);
    }
}

#[cfg(not(target_os = "linux"))]
fn die_with_parent() {} // stdin EOF covers macOS; Windows uses a kill-on-close Job Object

/// Moves the IPC stream to a private descriptor and points fd 1 at stderr, so any stray print
/// from the native libraries (ONNX Runtime, llama.cpp) can never corrupt the protocol.
#[cfg(unix)]
fn take_stdout_for_ipc() -> Box<dyn Write + Send> {
    use std::os::fd::FromRawFd;
    extern "C" {
        fn dup(fd: i32) -> i32;
        fn dup2(old: i32, new: i32) -> i32;
    }
    unsafe {
        let ipc = dup(1);
        if ipc < 0 || dup2(2, 1) < 0 {
            return Box::new(std::io::stdout());
        }
        Box::new(std::fs::File::from_raw_fd(ipc))
    }
}

#[cfg(not(unix))]
fn take_stdout_for_ipc() -> Box<dyn Write + Send> {
    Box::new(std::io::stdout())
}
