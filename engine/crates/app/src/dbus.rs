//! D-Bus service for Linux front-ends (the GNOME Shell extension).
//!
//! `io.github.voicio.Engine` at `/io/github/voicio/Engine`:
//! methods `Start() Stop() Toggle() Cancel() OpenSettings() Attach() Detach()`, `Status() -> (state, rss_mb, peak_mb, model)`;
//! signals `Preview(text)` (everything said so far), `Done(text)` (final, cleaned),
//! `StateChanged(state)` (idle | listening | finishing), `Level(0..1)` (mic loudness, ~20/s), `Error(message)`.

use crate::agent::{Cmd, Msg};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zbus::blocking::object_server::InterfaceRef;
use zbus::object_server::SignalEmitter;
use zbus::{block_on, interface};

/// Tests set VOICIO_INSTANCE so they never claim the real agent's name.
fn bus_name() -> String {
    match std::env::var("VOICIO_INSTANCE") {
        Ok(suffix) if !suffix.is_empty() => format!("io.github.voicio.Engine.{}", suffix.replace(|c: char| !c.is_ascii_alphanumeric(), "_")),
        _ => "io.github.voicio.Engine".into(),
    }
}
const OBJECT_PATH: &str = "/io/github/voicio/Engine";

/// The bus name of the connected GNOME extension (the shell), if one has attached.
type Attached = Arc<Mutex<Option<Ext>>>;

/// What the connected extension does for itself: older ones draw their own pill and own the
/// shortcut; newer ones leave both to the agent (they type, and ask if the shortcut is bound).
#[derive(Clone, Debug)]
pub struct Ext {
    name: Option<String>,
    pub draws_pill: bool,
    pub owns_shortcut: bool,
}

struct Service {
    tx: Mutex<Sender<Msg>>,
    attached: Attached,
}

impl Service {
    fn command(&self, cmd: Cmd) {
        let _ = self.tx.lock().unwrap_or_else(|e| e.into_inner()).send(Msg::Command(cmd));
    }
}

#[interface(name = "io.github.voicio.Engine")]
impl Service {
    fn start(&self) {
        self.command(Cmd::Start);
    }

    fn stop(&self) {
        self.command(Cmd::Stop);
    }

    fn toggle(&self) {
        self.command(Cmd::Toggle);
    }

    fn cancel(&self) {
        self.command(Cmd::Cancel);
    }

    /// The GNOME extension announces itself, so the agent knows someone is there to type the text.
    fn attach(&self, #[zbus(header)] header: zbus::message::Header<'_>) {
        let name = header.sender().map(|s| s.to_string());
        *self.attached.lock().unwrap_or_else(|e| e.into_inner()) = Some(Ext { name, draws_pill: true, owns_shortcut: true });
    }

    /// Newer extensions: they only type. Returns whether the agent draws the pill itself (so the
    /// extension should not draw one too).
    fn attach2(&self, #[zbus(header)] header: zbus::message::Header<'_>) -> bool {
        let name = header.sender().map(|s| s.to_string());
        *self.attached.lock().unwrap_or_else(|e| e.into_inner()) = Some(Ext { name, draws_pill: false, owns_shortcut: false });
        crate::desktop::wants_xwayland()
    }

    /// Whether the desktop's shortcut service has accepted Voicio's shortcut. If not, a newer
    /// extension binds the key itself as a fallback.
    fn shortcut_bound(&self) -> bool {
        crate::hotkey::portal_bound()
    }

    fn detach(&self) {
        *self.attached.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    fn open_settings(&self) {
        self.command(Cmd::OpenSettings);
    }

    fn status(&self) -> (String, f64, f64, String) {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.command(Cmd::Status(reply_tx));
        match reply_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(s) => (s.state.to_string(), s.rss_mb.round(), s.peak_mb.round(), s.model),
            Err(_) => ("idle".into(), 0.0, 0.0, "agent busy".into()),
        }
    }

    #[zbus(signal)]
    async fn preview(emitter: &SignalEmitter<'_>, text: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn done(emitter: &SignalEmitter<'_>, text: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn state_changed(emitter: &SignalEmitter<'_>, state: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn level(emitter: &SignalEmitter<'_>, level: f64) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn error(emitter: &SignalEmitter<'_>, message: &str) -> zbus::Result<()>;
}

pub struct Bus {
    _connection: zbus::blocking::Connection,
    iface: InterfaceRef<Service>,
    attached: Attached,
}

impl Bus {
    pub fn connect(tx: Sender<Msg>) -> Result<Bus, String> {
        let attached: Attached = Arc::new(Mutex::new(None));
        let service = Service { tx: Mutex::new(tx), attached: attached.clone() };
        let connection = zbus::blocking::connection::Builder::session()
            .and_then(|b| b.name(bus_name()))
            .and_then(|b| b.serve_at(OBJECT_PATH, service))
            .and_then(|b| b.build())
            .map_err(|e| format!("D-Bus: {e} (is another Voicio already running?)"))?;
        let iface = connection.object_server().interface::<_, Service>(OBJECT_PATH).map_err(|e| e.to_string())?;
        watch_extension(&connection, attached.clone());
        Ok(Bus { _connection: connection, iface, attached })
    }

    pub fn extension_attached(&self) -> bool {
        self.attached.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    pub fn extension(&self) -> Option<Ext> {
        self.attached.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn preview(&self, text: &str) {
        let _ = block_on(Service::preview(self.iface.signal_emitter(), text));
    }

    pub fn done(&self, text: &str) {
        let _ = block_on(Service::done(self.iface.signal_emitter(), text));
    }

    pub fn state(&self, state: &str) {
        let _ = block_on(Service::state_changed(self.iface.signal_emitter(), state));
    }

    pub fn level(&self, level: f64) {
        let _ = block_on(Service::level(self.iface.signal_emitter(), level));
    }

    pub fn error(&self, message: &str) {
        let _ = block_on(Service::error(self.iface.signal_emitter(), message));
    }
}

/// The extension never says goodbye when the shell crashes or restarts. Watch its bus name, so a
/// vanished extension hands the pill and the clipboard fallback back to the agent.
fn watch_extension(connection: &zbus::blocking::Connection, attached: Attached) {
    let Ok(proxy) = zbus::blocking::fdo::DBusProxy::new(connection) else { return };
    let Ok(signals) = proxy.receive_name_owner_changed() else { return };
    let _ = std::thread::Builder::new().name("dbus-watch".into()).spawn(move || {
        for signal in signals {
            let Ok(args) = signal.args() else { continue };
            if args.new_owner().is_none() {
                let mut attached = attached.lock().unwrap_or_else(|e| e.into_inner());
                if attached.as_ref().and_then(|e| e.name.as_deref()) == Some(args.name().as_str()) {
                    eprintln!("voicio: the GNOME extension left the bus; drawing the pill and keeping text on the clipboard");
                    *attached = None;
                }
            }
        }
    });
}

/// Desktop notification (org.freedesktop.Notifications), used when text went to the clipboard.
pub fn notify(summary: &str, body: &str) {
    let result = zbus::blocking::Connection::session().and_then(|connection| {
        let hints: std::collections::HashMap<&str, zbus::zvariant::Value> = std::collections::HashMap::new();
        connection.call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &("Voicio", 0u32, "audio-input-microphone-symbolic", summary, body, Vec::<&str>::new(), hints, 4000i32),
        )
    });
    if let Err(e) = result {
        eprintln!("voicio: {summary}: {body} (notification failed: {e})");
    }
}

