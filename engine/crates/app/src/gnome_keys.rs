//! GNOME's shortcut portal can crash or refuse. A plain GNOME custom keybinding that runs
//! `voicio toggle` always works, so the app installs one itself (tap to start and stop only) and
//! says so when another app already uses the key.

use std::process::Command;

const SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const LIST_KEY: &str = "custom-keybindings";
const SLOT: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/voicio/";
const NAME: &str = "Voicio dictate";

/// "Super+Alt+Space" -> "<Super><Alt>space"; "F9" -> "F9".
pub fn accel(shortcut: &str) -> String {
    shortcut
        .split('+')
        .map(|p| match p.trim().to_ascii_uppercase().as_str() {
            "SUPER" | "META" | "WIN" | "CMD" => "<Super>".to_string(),
            "CTRL" | "CONTROL" => "<Control>".to_string(),
            "ALT" | "OPTION" => "<Alt>".to_string(),
            "SHIFT" => "<Shift>".to_string(),
            k if k.len() > 1 && k.starts_with('F') && k[1..].chars().all(|c| c.is_ascii_digit()) => k.to_string(),
            _ => p.trim().to_ascii_lowercase(),
        })
        .collect()
}

/// The same key written two ways compares equal: modifiers sorted, case ignored.
fn canon(accel: &str) -> String {
    let lower = accel.to_ascii_lowercase();
    let mut mods: Vec<&str> = lower.split_inclusive('>').filter(|p| p.ends_with('>')).collect();
    mods.sort();
    let key = lower.rsplit('>').next().unwrap_or("");
    format!("{}{key}", mods.concat())
}

/// Another app that already uses `accel`, found in the shortcuts GNOME has stored.
pub fn conflict(accel: &str, dump: &str) -> Option<String> {
    let want = canon(accel);
    let mut section = String::new();
    for line in dump.lines() {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.rsplit('/').next().unwrap_or(name).to_string();
            continue;
        }
        let ours = matches!(section.as_str(), "voicio" | "whispr" | "io.github.voicio.Voicio");
        if ours || line.starts_with("applications") {
            continue;
        }
        if line.split('\'').skip(1).step_by(2).any(|tok| canon(tok) == want) {
            return Some(section.clone());
        }
    }
    None
}

fn gsettings(args: &[&str]) -> Result<String, String> {
    let out = Command::new("gsettings").args(args).output().map_err(|e| format!("gsettings: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

fn dump(path: &str) -> String {
    Command::new("dconf").args(["dump", path]).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

/// The slot already running `voicio toggle` (an older install may have made one), else ours.
fn our_slot(list: &str) -> String {
    for path in list.split('\'').skip(1).step_by(2) {
        let schema = format!("{SCHEMA}.custom-keybinding:{path}");
        if gsettings(&["get", &schema, "command"]).is_ok_and(|c| c.contains("voicio") && c.contains("toggle")) {
            return path.to_string();
        }
    }
    SLOT.to_string()
}

/// What `ensure` set up: the key now in use, and a sentence when it is not the one asked for.
pub struct Installed {
    pub key: String,
    pub note: Option<String>,
}

/// Makes sure a GNOME keybinding runs `voicio toggle`: on `shortcut`, or on the first free key from
/// a short list when another app has it. Err when nothing works.
pub fn ensure(shortcut: &str) -> Result<Installed, String> {
    let mut taken = dump("/org/gnome/settings-daemon/global-shortcuts/");
    taken.push_str(&dump("/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/"));
    let mut owner = None;
    let pick = [shortcut, "Super+Alt+Space", "Ctrl+Alt+Space", "Super+Shift+Space"].into_iter().find(|c| match conflict(&accel(c), &taken) {
        Some(other) => {
            owner.get_or_insert((c.to_string(), other));
            false
        }
        None => true,
    });
    let Some(pick) = pick else { return Err(format!("{shortcut} is already used by another app")) };
    let note = owner.map(|(asked, other)| format!("{asked} is already used by {other}, so Voicio uses {pick} (tap to start, tap to stop)."));
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let list = gsettings(&["get", SCHEMA, LIST_KEY])?;
    let slot = our_slot(&list);
    let base = format!("{SCHEMA}.custom-keybinding:{slot}");
    gsettings(&["set", &base, "name", NAME])?;
    gsettings(&["set", &base, "command", &format!("{} toggle", exe.display())])?;
    gsettings(&["set", &base, "binding", &accel(pick)])?;
    if !list.contains(&slot) {
        let new = if list.contains("@as []") || list.trim() == "[]" { format!("['{slot}']") } else { format!("{}, '{slot}']", list.trim_end_matches(']')) };
        gsettings(&["set", SCHEMA, LIST_KEY, &new])?;
    }
    Ok(Installed { key: pick.to_string(), note })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_become_gtk_accelerators() {
        assert_eq!(accel("Super+Alt+Space"), "<Super><Alt>space");
        assert_eq!(accel("F9"), "F9");
        assert_eq!(accel("Ctrl+Shift+D"), "<Control><Shift>d");
    }

    #[test]
    fn a_key_used_by_another_app_is_found_whatever_the_modifier_order() {
        let dump = "[anomaly.hex]\nshortcuts=[('capture-F9', {'shortcuts': <['F9']>, 'description': <'x'>})]\n[io.github.voicio.Voicio]\nshortcuts=[('dictate', {'shortcuts': <['F9']>})]\n";
        assert_eq!(conflict("F9", dump).as_deref(), Some("anomaly.hex"));
        let d2 = "[other]\nbinding='<Alt><Super>space'\n";
        assert_eq!(conflict("<Super><Alt>space", d2).as_deref(), Some("other"));
        assert_eq!(conflict("F10", dump), None);
    }
}
