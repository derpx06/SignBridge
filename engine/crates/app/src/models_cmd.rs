//! `voicio models`: list, install and verify models from the terminal. The app window's Models
//! page drives the same `voicio_models` functions.

use crate::config::Config;
use std::io::Write;
use voicio_models::{self as models, Cancel, Status, Tier};

pub fn models_dir() -> std::path::PathBuf {
    Config::load().models_dir() // honours VOICIO_MODELS
}

/// This machine's recommended tier.
pub fn recommended() -> Tier {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    models::recommended_tier(system.total_memory() as f64 / 1_048_576.0, cores)
}

pub fn run(args: &[String]) {
    let dir = models_dir();
    match args.first().map(String::as_str) {
        None | Some("list") => list(&dir),
        Some("install") => install(&dir, args.get(1).map(String::as_str)),
        Some("verify") => verify(&dir),
        Some(other) => crate::fail(&format!("unknown models command {other:?} (list, install, verify)")),
    }
}

fn list(dir: &std::path::Path) {
    let recommended = recommended();
    println!("models folder: {}", dir.display());
    println!("recommended for this computer: {} ({})\n", recommended.name(), models::format_bytes(recommended.download_bytes()));
    for tier in Tier::ALL {
        let state = if models::tier_installed(dir, tier) { "installed" } else { "not installed" };
        println!(
            "{:<9} {:<14} needs ~{} MB memory, {} to download. {}",
            tier.name(),
            state,
            tier.ram_mb(),
            models::format_bytes(tier.download_bytes()),
            tier.summary()
        );
    }
    println!();
    for model in models::CATALOG {
        let status = match models::status(dir, model) {
            Status::Installed => "installed",
            Status::Missing => "missing",
            Status::Incomplete => "incomplete",
        };
        println!("{:<18} {:<11} {:>8}  {}", model.id, status, models::format_bytes(model.download_bytes()), model.summary);
    }
}

fn install(dir: &std::path::Path, what: Option<&str>) {
    let targets: Vec<&'static models::Model> = match what {
        None => models::missing(dir, recommended()),
        Some(name) => match (Tier::parse(name), models::model(name)) {
            (Some(tier), _) => models::missing(dir, tier),
            (None, Some(model)) => vec![model],
            _ => crate::fail(&format!("unknown tier or model {name:?}")),
        },
    };
    if targets.is_empty() {
        println!("everything requested is already installed");
        return;
    }
    let cancel = Cancel::default();
    for model in targets {
        println!("{} ({})", model.name, models::format_bytes(model.download_bytes()));
        let mut report = |p: models::Progress| {
            let percent = if p.total > 0 { p.done * 100 / p.total } else { 0 };
            print!("\r  {:<11} {:>3}%  {} / {}   ", p.phase, percent, models::format_bytes(p.done), models::format_bytes(p.total));
            let _ = std::io::stdout().flush();
        };
        match models::install(dir, model, &cancel, &mut report) {
            Ok(()) => println!("\r  done                                         "),
            Err(e) => crate::fail(&format!("\n{e}")),
        }
    }
}

fn verify(dir: &std::path::Path) {
    let mut bad = 0;
    for model in models::CATALOG {
        if models::status(dir, model) != Status::Installed {
            continue;
        }
        match models::verify(dir, model) {
            Ok(()) => println!("{:<18} ok", model.id),
            Err(e) => {
                bad += 1;
                println!("{:<18} DAMAGED: {e}", model.id);
            }
        }
    }
    if bad > 0 {
        std::process::exit(1);
    }
}
