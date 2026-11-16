//! User settings in `config.toml` (Linux: ~/.config/voicio, macOS: ~/Library/Application
//! Support/io.github.voicio.voicio, Windows: %APPDATA%\voicio\voicio\config).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Unload the speech models after this many minutes without dictation.
    pub idle_unload_minutes: f64,
    /// Show the recognised words in the pill while speaking; off: the text appears after release.
    pub live_text: bool,
    /// Suppress steady background noise (fans, hum, hiss) before recognition.
    pub noise_reduction: bool,
    /// CPU use of the speech worker: "balanced" (default), "low" (fewest threads, slower partials) or "fast".
    pub cpu_mode: String,
    /// Polish the finished dictation with the cleanup model (fillers, punctuation, numbers).
    pub cleanup: bool,
    /// How cleanup writes: "casual", "semi-casual", "semi-formal" (default) or "formal". The cleanup model
    /// was trained on exactly these, so nothing else is accepted. A spoken "make it formal" overrides it once.
    pub cleanup_styling: String,
    /// Obey spoken formatting instructions ("as a list", "make it formal"). Off: they are typed as words.
    pub voice_commands: bool,
    /// "auto" (default: prose, but a spoken list or email is laid out as one), "prose" (never) or "lists" (always whole-text).
    pub cleanup_structure: String,
    /// "general" (default) or "email" (greeting, body and sign-off laid out as an email).
    pub cleanup_context: String,
    /// Treat the system as under memory pressure below this much available RAM.
    pub low_memory_mb: u64,
    /// Global shortcut on desktops where Voicio registers it itself (not GNOME: the extension does).
    pub shortcut: String,
    /// How the shortcut behaves: "hold" (listen only while it is held), "toggle" or "hold-or-tap".
    pub shortcut_mode: String,
    /// "When I say X, write Y": names, jargon, shortcuts. Applied to every finished dictation.
    pub replacements: Vec<crate::replacements::TextReplacement>,
    /// Days to keep dictated text on disk for the History page; 0 keeps it in memory only.
    pub history_days: u32,
    /// Ids of one-time upgrades already applied to this file (see `migrate`).
    pub migrations: Vec<String>,
    /// Model tier: "auto" (pick from this computer's memory and cores), "lite", "standard" or "quality".
    pub tier: String,
    /// The first-run setup has been completed (or skipped); false opens it again on the next start.
    pub onboarded: bool,
    /// Spoken language: "auto" or a code such as "en", "hi". Only the English models run today; the other
    /// packs of the Model library are stored and chosen here, and the engine will use them when it supports them.
    pub language: String,
    /// Id of the optional language pack chosen in the Model library ("" = the tier's English model).
    pub speech_pack: String,
    /// Optional cloud transcription provider id ("groq", "deepgram", ...; see providers/cloud-stt.json).
    /// Only the id is kept here; the API key lives in its own owner-only file (see `cloud.rs`).
    pub cloud_provider: String,
    /// The model of the provider in use ("" = its default); kept equal to `cloud_models[cloud_provider]`.
    pub cloud_model: String,
    /// The model chosen for each provider (provider id -> model id); a provider not listed uses its default.
    pub cloud_models: std::collections::BTreeMap<String, String>,
    /// When the cloud is used: "off" (default), "always" (every dictation; the on-device text is the
    /// fallback) or "fallback" (only when no on-device speech model is installed).
    pub cloud_mode: String,
    /// Azure only: the Speech resource's region ("westeurope") or its full host.
    pub cloud_region: String,
    /// Set only after the user accepted that provider's disclosure; it must equal `cloud_provider`.
    pub cloud_consented: String,
    /// Providers whose "Live" toggle is on (words stream in while speaking); off by default. Only ids whose
    /// provider has a live API (`cloud::Provider::live_supported`) take effect.
    pub cloud_live: Vec<String>,
    pub models_dir: Option<PathBuf>,
    /// Path to `voicio-engine`; default is next to this executable, then `PATH`.
    pub worker_path: Option<PathBuf>,
    /// Input device by name ("" = the system default).
    pub input_device: String,
    /// Pause media players (MPRIS) while dictating and resume them after. Opt-in.
    pub pause_media: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            idle_unload_minutes: 5.0,
            live_text: true,
            noise_reduction: true,
            cpu_mode: "balanced".into(),
            cleanup: true,
            cleanup_styling: "semi-formal".into(),
            voice_commands: true,
            cleanup_structure: "auto".into(),
            cleanup_context: "general".into(),
            low_memory_mb: 1024,
            // Super+Space switches input sources on GNOME/KDE; Cmd+Space is Spotlight; Win+Space
            // switches keyboard layouts.
            // One key that is easy to hold. A plain F-key is not taken by GNOME, KDE, Windows or macOS
            // (Fn on some laptops); Super+Space and Alt+Space switch input sources or open window menus.
            shortcut: "F9".into(),
            shortcut_mode: "hold".into(),
            replacements: Vec::new(),
            history_days: 30,
            migrations: Vec::new(),
            tier: "auto".into(),
            onboarded: false,
            language: "auto".into(),
            speech_pack: String::new(),
            cloud_provider: String::new(),
            cloud_model: String::new(),
            cloud_models: Default::default(),
            cloud_mode: "off".into(),
            cloud_region: String::new(),
            cloud_consented: String::new(),
            cloud_live: Vec::new(),
            models_dir: None,
            worker_path: None,
            input_device: String::new(),
            pause_media: false,
        }
    }
}

