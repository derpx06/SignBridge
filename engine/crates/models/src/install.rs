//! Downloading and verifying model files: resumable, checksum-pinned, cancellable.
//! A file only appears under its real name once its SHA-256 matches, so a crash or a lost
//! connection can never leave a half-written model that looks installed.

use crate::{FileSpec, Model, Source};
use sha2::{Digest, Sha256};
use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub enum InstallError {
    Cancelled,
    NoSpace { need: u64, have: u64 },
    Network(String),
    Checksum(String),
    Io(String),
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstallError::Cancelled => write!(f, "Download cancelled."),
            InstallError::NoSpace { need, have } => write!(
                f,
                "Not enough disk space: {} needed, {} free.",
                crate::format_bytes(*need),
                crate::format_bytes(*have)
            ),
            InstallError::Network(e) => write!(f, "Download failed: {e}. Check your connection and try again; it will resume."),
            InstallError::Checksum(file) => write!(f, "{file} did not download correctly (checksum mismatch). It was discarded; try again."),
            InstallError::Io(e) => write!(f, "Could not write the model files: {e}"),
        }
    }
}

impl From<std::io::Error> for InstallError {
    fn from(e: std::io::Error) -> Self {
        InstallError::Io(e.to_string())
    }
}

/// Shared flag: set it from any thread to stop a running install.
#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

pub struct Progress {
    pub done: u64,
    pub total: u64,
    /// "Downloading", "Unpacking" or "Verifying".
    pub phase: &'static str,
}

const CHUNK: usize = 256 * 1024;
const ATTEMPTS: u32 = 4;

fn agent() -> ureq::Agent {
    let builder = ureq::Agent::config_builder();
    #[cfg(not(target_os = "linux"))]
    let builder = builder.tls_config(ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).build());
    builder
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .build()
        .into()
}

pub(crate) fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&buf[..n]);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
}

/// Free bytes on the disk that holds `dir`.
fn free_space(dir: &Path) -> Option<u64> {
    let dir = dir.canonicalize().ok()?;
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks.iter().filter(|d| dir.starts_with(d.mount_point())).max_by_key(|d| d.mount_point().as_os_str().len()).map(|d| d.available_space())
}

/// Installs every missing file of `model` into `dir`. Safe to call again after any failure.
pub fn install(
    dir: &Path,
    model: &Model,
    cancel: &Cancel,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<(), InstallError> {
    let present = |f: &FileSpec| std::fs::metadata(dir.join(f.path)).is_ok_and(|m| m.len() == f.size);
    let missing: Vec<&FileSpec> = model.files().iter().filter(|f| !present(f)).collect();
    if missing.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    let total = match &model.source {
        Source::Files(_) => missing.iter().map(|f| f.size).sum(),
        Source::Archive { size, .. } => *size,
    };
    // Room for the download plus the unpacked files, with a margin.
    let need = (total + missing.iter().map(|f| f.size).sum::<u64>()) / 10 * 11;
    if let Some(have) = free_space(dir) {
        if have < need {
            return Err(InstallError::NoSpace { need, have });
        }
    }
    let agent = agent();
    let mut done = 0u64;
    match &model.source {
        Source::Files(_) => {
            for file in missing {
                fetch_verified(&agent, file.url, &dir.join(file.path), file.size, file.sha256, cancel, &mut done, total, on_progress)?;
            }
        }
        Source::Archive { url, size, sha256, files } => {
            let archive = dir.join(".downloads").join(format!("{}.tar.bz2", model.id));
            std::fs::create_dir_all(archive.parent().expect("has a parent"))?;
            fetch_verified(&agent, url, &archive, *size, sha256, cancel, &mut done, total, on_progress)?;
            on_progress(Progress { done: total, total, phase: "Unpacking" });
            unpack(&archive, dir, files, cancel)?;
            let _ = std::fs::remove_file(&archive);
        }
    }
    Ok(())
}

/// Downloads `url` to `dest` through `dest.part`, resuming where it left off, then verifies.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fetch_verified(
    agent: &ureq::Agent,
    url: &str,
    dest: &Path,
    size: u64,
    sha256: &str,
    cancel: &Cancel,
    done: &mut u64,
    total: u64,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<(), InstallError> {
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let part = part_path(dest);
    let mut last_error = InstallError::Network("no attempt made".into());
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(500 * 2u64.pow(attempt)));
        }
        match attempt_download(agent, url, &part, size, sha256, cancel, done, total, on_progress) {
            Ok(()) => {
                std::fs::rename(&part, dest)?;
                return Ok(());
            }
            Err(InstallError::Network(e)) => last_error = InstallError::Network(e), // retry, resuming
            Err(other) => return Err(other),
        }
    }
    Err(last_error)
}

