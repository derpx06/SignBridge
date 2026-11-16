//! Spawns and supervises the `voicio-engine worker` model process.
//!
//! The worker exits on stdin EOF and (Linux) when this thread's process dies, so dropping a
//! `Worker` or crashing the agent never leaves models resident. Unloading = ending the process.

use crate::agent::Msg;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};
use voicio_ipc::{read_frame, write_audio, write_control, Frame, FromWorker, ToWorker};

pub struct Worker {
    child: Option<Child>,
    stdin: Option<BufWriter<ChildStdin>>,
    pub generation: u64,
    pub ready: bool,
    pub model: String,
    pub rss_mb: f64,
    pub peak_mb: f64,
}

pub fn locate(configured: Option<&Path>) -> PathBuf {
    if let Some(path) = configured {
        return path.to_path_buf();
    }
    let name = format!("voicio-engine{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .map(|exe| exe.with_file_name(&name))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

impl Worker {
    pub fn spawn(binary: &Path, models_dir: &Path, asr_dir: &str, memory_budget_mb: u32, cpu_mode: &str, language: &str, generation: u64, events: Sender<Msg>) -> std::io::Result<Worker> {
        let mut command = Command::new(binary);
        command
            .arg("worker")
            .arg("--models")
            .arg(models_dir)
            .args(["--asr", asr_dir, "--memory-budget", &memory_budget_mb.to_string(), "--cpu-mode", cpu_mode])
            .env("VOICIO_PARENT_PID", std::process::id().to_string());
        if !language.is_empty() {
            command.args(["--language", language]);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| std::io::Error::new(e.kind(), format!("cannot start {}: {e}", binary.display())))?;
        crate::platform::contain_worker(&child);
        let stdout = child.stdout.take().expect("piped stdout");
        let stdin = child.stdin.take().map(BufWriter::new);
        std::thread::Builder::new().name(format!("worker-{generation}-out")).spawn(move || {
            let mut stdout = BufReader::new(stdout);
            loop {
                match read_frame::<_, FromWorker>(&mut stdout) {
                    Ok(Some(Frame::Control(message))) => {
                        if events.send(Msg::Worker(generation, message)).is_err() {
                            return;
                        }
                    }
                    Ok(Some(Frame::Audio(_))) => {} // the worker never sends audio
                    Ok(None) | Err(_) => {
                        let _ = events.send(Msg::WorkerExited(generation));
                        return;
                    }
                }
            }
        })?;
        Ok(Worker { child: Some(child), stdin, generation, ready: false, model: String::new(), rss_mb: 0.0, peak_mb: 0.0 })
    }

    /// False if the pipe is broken; the reader thread will report the exit.
    pub fn send(&mut self, message: &ToWorker) -> bool {
        self.stdin.as_mut().is_some_and(|w| write_control(w, message).is_ok())
    }

    pub fn audio(&mut self, samples: &[f32]) -> bool {
        self.stdin.as_mut().is_some_and(|w| write_audio(w, samples).is_ok())
    }

    /// Graceful exit, then a hard kill if it does not leave within 2 s.
    pub fn shutdown(mut self) {
        let _ = self.send(&ToWorker::Shutdown);
        self.stdin = None; // EOF is the second, unconditional exit signal
        if let Some(mut child) = self.child.take() {
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if let Ok(Some(_)) = child.try_wait() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Worker {
    /// Never leave a model process behind (and reap it so no zombie remains).
    fn drop(&mut self) {
        self.stdin = None;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
