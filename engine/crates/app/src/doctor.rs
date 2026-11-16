//! `voicio doctor`: a plain-text health report that works without the agent running. It names
//! what works, what does not and why, and contains no audio and no dictated text, so it is safe
//! to paste into a bug report.

use crate::config::Config;
use crate::desktop::Desktop;
use cpal::traits::{DeviceTrait, HostTrait};
use std::path::Path;
#[cfg(target_os = "linux")]
use std::process::Command;

pub fn run() {
    let config = Config::load();
    let desktop = Desktop::detect();
    println!("Voicio {} doctor", env!("CARGO_PKG_VERSION"));
    println!("system:   {} {}", std::env::consts::OS, std::env::consts::ARCH);
    println!("desktop:  {desktop:?}");
    #[cfg(target_os = "linux")]
    linux_session();
    println!("agent:    {}", if crate::instance::send("status").is_ok() { "running" } else { "not running (start it with `voicio`)" });
    microphone();
    models(&config);
    println!("shortcut: {}", config.shortcut);
    #[cfg(target_os = "linux")]
    typing(desktop);
    println!("overlay:  {}", overlay_status(desktop));
}

/// Says which pill will actually show, and why there may be none (dictation works either way).
fn overlay_status(desktop: Desktop) -> &'static str {
    let xwayland = crate::desktop::wants_xwayland();
    match desktop {
        Desktop::Gnome if xwayland => "GNOME extension pill when it is active (see above), otherwise a floating pill through XWayland",
        Desktop::Gnome => "GNOME extension pill (the app runs on native Wayland: sharp window, real cursor); if the extension is not loaded there is no pill, log out and in once",
        Desktop::Wayland if xwayland => "floating pill through XWayland (native Wayland cannot keep a window on top without focus)",
        Desktop::Wayland => "none: Wayland without XWayland has no focus-free overlay; dictation still works",
        _ => "floating pill",
    }
}

#[cfg(target_os = "linux")]
fn linux_session() {
    let var = |name: &str| std::env::var(name).unwrap_or_else(|_| "-".into());
    println!(
        "session:  type={} wayland={} x11={} desktop-env={}",
        var("XDG_SESSION_TYPE"),
        var("WAYLAND_DISPLAY"),
        var("DISPLAY"),
        var("XDG_CURRENT_DESKTOP")
    );
}

fn microphone() {
    let host = cpal::default_host();
    match host.default_input_device() {
        None => println!("mic:      NO INPUT DEVICE FOUND. Plug in or enable a microphone."),
        Some(device) => {
            let name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "unknown".into());
            match device.default_input_config() {
                Ok(c) => println!("mic:      {name} ({} Hz, {} ch, {:?})", c.sample_rate(), c.channels(), c.sample_format()),
                Err(e) => println!("mic:      {name}: cannot read its format ({e})"),
            }
        }
    }
}

fn models(config: &Config) {
    let dir = config.models_dir();
    println!("models:   {}", dir.display());
    for (label, path) in [
        ("voice detection", "silero_vad.onnx"),
        ("speech (Parakeet 110M)", "sherpa-onnx-nemo-parakeet_tdt_transducer_110m-en-36000-int8"),
        ("cleanup (S1-mini)", "s1-mini/s1-mini-q4_k_m.gguf"),
    ] {
        let full = dir.join(path);
        let state = match size_mb(&full) {
            Some(mb) => format!("ok, {mb:.0} MB"),
            None => "MISSING".into(),
        };
        println!("          {label:<24} {state}");
    }
    let worker = config
        .worker_path
        .clone()
        .or_else(|| std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join(worker_name()))));
    match worker {
        Some(p) if p.is_file() => println!("worker:   {}", p.display()),
        Some(p) => println!("worker:   MISSING ({})", p.display()),
        None => println!("worker:   unknown"),
    }
}

fn worker_name() -> &'static str {
    if cfg!(windows) { "voicio-engine.exe" } else { "voicio-engine" }
}

/// Size of a file, or the total size of a directory tree.
fn size_mb(path: &Path) -> Option<f64> {
    fn bytes(p: &Path) -> u64 {
        match std::fs::metadata(p) {
            Ok(m) if m.is_dir() => std::fs::read_dir(p).map(|d| d.flatten().map(|e| bytes(&e.path())).sum()).unwrap_or(0),
            Ok(m) => m.len(),
            Err(_) => 0,
        }
    }
    path.exists().then(|| bytes(path) as f64 / 1_048_576.0)
}

#[cfg(target_os = "linux")]
fn typing(desktop: Desktop) {
    let have = |tool: &str| Command::new("sh").args(["-c", &format!("command -v {tool}")]).output().is_ok_and(|o| o.status.success());
    println!("typing:");
    match desktop {
        Desktop::Gnome => {
            let ext = Command::new("gnome-extensions").args(["info", "voicio@derpx06.github.io"]).output();
            let state = ext
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .and_then(|t| t.lines().find_map(|l| l.trim().strip_prefix("State:").map(|s| s.trim().to_string())))
                .unwrap_or_else(|| "not installed".into());
            println!("          GNOME extension       {state}{}", if state == "ACTIVE" { "" } else { "  <- needed to type; log out and in after installing" });
        }
        Desktop::Wayland => println!("          wtype                 {}", if have("wtype") { "found" } else { "not found (Sway/Hyprland: install wtype; KDE types through the portal prompt)" }),
        _ => {}
    }
    println!("          xdotool / XWayland    {}", if std::env::var_os("DISPLAY").is_some() { "display present" } else { "no X display" });
    println!("          clipboard (wl-copy)   {}", if have("wl-copy") || have("xclip") { "found" } else { "not found" });
}
