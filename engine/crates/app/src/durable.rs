//! Crash-safe file writes: a reader (or the next start) sees the old file or the new one, never a
//! half-written one, and private files are readable by their owner only. The idea follows Hex's
//! `durable::write` (https://github.com/anomalyco/hex, MIT; see THIRD_PARTY_NOTICES.md).

use std::io::Write;
use std::path::Path;

/// Writes `bytes` to `path` atomically (temp file in the same folder, fsync, rename).
pub fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", path.extension().and_then(|e| e.to_str()).unwrap_or("dat")));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}

/// Moves an unreadable file aside (`name.bad`, `name.bad.1`, ...) so a fresh one can be written
/// without destroying what the user had. Returns where it went.
pub fn set_aside(path: &Path) -> Option<std::path::PathBuf> {
    for n in 0..50 {
        let name = path.file_name()?.to_string_lossy().into_owned();
        let target = path.with_file_name(if n == 0 { format!("{name}.bad") } else { format!("{name}.bad.{n}") });
        if !target.exists() {
            return std::fs::rename(path, &target).ok().map(|()| target);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("voicio-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_write_replaces_the_file_whole_and_leaves_no_temp_file() {
        let d = dir("write");
        let f = d.join("a.toml");
        write(&f, b"one").unwrap();
        write(&f, b"two").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn a_damaged_file_is_set_aside_not_overwritten() {
        let d = dir("aside");
        let f = d.join("config.toml");
        std::fs::write(&f, "not = [valid").unwrap();
        let moved = set_aside(&f).unwrap();
        assert!(!f.exists() && moved.exists());
        std::fs::write(&f, "again").unwrap();
        let second = set_aside(&f).unwrap();
        assert_ne!(moved, second, "an earlier .bad is never clobbered");
    }
}
