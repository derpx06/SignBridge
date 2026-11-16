//! Types text into the focused app through the XDG RemoteDesktop portal, for Wayland desktops
//! that have no other way (GNOME without the Voicio extension, KDE). The first use shows the
//! desktop's own "allow remote interaction" prompt; the answer is kept as a restore token, so
//! later dictations type silently. The portal session is closed after a few idle seconds (the
//! desktop shows a "remote control" indicator while one is open). Typing runs on its own thread,
//! never the UI thread, and every failure falls back to the clipboard.

use ashpd::desktop::remote_desktop::{DeviceType, KeyState, NotifyKeyboardKeysymOptions, RemoteDesktop, SelectDevicesOptions, StartOptions};
use ashpd::desktop::{CreateSessionOptions, PersistMode, Session};
use ashpd::enumflags2::BitFlags;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::time::Duration;

const APP_ID: &str = "io.github.voicio.Voicio";
/// How long an idle portal session stays open.
const IDLE: Duration = Duration::from_secs(8);

type Done = Box<dyn FnOnce(Result<(), String>) + Send>;
struct Job {
    text: String,
    done: Done,
}

struct Open {
    portal: RemoteDesktop,
    session: Session<RemoteDesktop>,
}

/// Whether Voicio can type without a prompt: the desktop was already told "allow" (the restore token
/// exists), or `wtype` is installed (tried first off GNOME, no permission needed).
pub fn allowed() -> bool {
    token_path().is_some_and(|p| p.is_file()) || (!is_gnome() && have_wtype())
}

fn have_wtype() -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("wtype").is_file()))
}

fn token_path() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("io.github", "voicio", "voicio").map(|d| d.config_dir().join("portal-token"))
}

fn save_token(token: &str) {
    use std::os::unix::fs::PermissionsExt;
    if let Some(path) = token_path() {
        let _ = std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")));
        if std::fs::write(&path, token).is_ok() {
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
    }
}

async fn open() -> Result<Open, String> {
    let app_id = ashpd::AppID::try_from(APP_ID).map_err(|e| e.to_string())?;
    // Ignore "already registered": hotkey.rs registers the same id.
    let _ = ashpd::register_host_app(app_id).await;
    let portal = RemoteDesktop::new().await.map_err(|e| format!("remote-desktop portal: {e}"))?;
    let session = portal.create_session(CreateSessionOptions::default()).await.map_err(|e| e.to_string())?;
    let token = token_path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    portal
        .select_devices(
            &session,
            SelectDevicesOptions::default()
                .set_devices(BitFlags::from(DeviceType::Keyboard))
                .set_persist_mode(PersistMode::ExplicitlyRevoked)
                .set_restore_token(token.as_deref()),
        )
        .await
        .map_err(|e| e.to_string())?
        .response()
        .map_err(|e| e.to_string())?;
    // The first time this blocks on the desktop's permission prompt.
    let started = portal
        .start(&session, None, StartOptions::default())
        .await
        .map_err(|e| e.to_string())?
        .response()
        .map_err(|_| "typing was not allowed (answer the desktop's prompt with Allow)".to_string())?;
    if let Some(token) = started.restore_token() {
        save_token(token);
    }
    Ok(Open { portal, session })
}

/// The X11 keysym that types `c` (Latin-1 keysyms equal their code points; the rest use the
/// Unicode range).
fn keysym(c: char) -> i32 {
    match c {
        '\n' | '\r' => 0xff0d,
        '\t' => 0xff09,
        c if (' '..='~').contains(&c) || ('\u{a0}'..='\u{ff}').contains(&c) => c as i32,
        c => 0x0100_0000 + c as i32,
    }
}

async fn type_with(open: &Open, text: &str) -> Result<(), String> {
    for c in text.chars() {
        let sym = keysym(c);
        for state in [KeyState::Pressed, KeyState::Released] {
            open.portal
                .notify_keyboard_keysym(&open.session, sym, state, NotifyKeyboardKeysymOptions::default())
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn worker(rx: mpsc::Receiver<Job>) {
    let mut open_session: Option<Open> = None;
    loop {
        let job = if open_session.is_some() {
            match rx.recv_timeout(IDLE) {
                Ok(job) => job,
                Err(RecvTimeoutError::Timeout) => {
                    if let Some(old) = open_session.take() {
                        let _ = zbus::block_on(old.session.close());
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(job) => job,
                Err(_) => return,
            }
        };
        if !is_gnome() && wtype(&job.text).is_ok() {
            (job.done)(Ok(()));
            continue;
        }
        let result = zbus::block_on(async {
            if open_session.is_none() {
                open_session = Some(open().await?);
            }
            let result = type_with(open_session.as_ref().expect("opened above"), &job.text).await;
            if result.is_err() {
                open_session = None; // a dead session: the next dictation opens a new one
            }
            result
        });
        (job.done)(result);
    }
}

/// `wtype` (the Wayland virtual-keyboard protocol) types without any permission prompt, on
/// compositors that offer it (Sway, Hyprland, river, niri); GNOME and KDE do not (KWin lacks the virtual-keyboard protocol, so the call fails fast and the portal takes over), so it goes
/// straight to the portal there. The text is passed on stdin, never as an argument, so it cannot
/// be read from the process list.
fn wtype(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("wtype")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("wtype: {e}"))?;
    child.stdin.take().ok_or("wtype: no stdin")?.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("wtype: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

fn is_gnome() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.split(':').any(|p| p.eq_ignore_ascii_case("GNOME")))
}

/// Types `text` on the typing thread and reports the outcome there (not on the caller's thread).
pub fn type_async(text: String, done: impl FnOnce(Result<(), String>) + Send + 'static) {
    static WORKER: OnceLock<Sender<Job>> = OnceLock::new();
    let tx = WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new().name("portal-typing".into()).spawn(move || worker(rx));
        tx
    });
    if let Err(e) = tx.send(Job { text, done: Box::new(done) }) {
        (e.0.done)(Err("the typing thread is gone".into()));
    }
}

#[cfg(test)]
mod tests {
    use super::keysym;

    #[test]
    fn keysyms_cover_ascii_latin1_unicode_and_controls() {
        assert_eq!(keysym('a'), 0x61);
        assert_eq!(keysym('A'), 0x41);
        assert_eq!(keysym(' '), 0x20);
        assert_eq!(keysym('é'), 0xe9);
        assert_eq!(keysym('\n'), 0xff0d);
        assert_eq!(keysym('€'), 0x0100_20ac);
        assert_eq!(keysym("नमस्ते".chars().next().unwrap()), 0x0100_0000 + 0x928);
    }
}
