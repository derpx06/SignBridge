//! Which desktop we run on decides how the hotkey, live text and final text are delivered.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Desktop {
    /// GNOME Shell: the Voicio extension owns hotkey, live preedit and text insertion (D-Bus).
    Gnome,
    /// Other Wayland compositors (KDE Plasma, Hyprland, Sway, ...): XDG-portal shortcut, final text
    /// via the clipboard (Wayland forbids synthetic typing without a compositor-specific protocol).
    Wayland,
    /// X11 sessions: native hotkey, overlay, synthetic typing.
    X11,
    Windows,
    MacOs,
}

/// Whether the app should run on X11 through XWayland.
///
/// It must on most Wayland desktops: the pill needs always-on-top, no focus and an exact position,
/// which native Wayland does not give. On GNOME the Voicio extension draws the pill instead, so the
/// agent can stay on native Wayland, which gives the settings window the real cursor size, sharp
/// fractional scaling and a proper app id. Decided once: the backend cannot change while running.
pub fn wants_xwayland() -> bool {
    static CHOICE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CHOICE.get_or_init(|| {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let display = std::env::var_os("DISPLAY").is_some();
        let gnome = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.split(':').any(|p| p.eq_ignore_ascii_case("GNOME")));
        decide_xwayland(wayland, display, gnome && extension_enabled())
    })
}

/// Pure rule: XWayland only on Wayland with an X display, unless the GNOME extension owns the pill.
fn decide_xwayland(wayland: bool, display: bool, extension_owns_pill: bool) -> bool {
    wayland && display && !extension_owns_pill
}

const EXTENSION_UUID: &str = "voicio@derpx06.github.io";

/// Is the Voicio GNOME extension loaded and running right now? Only then does it draw the pill, so
/// only then can the app skip its own. (Enabled is not enough: a freshly installed extension is
/// unknown to the running shell until the next login, and until then nothing would draw a pill.)
#[cfg(target_os = "linux")]
fn extension_enabled() -> bool {
    std::process::Command::new("gnome-extensions").args(["info", EXTENSION_UUID]).output().ok()
        .filter(|o| o.status.success())
        .is_some_and(|o| info_says_active(&String::from_utf8_lossy(&o.stdout)))
}

#[cfg(not(target_os = "linux"))]
fn extension_enabled() -> bool {
    false
}

/// Pure rule over `gnome-extensions info` output.
#[allow(dead_code)]
fn info_says_active(info: &str) -> bool {
    info.lines().any(|l| l.trim().eq_ignore_ascii_case("state: active"))
}

impl Desktop {
    pub fn detect() -> Desktop {
        if cfg!(windows) {
            Desktop::Windows
        } else if cfg!(target_os = "macos") {
            Desktop::MacOs
        } else if std::env::var("XDG_CURRENT_DESKTOP")
            .is_ok_and(|d| d.split(':').any(|p| p.eq_ignore_ascii_case("GNOME")))
        {
            Desktop::Gnome
        } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            Desktop::Wayland
        } else {
            Desktop::X11
        }
    }

    /// Voicio draws its own tray icon (GNOME has the extension's indicator instead).
    pub fn has_tray(self) -> bool {
        self != Desktop::Gnome
    }

    /// A floating "listening" pill that never takes focus. Native Wayland cannot place one without
    /// layer-shell (winit has none), so Wayland sessions draw it through XWayland where available.
    pub fn has_overlay(self) -> bool {
        match self {
            Desktop::X11 | Desktop::Windows | Desktop::MacOs => true,
            Desktop::Wayland => std::env::var_os("DISPLAY").is_some(),
            Desktop::Gnome => false, // the extension draws it; see Frontend::pill for the fallback
        }
    }

    /// Final text is typed as keystrokes; otherwise it goes to the clipboard.
    pub fn types_text(self) -> bool {
        matches!(self, Desktop::X11 | Desktop::Windows | Desktop::MacOs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnome_with_the_extension_stays_on_native_wayland() {
        assert!(!decide_xwayland(true, true, true));
        assert!(decide_xwayland(true, true, false)); // KDE, Sway, or GNOME without the extension
        assert!(!decide_xwayland(true, false, false)); // pure Wayland
        assert!(!decide_xwayland(false, true, false)); // an X11 session is simply X11
    }

    #[test]
    fn only_a_running_extension_takes_over_the_pill() {
        assert!(info_says_active("voicio@derpx06.github.io\n  Name: Voicio\n  Enabled: Yes\n  State: ACTIVE\n"));
        assert!(!info_says_active("  Enabled: Yes\n  State: INITIALIZED\n")); // installed, not loaded until the next login
        assert!(!info_says_active("  Enabled: Yes\n  State: ERROR\n"));
        assert!(!info_says_active(""));
    }
}
