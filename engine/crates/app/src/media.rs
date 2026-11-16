//! Opt-in "pause music while I talk" (`pause_media` in config.toml): on Linux, every MPRIS player
//! that is playing when a dictation starts is paused, and exactly those are resumed when it ends.
//! Runs on its own thread so a slow player never holds up the microphone.

use std::sync::Mutex;

/// Bus names of the players Voicio paused (and so may resume).
static PAUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

pub fn pause_playing() {
    let _ = std::thread::Builder::new().name("media-pause".into()).spawn(|| {
        let Ok(connection) = zbus::blocking::Connection::session() else { return };
        let Ok(bus) = zbus::blocking::fdo::DBusProxy::new(&connection) else { return };
        let Ok(names) = bus.list_names() else { return };
        let mut paused = Vec::new();
        for name in names.iter().map(|n| n.as_str()).filter(|n| n.starts_with("org.mpris.MediaPlayer2.")) {
            let Ok(player) = zbus::blocking::Proxy::new(&connection, name, "/org/mpris/MediaPlayer2", PLAYER) else { continue };
            if player.get_property::<String>("PlaybackStatus").is_ok_and(|s| s == "Playing") && player.call_method("Pause", &()).is_ok() {
                paused.push(name.to_string());
            }
        }
        PAUSED.lock().unwrap_or_else(|e| e.into_inner()).extend(paused);
    });
}

pub fn resume_paused() {
    let names = std::mem::take(&mut *PAUSED.lock().unwrap_or_else(|e| e.into_inner()));
    if names.is_empty() {
        return;
    }
    let _ = std::thread::Builder::new().name("media-resume".into()).spawn(move || {
        let Ok(connection) = zbus::blocking::Connection::session() else { return };
        for name in names {
            if let Ok(player) = zbus::blocking::Proxy::new(&connection, name.as_str(), "/org/mpris/MediaPlayer2", PLAYER) {
                let _ = player.call_method("Play", &());
            }
        }
    });
}
