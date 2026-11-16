//! Single instance + local control channel (`voicio toggle`, `voicio status`, ...).
//!
//! The lock is an OS file lock, released by the kernel when the process dies, so it can never go
//! stale. The control socket is a Unix socket / Windows named pipe in the user's namespace.

use crate::agent::{Cmd, Msg};
use interprocess::local_socket::{prelude::*, GenericFilePath, GenericNamespaced, ListenerOptions, Name, Stream};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

/// Per user, plus an optional `VOICIO_INSTANCE` suffix so tests never touch the real agent.
fn instance() -> String {
    let user = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_else(|_| "user".into());
    match std::env::var("VOICIO_INSTANCE") {
        Ok(suffix) if !suffix.is_empty() => format!("voicio-{user}-{suffix}"),
        _ => format!("voicio-{user}"),
    }
}

/// Linux: a socket file in $XDG_RUNTIME_DIR (a 0700 directory), so other users cannot reach it.
/// An abstract-namespace socket has no permissions at all. Windows named pipes and macOS are
/// per-user already.
fn socket_name() -> std::io::Result<Name<'static>> {
    let file = format!("{}.sock", instance());
    if cfg!(target_os = "linux") {
        let dir = directories::BaseDirs::new()
            .and_then(|d| d.runtime_dir().map(PathBuf::from))
            .ok_or_else(|| std::io::Error::other("XDG_RUNTIME_DIR is not set"))?;
        dir.join(file).into_os_string().to_fs_name::<GenericFilePath>()
    } else {
        file.to_ns_name::<GenericNamespaced>()
    }
}

fn lock_path() -> PathBuf {
    let dir = directories::BaseDirs::new()
        .and_then(|d| d.runtime_dir().map(PathBuf::from))
        .unwrap_or_else(std::env::temp_dir);
    dir.join(format!("{}.lock", instance()))
}

/// `Some(lock)` if we are the only running agent; keep the file alive for the process lifetime.
pub fn acquire() -> std::io::Result<Option<File>> {
    let file = File::options().create(true).truncate(false).write(true).open(lock_path())?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(e)) => Err(e),
    }
}

/// Serves control commands on a background thread. Call only while holding the lock.
pub fn serve(tx: Sender<Msg>) -> std::io::Result<()> {
    let listener = match ListenerOptions::new().name(socket_name()?).create_sync() {
        Ok(listener) => listener,
        // We hold the lock, so an existing socket file is a leftover from a crash: replace it.
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            ListenerOptions::new().name(socket_name()?).try_overwrite(true).create_sync()?
        }
        Err(e) => return Err(e),
    };
    std::thread::Builder::new().name("control-socket".into()).spawn(move || {
        for connection in listener.incoming().flatten() {
            // One thread per client: a client that connects and says nothing blocks only itself.
            let tx = tx.clone();
            let _ = std::thread::Builder::new().name("control-client".into()).spawn(move || {
                let mut connection = BufReader::new(connection);
                let mut line = String::new();
                if connection.read_line(&mut line).is_err() {
                    return;
                }
                let reply = handle(line.trim(), &tx);
                let _ = connection.get_mut().write_all(format!("{reply}\n").as_bytes());
            });
        }
    })?;
    Ok(())
}

fn handle(command: &str, tx: &Sender<Msg>) -> String {
    let simple = match command {
        "start" => Some(Cmd::Start),
        "stop" => Some(Cmd::Stop),
        "toggle" => Some(Cmd::Toggle),
        "cancel" => Some(Cmd::Cancel),
        "settings" => Some(Cmd::OpenSettings),
        "quit" => Some(Cmd::Quit),
        _ => None,
    };
    if let Some(cmd) = simple {
        return match tx.send(Msg::Command(cmd)) {
            Ok(()) => "ok".into(),
            Err(_) => "error: agent is shutting down".into(),
        };
    }
    if command == "status" {
        let (reply_tx, reply_rx) = mpsc::channel();
        if tx.send(Msg::Command(Cmd::Status(reply_tx))).is_ok() {
            if let Ok(s) = reply_rx.recv_timeout(Duration::from_secs(2)) {
                let (key, mode) = s.shortcut.unwrap_or_else(|| ("none".into(), "none".into()));
                return format!(
                    "state={} models={} rss_mb={:.0} peak_mb={:.0} model={} shortcut={key} mode={mode}",
                    s.state,
                    if s.worker_loaded { "loaded" } else { "unloaded" },
                    s.rss_mb,
                    s.peak_mb,
                    s.model
                );
            }
        }
        return "error: no answer from agent".into();
    }
    format!("error: unknown command {command:?}")
}

/// Client side: sends one command to the running agent and returns its reply.
pub fn send(command: &str) -> std::io::Result<String> {
    let mut stream = BufReader::new(Stream::connect(socket_name()?)?);
    stream.get_mut().write_all(format!("{command}\n").as_bytes())?;
    let mut reply = String::new();
    stream.read_line(&mut reply)?;
    Ok(reply.trim().to_string())
}
