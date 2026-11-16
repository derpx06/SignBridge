//! Routes dictation output to the right place for this desktop:
//! - GNOME: D-Bus to the extension (live preedit in the focused field, then commit).
//! - Windows / macOS / X11: floating overlay with the live text, then typed keystrokes.
//! - Other Wayland: tray state while listening, then the clipboard + a notification.
//! The tray icon (where present) mirrors the state everywhere but GNOME.

use crate::agent::Msg;
use crate::config::Config;
use crate::desktop::Desktop;
use crate::ui;
use std::sync::mpsc::Sender;

pub struct Frontend {
    desktop: Desktop,
    #[cfg(target_os = "linux")]
    bus: Option<crate::dbus::Bus>,
}

impl Frontend {
    pub fn new(desktop: Desktop, tx: Sender<Msg>) -> Result<Frontend, String> {
        #[cfg(target_os = "linux")]
        {
            // The D-Bus API is the GNOME extension's channel; elsewhere it is a bonus, not a must.
            let bus = match crate::dbus::Bus::connect(tx) {
                Ok(bus) => Some(bus),
                Err(e) if desktop == Desktop::Gnome => return Err(e),
                Err(e) => {
                    eprintln!("voicio: {e}");
                    None
                }
            };
            Ok(Frontend { desktop, bus })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = tx;
            Ok(Frontend { desktop })
        }
    }

    pub fn preview(&self, text: &str) {
        #[cfg(target_os = "linux")]
        if let Some(bus) = &self.bus {
            bus.preview(text);
        }
        if self.pill() {
            ui::overlay_text(text.to_string()); // words only: a late partial must not change the pill's state
        }
    }

    pub fn done(&self, text: &str) {
        if !text.is_empty() {
            ui::add_history(text.to_string());
        }
        #[cfg(target_os = "linux")]
        if let Some(bus) = &self.bus {
            bus.done(text);
        }
        if text.is_empty() {
            return;
        }
        ui::hide_overlay(); // first, so the pill is already leaving before the keystrokes land
        // Wayland without the extension cannot type through a toolkit: ask the desktop's
        // RemoteDesktop portal (first use shows its permission prompt), and never lose the text.
        #[cfg(target_os = "linux")]
        if self.portal_typing() {
            let text = text.to_string();
            crate::typing::type_async(text.clone(), move |result| {
                if let Err(e) = result {
                    eprintln!("voicio: could not type ({e}); the text is on the clipboard");
                    ui::deliver(text, false);
                }
            });
            return;
        }
        if self.desktop != Desktop::Gnome {
            ui::deliver(text.to_string(), self.desktop.types_text());
        }
    }

    /// Wayland sessions where nothing but the portal can type: GNOME until its extension has
    /// attached (the extension types itself), and other Wayland compositors.
    #[cfg(target_os = "linux")]
    fn portal_typing(&self) -> bool {
        match self.desktop {
            Desktop::Gnome => !self.attached(),
            Desktop::Wayland => true,
            _ => false,
        }
    }

    /// The on-screen pill: always where the desktop has an overlay, and on GNOME until the extension
    /// (which draws its own pill) has loaded.
    fn pill(&self) -> bool {
        #[cfg(target_os = "linux")]
        if self.desktop == Desktop::Gnome {
            // The extension's pill only when it draws one; otherwise ours, wherever XWayland exists.
            return !self.extension_draws_pill() && crate::desktop::wants_xwayland();
        }
        self.desktop.has_overlay()
    }

    #[cfg(target_os = "linux")]
    fn attached(&self) -> bool {
        self.bus.as_ref().is_some_and(|b| b.extension_attached())
    }

    /// A connected extension that draws the pill itself (older ones do; newer ones leave it to us).
    #[cfg(target_os = "linux")]
    fn extension_draws_pill(&self) -> bool {
        self.bus.as_ref().and_then(|b| b.extension()).is_some_and(|e| e.draws_pill)
    }

