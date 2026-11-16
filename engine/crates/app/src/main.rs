//! `voicio`: the always-running desktop agent (tiny; models live in a separate worker process).
//!
//!   voicio                         run the agent (one per user)
//!   voicio toggle|start|stop|cancel|status|settings|quit
//!                                  control the running agent (bind these to any desktop shortcut)
//!   voicio models [install [lite|standard|quality|MODEL-ID]|verify]
//!                                  list, download (resumable, checksum-verified) and check models
//!   voicio cloud-check [--provider ID] [--clip FILE.wav] [--region R]
//!                                  send the bench clip to every cloud provider with a stored key
//!   voicio cloud-key set ID        store a provider's API key, read from stdin (not echoed)
//!   voicio doctor                  health report (mic, models, typing, session); safe to paste
//!   voicio run --input-wav FILE    agent that "hears" FILE instead of the mic (headless tests)

mod agent;
mod audio;
mod autostart;
mod cloud;
mod cloud_check;
mod config;
#[cfg(target_os = "linux")]
mod dbus;
mod desktop;
mod durable;
mod doctor;
mod frontend;
#[cfg(target_os = "linux")]
mod gnome_keys;
mod history;
mod health;
mod hotkey;
mod inject;
mod instance;
#[cfg(target_os = "linux")]
mod media;
mod memguard;
mod memory;
mod miclevel;
mod models_cmd;
mod pill;
mod platform;
mod policy;
mod replacements;
mod trigger;
#[cfg(target_os = "linux")]
mod typing;
mod ui;
mod worker;

use std::path::PathBuf;
use std::sync::mpsc;

const COMMANDS: &[&str] = &["toggle", "start", "stop", "cancel", "status", "settings", "quit"];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some(command) if COMMANDS.contains(&command) => control(command),
        Some("doctor") => doctor::run(),
        Some("ui-snapshot") => {
            let page = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(0);
            let out = args.get(2).cloned().unwrap_or_else(|| "voicio-ui.ppm".into());
            if let Err(e) = ui::snapshot(page, out.into(), false) {
                fail(&e);
            }
        }
        Some("overlay-snapshot") => {
            let out = args.get(1).cloned().unwrap_or_else(|| "voicio-overlay.ppm".into());
            let words = args.get(2).cloned().unwrap_or_default();
            if let Err(e) = ui::overlay_snapshot(out.into(), words, args.get(3).cloned().unwrap_or_default()) {
                fail(&e);
            }
        }
        Some("pill-snapshot") => {
            let out = args.get(1).cloned().unwrap_or_else(|| "voicio-pill.ppm".into());
            if let Err(e) = pill::snapshot(std::path::Path::new(&out)) {
                fail(&e.to_string());
            }
        }
        Some("models") => models_cmd::run(&args[1..]),
        Some("cloud-check") => cloud_check::run(&args[1..]),
        Some("cloud-key") => cloud_check::key(&args[1..]),
        None | Some("run") => run(&args),
        Some("-h" | "--help") => {
            println!("usage: voicio [run [--input-wav FILE]] | voicio {}", COMMANDS.join("|"));
        }
        Some(other) => fail(&format!("unknown command {other:?} (see --help)")),
    }
}

fn control(command: &str) {
    match instance::send(command) {
        Ok(reply) if reply.starts_with("error") => fail(&reply),
        Ok(reply) => println!("{reply}"),
        Err(_) => fail("Voicio is not running (start it with `voicio`)"),
    }
}

fn run(args: &[String]) {
    voicio_ipc::heap::pin_mmap_threshold();
    let mut options = agent::Options { input_wav: None };
    let mut it = args.iter().skip_while(|a| *a == "run");
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--input-wav" => options.input_wav = it.next().map(PathBuf::from),
            other => fail(&format!("unknown flag {other:?}")),
        }
    }
    let _lock = match instance::acquire() {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            // Opening Voicio again (app grid, Start menu) brings up the app window.
            let _ = instance::send("settings");
            return;
        }
        Err(e) => fail(&format!("cannot create the instance lock: {e}")),
    };
    let config = config::Config::load();
    let (tx, rx) = mpsc::channel();
    if let Err(e) = instance::serve(tx.clone()) {
        fail(&format!("cannot open the control socket: {e}"));
    }
    let desktop = desktop::Desktop::detect();
    let shortcut = config.shortcut.clone();
    let frontend = frontend::Frontend::new(desktop, tx.clone()).unwrap_or_else(|e| fail(&e));
    eprintln!(
        "voicio: ready (models load on first use, unload after {} min idle)",
        config.idle_unload_minutes
    );
    // A long-lived control thread spawns the worker: Linux PR_SET_PDEATHSIG follows the spawning
    // thread, so it must not be a short-lived one.
    let agent_tx = tx.clone();
    let control = std::thread::Builder::new()
        .name("control".into())
        .spawn(move || {
            agent::Agent::new(config, options, agent_tx, frontend).run(rx);
            ui::quit(); // agent finished (Quit): end the UI loop too
        })
        .expect("spawn control thread");
    // The UI loop must own the main thread (macOS requirement). Without a display, run headless.
    if let Err(e) = ui::run(tx, desktop, &shortcut) {
        eprintln!("voicio: no UI available ({e}); running headless");
    }
    let _ = control.join();
}

pub fn fail(message: &str) -> ! {
    eprintln!("voicio: {message}");
    std::process::exit(1)
}