impl Config {
    /// One-time upgrades of a saved file, each recorded by id so it never runs twice and never
    /// overrides a choice the user made afterwards. Returns whether anything changed.
    fn migrate(&mut self) -> bool {
        let mut changed = false;
        if !self.migrations.iter().any(|m| m == "shortcut-f9") {
            // the old default was awkward to hold: move people who never changed it
            if self.shortcut == "Super+Alt+Space" || self.shortcut == "Ctrl+Alt+Space" {
                self.shortcut = "F9".into();
            }
            self.migrations.push("shortcut-f9".into());
            changed = true;
        }
        if !self.migrations.iter().any(|m| m == "onboarded") {
            // an install that already dictates has been through setup in everything but name
            if voicio_models::can_dictate(&self.models_dir(), self.tier()) {
                self.onboarded = true;
            }
            self.migrations.push("onboarded".into());
            changed = true;
        }
        if !self.migrations.iter().any(|m| m == "cloud-mode") {
            // before cloud_mode existed, a chosen and consented provider meant "use it for every dictation"
            if !self.cloud_provider.is_empty() && self.cloud_consented == self.cloud_provider {
                self.cloud_mode = "always".into();
            }
            self.migrations.push("cloud-mode".into());
            changed = true;
        }
        if !self.migrations.iter().any(|m| m == "cloud-models") {
            // one model for the chosen provider became one model per provider
            if !self.cloud_provider.is_empty() && !self.cloud_model.is_empty() {
                self.cloud_models.entry(self.cloud_provider.clone()).or_insert_with(|| self.cloud_model.clone());
            }
            self.migrations.push("cloud-models".into());
            changed = true;
        }
        if !self.migrations.iter().any(|m| m == "smart-layout") {
            // "prose" used to be the default and means "never lay out lists or emails"; "auto" does it when you dictate one
            if self.cleanup_structure == "prose" {
                self.cleanup_structure = "auto".into();
            }
            self.migrations.push("smart-layout".into());
            changed = true;
        }
        changed
    }

    /// The model chosen for provider `id` ("" = its default).
    pub fn cloud_model_for(&self, id: &str) -> &str {
        self.cloud_models.get(id).map_or("", String::as_str)
    }

