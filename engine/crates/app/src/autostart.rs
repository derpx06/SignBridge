//! Launch at login, per OS: the systemd user unit (when installed) or an XDG autostart entry on
//! Linux, the HKCU Run key on Windows, a LaunchAgent on macOS.

#[cfg(not(windows))]
use std::path::PathBuf;
#[cfg(not(target_os = "macos"))]
use std::process::Command;

fn exe() -> String {
    std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "voicio".into())
}

#[cfg(not(windows))]
fn home() -> PathBuf {
    directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()).unwrap_or_default()
}

#[cfg(target_os = "linux")]
const UNIT: &str = "voicio.service";

#[cfg(target_os = "linux")]
fn unit_installed() -> bool {
    home().join(".config/systemd/user").join(UNIT).is_file()
}

#[cfg(target_os = "linux")]
fn desktop_entry() -> PathBuf {
    home().join(".config/autostart/io.github.voicio.desktop")
}

#[cfg(target_os = "linux")]
pub fn enabled() -> bool {
    if unit_installed() {
        Command::new("systemctl").args(["--user", "is-enabled", "--quiet", UNIT]).status().is_ok_and(|s| s.success())
    } else {
        desktop_entry().is_file()
    }
}

#[cfg(target_os = "linux")]
pub fn set(enable: bool) -> Result<(), String> {
    if unit_installed() {
        let verb = if enable { "enable" } else { "disable" };
        let ok = Command::new("systemctl").args(["--user", verb, UNIT]).status().is_ok_and(|s| s.success());
        return if ok { Ok(()) } else { Err(format!("systemctl --user {verb} {UNIT} failed")) };
    }
    let path = desktop_entry();
    if enable {
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=Voicio\nComment=On-device dictation\nExec={}\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n",
            exe()
        );
        std::fs::create_dir_all(path.parent().unwrap_or(&path)).map_err(|e| e.to_string())?;
        std::fs::write(&path, entry).map_err(|e| e.to_string())
    } else {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }
}

#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

#[cfg(windows)]
pub fn enabled() -> bool {
    Command::new("reg").args(["query", RUN_KEY, "/v", "Voicio"]).output().is_ok_and(|o| o.status.success())
}

#[cfg(windows)]
pub fn set(enable: bool) -> Result<(), String> {
    let status = if enable {
        let value = format!("\"{}\"", exe());
        Command::new("reg").args(["add", RUN_KEY, "/v", "Voicio", "/t", "REG_SZ", "/d", &value, "/f"]).status()
    } else {
        Command::new("reg").args(["delete", RUN_KEY, "/v", "Voicio", "/f"]).status()
    };
    match status {
        Ok(s) if s.success() || !enable => Ok(()),
        _ => Err("could not update the Run registry key".into()),
    }
}

#[cfg(target_os = "macos")]
fn agent_plist() -> PathBuf {
    home().join("Library/LaunchAgents/io.github.voicio.plist")
}

#[cfg(target_os = "macos")]
pub fn enabled() -> bool {
    agent_plist().is_file()
}

#[cfg(target_os = "macos")]
pub fn set(enable: bool) -> Result<(), String> {
    let path = agent_plist();
    if enable {
        let plist = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>io.github.voicio</string>\n<key>ProgramArguments</key><array><string>{}</string></array>\n<key>RunAtLoad</key><true/>\n<key>ProcessType</key><string>Interactive</string>\n</dict></plist>\n",
            exe()
        );
        std::fs::create_dir_all(path.parent().unwrap_or(&path)).map_err(|e| e.to_string())?;
        std::fs::write(&path, plist).map_err(|e| e.to_string())
    } else {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }
}