#[allow(clippy::too_many_arguments)]
fn attempt_download(
    agent: &ureq::Agent,
    url: &str,
    part: &Path,
    size: u64,
    sha256: &str,
    cancel: &Cancel,
    done: &mut u64,
    total: u64,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<(), InstallError> {
    let mut existing = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    if existing > size {
        std::fs::remove_file(part)?;
        existing = 0;
    }
    let mut hasher = Sha256::new();
    if existing > 0 {
        let mut old = std::fs::File::open(part)?;
        let mut buf = vec![0u8; CHUNK];
        loop {
            let n = old.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
    }
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(part)?;
    let mut written = existing;
    *done += existing;

    if written < size {
        let mut request = agent.get(url);
        if written > 0 {
            request = request.header("Range", format!("bytes={written}-"));
        }
        let response = request.call().map_err(|e| InstallError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        match status {
            206 => {}
            200 if written > 0 => {
                // The server ignored the range: start over.
                *done -= written;
                written = 0;
                file.set_len(0)?;
                hasher = Sha256::new();
            }
            200 => {}
            other => return Err(InstallError::Network(format!("server answered {other}"))),
        }
        let mut reader = response.into_body().into_reader();
        let mut buf = vec![0u8; CHUNK];
        let mut last_report = Instant::now() - Duration::from_secs(1);
        loop {
            if cancel.is_cancelled() {
                file.flush()?;
                return Err(InstallError::Cancelled);
            }
            let n = reader.read(&mut buf).map_err(|e| InstallError::Network(e.to_string()))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            written += n as u64;
            *done += n as u64;
            if last_report.elapsed() >= Duration::from_millis(150) {
                last_report = Instant::now();
                on_progress(Progress { done: *done, total, phase: "Downloading" });
            }
        }
        file.flush()?;
    }
    on_progress(Progress { done: *done, total, phase: "Verifying" });
    if written < size {
        return Err(InstallError::Network("the connection ended early".into()));
    }
    if written > size || hex(&hasher.finalize()) != sha256 {
        let _ = std::fs::remove_file(part);
        return Err(InstallError::Checksum(part.file_name().map(|n| n.to_string_lossy().replace(".part", "")).unwrap_or_default()));
    }
    Ok(())
}

/// Extracts the wanted files from the archive, verifying each before it gets its real name.
fn unpack(archive: &Path, dir: &Path, files: &[FileSpec], cancel: &Cancel) -> Result<(), InstallError> {
    let reader = bzip2::read::BzDecoder::new(std::fs::File::open(archive)?);
    let mut tar = tar::Archive::new(reader);
    let mut remaining: Vec<&FileSpec> = files.iter().collect();
    for entry in tar.entries()? {
        if cancel.is_cancelled() {
            return Err(InstallError::Cancelled);
        }
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().trim_start_matches("./").to_string();
        let Some(index) = remaining.iter().position(|f| f.path == path) else { continue };
        let spec = remaining.swap_remove(index);
        let dest = dir.join(spec.path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let part = part_path(&dest);
        let mut out = std::fs::File::create(&part)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; CHUNK];
        loop {
            let n = entry.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
        }
        out.flush()?;
        if hex(&hasher.finalize()) != spec.sha256 {
            let _ = std::fs::remove_file(&part);
            return Err(InstallError::Checksum(spec.path.to_string()));
        }
        std::fs::rename(part, dest)?;
    }
    if let Some(missing) = remaining.first() {
        return Err(InstallError::Io(format!("the archive is missing {}", missing.path)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    /// A tiny HTTP server for one file: honours Range, and can cut the first answer short.
    fn serve(body: Vec<u8>, cut_first_at: Option<usize>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let mut first = true;
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut start = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(range) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        start = range.trim().trim_end_matches('-').parse().unwrap();
                    }
                }
                let slice = &body[start..];
                let (status, extra) = if start > 0 {
                    ("206 Partial Content", format!("Content-Range: bytes {start}-{}/{}\r\n", body.len() - 1, body.len()))
                } else {
                    ("200 OK", String::new())
                };
                let head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n", slice.len());
                let _ = stream.write_all(head.as_bytes());
                let send = if first { cut_first_at.map_or(slice.len(), |c| c.min(slice.len())) } else { slice.len() };
                first = false;
                let _ = stream.write_all(&slice[..send]);
            }
        });
        url
    }

    fn digest(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    fn data() -> Vec<u8> {
        (0..700_000u32).map(|i| (i % 251) as u8).collect()
    }

    fn run(url: &str, dest: &Path, size: u64, sha: &str, cancel: &Cancel) -> Result<(), InstallError> {
        let mut done = 0;
        fetch_verified(&agent(), url, dest, size, sha, cancel, &mut done, size, &mut |_| {})
    }

    #[test]
    fn downloads_and_verifies() {
        let body = data();
        let url = serve(body.clone(), None);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("m/file.bin");
        run(&url, &dest, body.len() as u64, &digest(&body), &Cancel::default()).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert!(!part_path(&dest).exists());
    }

    #[test]
    fn an_interrupted_download_resumes() {
        let body = data();
        let url = serve(body.clone(), Some(300_000)); // first answer dies part-way
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("file.bin");
        run(&url, &dest, body.len() as u64, &digest(&body), &Cancel::default()).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
    }

    #[test]
    fn a_wrong_checksum_is_discarded_never_installed() {
        let body = data();
        let url = serve(body.clone(), None);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("file.bin");
        let wrong = digest(b"something else");
        let err = run(&url, &dest, body.len() as u64, &wrong, &Cancel::default()).unwrap_err();
        assert!(matches!(err, InstallError::Checksum(_)), "{err:?}");
        assert!(!dest.exists() && !part_path(&dest).exists());
    }

    #[test]
    fn cancel_stops_and_keeps_the_partial_file_for_later() {
        let body = data();
        let url = serve(body.clone(), None);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("file.bin");
        let cancel = Cancel::default();
        cancel.cancel();
        let err = run(&url, &dest, body.len() as u64, &digest(&body), &cancel).unwrap_err();
        assert_eq!(err, InstallError::Cancelled);
        assert!(!dest.exists());
    }
}