    /// The cloud provider in use: switched on, chosen AND consented to. None = fully local (the default).
    pub fn cloud_active(&self) -> Option<&str> {
        let on = matches!(self.cloud_mode.as_str(), "always" | "fallback");
        (on && !self.cloud_provider.is_empty() && self.cloud_consented == self.cloud_provider).then_some(self.cloud_provider.as_str())
    }

    /// The active provider streams live: its Live toggle is on and it has a live API.
    pub fn cloud_live_on(&self) -> bool {
        self.cloud_active().is_some_and(|id| self.cloud_live.iter().any(|l| l == id) && crate::cloud::provider(id).is_some_and(|p| p.live_supported()))
    }

    pub fn path() -> Option<PathBuf> {
        directories::ProjectDirs::from("io.github", "voicio", "voicio").map(|d| d.config_dir().join("config.toml"))
    }

    /// A missing file means defaults; a broken one is reported and ignored rather than fatal.
    pub fn load() -> Config {
        let Some(path) = Self::path() else { return Config::default() };
        migrate_from_whispr(&path);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let mut config: Config = toml::from_str(&text).unwrap_or_else(|e| {
                    // Keep the unreadable file aside: the next save must not destroy what the user wrote.
                    let kept = crate::durable::set_aside(&path);
                    eprintln!("voicio: invalid {} ({e}); using defaults{}", path.display(), kept.map(|k| format!(", the old file is kept as {}", k.display())).unwrap_or_default());
                    Config::default()
                });
                if config.migrate() {
                    if let Err(e) = config.save() {
                        eprintln!("voicio: could not save the upgraded settings: {e}");
                    }
                }
                config
            }
            Err(_) => Config::default(),
        }
    }

    /// Atomic write: a crash mid-save never leaves a half-written config.
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path().ok_or_else(|| std::io::Error::other("no config directory"))?;
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        crate::durable::write(&path, text.as_bytes())
    }

    /// The tier to run: the chosen one, or the recommendation for this computer.
    pub fn tier(&self) -> voicio_models::Tier {
        voicio_models::Tier::parse(&self.tier).unwrap_or_else(crate::models_cmd::recommended)
    }

    pub fn models_dir(&self) -> PathBuf {
        std::env::var_os("VOICIO_MODELS").map(PathBuf::from).or_else(|| self.models_dir.clone()).unwrap_or_else(|| {
            directories::BaseDirs::new()
                .map(|d| default_models_dir(&d.home_dir().join(".cache")))
                .unwrap_or_else(|| PathBuf::from("voicio-models"))
        })
    }
}

/// `<cache>/voicio`, moving the pre-rename `<cache>/whispr-voice-lab` there once. If the move fails
/// (another filesystem, permissions) the old folder keeps being used so nothing is downloaded twice.
fn default_models_dir(cache: &std::path::Path) -> PathBuf {
    let new = cache.join("voicio");
    let old = cache.join("whispr-voice-lab");
    if new.exists() || !old.is_dir() || std::fs::rename(&old, &new).is_ok() {
        new
    } else {
        old
    }
}

