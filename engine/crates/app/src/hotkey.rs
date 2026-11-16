//! The global dictation shortcut, delivered as `Msg::Hotkey(pressed)` so the agent can do both
//! tap-to-toggle and hold-to-talk.
//!
//! - Windows, macOS, X11: `global-hotkey` (must be created on the UI/main thread).
//! - KDE / Hyprland / other Wayland: the XDG desktop portal GlobalShortcuts (press + release).
//! - GNOME: nothing here; the extension owns the shortcut.
//! - Anywhere: `voicio toggle` can be bound as a desktop shortcut instead.

use crate::agent::Msg;
use crate::desktop::Desktop;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::mpsc::Sender;

pub enum Registration {
    Native(#[allow(dead_code)] GlobalHotKeyManager),
    #[cfg(target_os = "linux")]
    Portal,
    None,
}

/// The desktop's shortcut service accepted Voicio's shortcut (press and release will arrive).
pub fn portal_bound() -> bool {
    #[cfg(target_os = "linux")]
    {
        portal::BOUND.load(std::sync::atomic::Ordering::Relaxed) || matches!(fallback(), Some(Fallback::Active { .. }))
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// The desktop's shortcut service refused (or never answered) the last request.
pub fn portal_failed() -> bool {
    #[cfg(target_os = "linux")]
    {
        portal::FAILED.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// What the GNOME keybinding fallback did after the portal failed (None: not tried or not needed).
#[derive(Clone, Debug, PartialEq)]
pub enum Fallback {
    /// A plain keybinding on `key` now runs `voicio toggle`; the note says so when it is not the asked-for key.
    Active { key: String, note: Option<String> },
    Failed(String),
}

pub fn fallback() -> Option<Fallback> {
    #[cfg(target_os = "linux")]
    {
        portal::FALLBACK.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    #[cfg(not(target_os = "linux"))]
    None
}

/// Asks the desktop for the shortcut again (shows its permission prompt again). Does nothing while
/// a request is already waiting for an answer or once the shortcut is bound.
pub fn retry(shortcut: &str, tx: Sender<Msg>) {
    #[cfg(target_os = "linux")]
    portal::spawn(shortcut.to_string(), tx);
    #[cfg(not(target_os = "linux"))]
    let _ = (shortcut, tx);
}

/// Call on the UI (main) thread before the event loop starts.
pub fn register(desktop: Desktop, shortcut: &str, tx: Sender<Msg>) -> Registration {
    match desktop {
        // GNOME: the extension owns the shortcut when it is loaded (the agent then ignores this
        // one), but the portal binding keeps Voicio usable before the extension is enabled.
        Desktop::Gnome | Desktop::Wayland => {
            #[cfg(target_os = "linux")]
            {
                portal::spawn(shortcut.to_string(), tx);
                Registration::Portal
            }
            #[cfg(not(target_os = "linux"))]
            Registration::None
        }
        Desktop::X11 | Desktop::Windows | Desktop::MacOs => match native(shortcut, tx) {
            Ok(manager) => Registration::Native(manager),
            Err(e) => {
                eprintln!("voicio: could not register {shortcut:?} ({e}); bind `voicio toggle` to a shortcut instead");
                Registration::None
            }
        },
    }
}

fn native(shortcut: &str, tx: Sender<Msg>) -> Result<GlobalHotKeyManager, String> {
    let hotkey: global_hotkey::hotkey::HotKey = shortcut.parse().map_err(|e| format!("{e}"))?;
    let manager = GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
    manager.register(hotkey).map_err(|e| e.to_string())?;
    let id = hotkey.id();
    let tx = std::sync::Mutex::new(tx);
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.id == id {
            let pressed = event.state == HotKeyState::Pressed;
            let _ = tx.lock().unwrap_or_else(|e| e.into_inner()).send(Msg::Hotkey(pressed));
        }
    }));
    Ok(manager)
}

#[cfg(target_os = "linux")]
mod portal {
    use crate::agent::{Cmd, Msg};
    use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
    use ashpd::desktop::CreateSessionOptions;
    use futures_util::StreamExt;
    use std::sync::mpsc::Sender;

    const APP_ID: &str = "io.github.voicio.Voicio";

    use std::sync::atomic::{AtomicBool, Ordering};

    pub static BOUND: AtomicBool = AtomicBool::new(false);
    /// The last request was refused or failed; cleared by the next attempt.
    pub static FAILED: AtomicBool = AtomicBool::new(false);
    /// A request thread is alive (waiting on the prompt, or serving the bound shortcut).
    static RUNNING: AtomicBool = AtomicBool::new(false);
    static REGISTERED: AtomicBool = AtomicBool::new(false);
    pub static FALLBACK: std::sync::Mutex<Option<super::Fallback>> = std::sync::Mutex::new(None);

    /// "Super+Alt+Space" -> the XDG shortcut format "LOGO+ALT+space".
    pub fn trigger(shortcut: &str) -> String {
        shortcut
            .split('+')
            .map(|part| match part.trim().to_ascii_uppercase().as_str() {
                "SUPER" | "META" | "WIN" | "CMD" => "LOGO".to_string(),
                "CONTROL" | "CTRL" => "CTRL".to_string(),
                "ALT" | "OPTION" => "ALT".to_string(),
                "SHIFT" => "SHIFT".to_string(),
                // function keys keep their keysym case ("F9"); other keys are lower case ("space", "d")
                key if key.len() > 1 && key.starts_with('F') && key[1..].chars().all(|c| c.is_ascii_digit()) => key.to_string(),
                _ => part.trim().to_ascii_lowercase(),
            })
            .collect::<Vec<_>>()
            .join("+")
    }

    pub fn spawn(shortcut: String, tx: Sender<Msg>) {
        let trigger = trigger(&shortcut);
        if RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }
        FAILED.store(false, Ordering::Relaxed);
        let tx2 = tx.clone();
        let _ = std::thread::Builder::new().name("shortcut-portal".into()).spawn(move || {
            let result = zbus::block_on(async move {
                // Portals identify non-Flatpak apps by an app id registered up front; it must match
                // the installed io.github.voicio.Voicio.desktop entry.
                if !REGISTERED.load(Ordering::Relaxed) {
                    let app_id = ashpd::AppID::try_from(APP_ID).map_err(|_| ashpd::Error::NoResponse)?;
                    ashpd::register_host_app(app_id).await?;
                    REGISTERED.store(true, Ordering::Relaxed);
                }
                let portal = GlobalShortcuts::new().await?;
                let session = portal.create_session(CreateSessionOptions::default()).await?;
                let shortcut = NewShortcut::new("dictate", "Dictate with Voicio (hold to talk, or tap to start and stop)")
                    .preferred_trigger(Some(trigger.as_str()));
                let bound = portal.bind_shortcuts(&session, &[shortcut], None, BindShortcutsOptions::default()).await?.response()?;
                // The desktop may accept the request yet assign no key (the user then has to pick one in
                // its settings). Count that as "not bound" so the extension binds the key itself.
                let keys: Vec<String> = bound.shortcuts().iter().map(|s| s.trigger_description().to_string()).collect();
                eprintln!("voicio: shortcut portal accepted the request, keys assigned: {keys:?}");
                if keys.iter().all(|k| k.trim().is_empty()) {
                    return Err(ashpd::Error::NoResponse);
                }
                BOUND.store(true, Ordering::Relaxed);
                let _ = tx.send(Msg::Command(Cmd::Recheck));
                let pressed = portal.receive_activated().await?.map(|_| true);
                let released = portal.receive_deactivated().await?.map(|_| false);
                let mut events = futures_util::stream::select(pressed, released);
                while let Some(down) = events.next().await {
                    if tx.send(Msg::Hotkey(down)).is_err() {
                        break;
                    }
                }
                drop(session); // the binding lives exactly as long as this session
                Ok::<(), ashpd::Error>(())
            });
            BOUND.store(false, Ordering::Relaxed);
            if let Err(e) = result {
                FAILED.store(true, Ordering::Relaxed);
                // The GNOME keybinding trick (gsettings) only exists on GNOME. KDE, Sway and friends
                // get a plain pointer to their own keybinding config instead of a gsettings error.
                let on_gnome = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.split(':').any(|p| p.eq_ignore_ascii_case("GNOME")));
                let result = if !on_gnome {
                    super::Fallback::Failed("this desktop has no global-shortcut service Voicio can use".into())
                } else {
                    match crate::gnome_keys::ensure(&shortcut) {
                        Ok(i) => super::Fallback::Active { key: i.key, note: i.note },
                        Err(why) => super::Fallback::Failed(why),
                    }
                };
                *FALLBACK.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
                eprintln!(
                    "voicio: global shortcut portal unavailable ({e}); bind `voicio toggle` to a desktop shortcut instead"
                );
            }
            RUNNING.store(false, Ordering::SeqCst);
            let _ = tx2.send(Msg::Command(Cmd::Recheck));
        });
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn shortcut_maps_to_portal_format() {
            assert_eq!(super::trigger("Super+Alt+Space"), "LOGO+ALT+space");
            assert_eq!(super::trigger("Ctrl+Shift+D"), "CTRL+SHIFT+d");
            assert_eq!(super::trigger("F9"), "F9");
            assert_eq!(super::trigger("Ctrl+f12"), "CTRL+F12");
        }
    }
}