    /// A connected extension that binds the shortcut itself (older ones do).
    pub fn extension_owns_shortcut(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.bus.as_ref().and_then(|b| b.extension()).is_some_and(|e| e.owns_shortcut)
        }
        #[cfg(not(target_os = "linux"))]
        false
    }

    /// The GNOME extension is loaded and owns the shortcut and the typing.
    pub fn extension_attached(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.desktop == Desktop::Gnome && self.attached()
        }
        #[cfg(not(target_os = "linux"))]
        false
    }

    pub fn state(&self, state: &'static str) {
        ui::set_state(state);
        #[cfg(target_os = "linux")]
        match state {
            "listening" if crate::config::Config::load().pause_media => crate::media::pause_playing(),
            "idle" => crate::media::resume_paused(),
            _ => {}
        }
        #[cfg(target_os = "linux")]
        if let Some(bus) = &self.bus {
            bus.state(state);
        }
        if self.pill() {
            match state {
                "listening" | "finishing" => ui::overlay(state, None),
                _ => ui::hide_overlay(),
            }
        }
    }

    /// Mic loudness 0..1 for the pill's waveform.
    pub fn level(&self, level: f64) {
        #[cfg(target_os = "linux")]
        if let Some(bus) = &self.bus {
            bus.level(level);
        }
        #[cfg(not(target_os = "linux"))]
        let _ = level;
    }

    /// Raw microphone RMS for the pill (it levels itself; the 0..1 `level` is for the bars).
    pub fn meter(&self, bands: crate::pill::model::Bands) {
        if self.pill() {
            ui::meter(bands);
        }
    }

    /// A message with its own look in the pill (and, on GNOME, the extension's pill via the error signal).
    pub fn notice(&self, kind: crate::pill::model::NoticeKind, text: &str) {
        if self.pill() {
            ui::notice(kind, text.to_string());
        }
        #[cfg(target_os = "linux")]
        if let Some(bus) = &self.bus {
            bus.error(text);
        }
    }

    /// The speech models are loading (the pill fills its lattice in) or ready.
    pub fn warming(&self, on: bool) {
        if self.pill() {
            ui::warming(on);
        }
    }

    /// One problem, told twice: `short` for the pill (a few words), `long` for the log and the
    /// GNOME extension (what happened and what to do).
    pub fn problem(&self, kind: crate::pill::model::NoticeKind, short: &str, long: &str) {
        eprintln!("voicio: {long}");
        if self.pill() {
            ui::notice(kind, short.to_string());
        }
        #[cfg(target_os = "linux")]
        if let Some(bus) = &self.bus {
            bus.error(long);
        }
    }

    pub fn error(&self, message: &str) {
        eprintln!("voicio: {message}");
        if self.pill() {
            ui::overlay_message(message.to_string());
        }
        #[cfg(target_os = "linux")]
        if let Some(bus) = &self.bus {
            bus.error(message);
        }
    }

    /// Checks what Voicio needs and shows the first problem (with its fix) on the Home page.
    pub fn health(&self, config: &Config) {
        use cpal::traits::HostTrait;
        let facts = crate::health::Facts {
            desktop: self.desktop,
            has_mic: cpal::default_host().default_input_device().is_some(),
            models_ready: voicio_models::can_dictate(&config.models_dir(), config.tier()),
            extension_attached: self.extension_attached(),
            #[cfg(target_os = "linux")]
            portal_typing_allowed: crate::typing::allowed(),
            #[cfg(not(target_os = "linux"))]
            portal_typing_allowed: true,
            xwayland: crate::desktop::wants_xwayland(),
            shortcut_denied: crate::hotkey::portal_failed(),
            fallback: crate::hotkey::fallback(),
            shortcut_ready: crate::hotkey::portal_bound() || self.extension_owns_shortcut() || !matches!(self.desktop, Desktop::Gnome | Desktop::Wayland),
        };
        let issues = crate::health::check(&facts);
        ui::set_health(issues.first().map(|i| (i.problem.clone(), i.fix.clone(), i.action.map(|a| a.label()).unwrap_or("").to_string())));
    }

    pub fn models(&self, line: &str) {
        ui::set_models(line.to_string());
    }

    /// The key and mode that work right now, or None while nothing is bound (yet).
    pub fn bound_shortcut(&self, config: &Config) -> Option<(String, String)> {
        match crate::hotkey::fallback() {
            // The GNOME keybinding fallback runs `voicio toggle`: no key release, so tap only.
            Some(crate::hotkey::Fallback::Active { key, .. }) => Some((key, "toggle".into())),
            _ if crate::hotkey::portal_bound() || self.extension_owns_shortcut() || !matches!(self.desktop, Desktop::Gnome | Desktop::Wayland) => {
                Some((config.shortcut.clone(), config.shortcut_mode.clone()))
            }
            _ => None,
        }
    }

    fn shortcut_hint(&self, config: &Config) -> String {
        let how = match config.shortcut_mode.as_str() {
            "toggle" => "press to start, press again to stop",
            "hold-or-tap" => "hold while speaking, or tap to start and stop",
            _ => "hold while speaking, let go to type",
        };
        format!("Shortcut: {} ({how}). Any desktop shortcut can also run `voicio toggle`.", config.shortcut)
    }

    /// Opens the app window on the Models page (first run, or a model is missing).
    pub fn open_models(&self, config: &Config) {
        ui::open_window(config.clone(), self.shortcut_hint(config), ui::Page::Models);
    }

    /// Opens the first-run setup (welcome, microphone, shortcut, pack, download, options).
    pub fn open_onboarding(&self, config: &Config) {
        ui::open_window(config.clone(), self.shortcut_hint(config), ui::Page::Onboarding);
    }

    pub fn open_settings(&self, config: &Config) {
        ui::open_window(config.clone(), self.shortcut_hint(config), ui::Page::Home);
    }
}