/// The app was called Whispr before it became Voicio: carry the settings over once.
fn migrate_from_whispr(new_path: &std::path::Path) {
    if new_path.exists() {
        return;
    }
    let old = directories::ProjectDirs::from("io.github", "whispr", "whispr").map(|d| d.config_dir().join("config.toml"));
    if let (Some(old), Some(dir)) = (old, new_path.parent()) {
        if old.is_file() && std::fs::create_dir_all(dir).is_ok() {
            let _ = std::fs::copy(old, new_path);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_old_models_folder_moves_once_and_a_new_one_is_never_overwritten() {
        let cache = std::env::temp_dir().join(format!("voicio-models-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cache);
        std::fs::create_dir_all(cache.join("whispr-voice-lab")).unwrap();
        std::fs::write(cache.join("whispr-voice-lab/m.onnx"), b"x").unwrap();
        assert_eq!(super::default_models_dir(&cache), cache.join("voicio"));
        assert!(cache.join("voicio/m.onnx").is_file() && !cache.join("whispr-voice-lab").exists());
        std::fs::create_dir_all(cache.join("whispr-voice-lab")).unwrap(); // both exist: keep the new one untouched
        assert_eq!(super::default_models_dir(&cache), cache.join("voicio"));
        assert!(cache.join("whispr-voice-lab").is_dir() && cache.join("voicio/m.onnx").is_file());
        let fresh = cache.join("fresh"); // neither exists: the new path, nothing created
        assert_eq!(super::default_models_dir(&fresh), fresh.join("voicio"));
        std::fs::remove_dir_all(&cache).unwrap();
    }

    #[test]
    fn the_old_default_shortcut_moves_to_f9_once_and_a_custom_one_is_kept() {
        let mut old = super::Config { shortcut: "Super+Alt+Space".into(), migrations: vec![], ..Default::default() };
        assert!(old.migrate());
        assert_eq!(old.shortcut, "F9");
        assert!(!old.migrate(), "it runs once");
        old.shortcut = "Super+Alt+Space".into(); // a deliberate later choice is not overridden
        assert!(!old.migrate());
        assert_eq!(old.shortcut, "Super+Alt+Space");
        let mut custom = super::Config { shortcut: "Ctrl+Shift+D".into(), migrations: vec![], ..Default::default() };
        custom.migrate();
        assert_eq!(custom.shortcut, "Ctrl+Shift+D");
    }

    use super::*;

    #[test]
    fn the_single_cloud_model_moves_to_the_per_provider_map_once() {
        let mut c = super::Config { cloud_provider: "groq".into(), cloud_model: "whisper-large-v3".into(), migrations: vec![], ..Default::default() };
        c.migrate();
        assert_eq!(c.cloud_model_for("groq"), "whisper-large-v3");
        assert_eq!(c.cloud_model_for("openai"), "");
    }

    #[test]
    fn cloud_is_off_unless_chosen_and_consented() {
        let mut c = Config::default();
        assert_eq!(c.cloud_active(), None);
        c.cloud_mode = "always".into();
        c.cloud_provider = "groq".into();
        assert_eq!(c.cloud_active(), None, "chosen but not consented");
        c.cloud_consented = "deepgram".into();
        assert_eq!(c.cloud_active(), None, "consent is per provider");
        c.cloud_consented = "groq".into();
        assert_eq!(c.cloud_active(), Some("groq"));
        c.cloud_mode = "off".into();
        assert_eq!(c.cloud_active(), None, "mode off wins");
    }

    #[test]
    fn an_old_enabled_cloud_provider_becomes_mode_always_once() {
        let mut old: Config = toml::from_str("cloud_provider = \"groq\"\ncloud_consented = \"groq\"\nmigrations = [\"shortcut-f9\", \"onboarded\"]").unwrap();
        assert_eq!(old.cloud_mode, "off");
        assert!(old.migrate());
        assert_eq!((old.cloud_mode.as_str(), old.cloud_active()), ("always", Some("groq")));
        old.cloud_mode = "off".into();
        assert!(!old.migrate(), "a later choice is kept");
        let mut unconsented: Config = toml::from_str("cloud_provider = \"groq\"\nmigrations = [\"shortcut-f9\", \"onboarded\"]").unwrap();
        unconsented.migrate();
        assert_eq!(unconsented.cloud_mode, "off");
    }

    #[test]
    fn partial_config_keeps_defaults_for_missing_keys() {
        let config: Config = toml::from_str("idle_unload_minutes = 1.5\ncleanup = false").unwrap();
        assert_eq!(config.idle_unload_minutes, 1.5);
        assert!(!config.cleanup);
        assert_eq!(config.low_memory_mb, Config::default().low_memory_mb);
    }
}
