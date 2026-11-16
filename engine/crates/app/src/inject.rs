//! Delivers finished text to the focused app on desktops without the GNOME extension.
//! Lives on the UI thread (macOS requires input synthesis there); Enigo and the clipboard are kept
//! alive because some backends (Linux clipboards) only serve data while their owner exists.

use enigo::{Enigo, Keyboard, Settings};
use std::time::Duration;

/// Text longer than this is pasted rather than typed.
const PASTE_OVER: usize = 200;
/// Time for the talk key's modifiers to be let go before Ctrl+V.
const MODIFIER_SETTLE: Duration = Duration::from_millis(250);
/// The user's clipboard comes back this long after the paste.
const RESTORE_AFTER: Duration = Duration::from_millis(500);

#[derive(Default)]
pub struct Injector {
    enigo: Option<Enigo>,
    clipboard: Option<arboard::Clipboard>,
}

pub enum Delivered {
    Typed,
    Clipboard,
}

impl Injector {
    /// Puts `text` into the focused app when `type_text`: long text is pasted (typing costs a few
    /// milliseconds a character), short text typed; either falls back to the other, then to leaving
    /// it on the clipboard.
    pub fn deliver(&mut self, text: &str, type_text: bool) -> Result<Delivered, String> {
        if type_text {
            let long = text.chars().count() > PASTE_OVER;
            let first = if long { self.paste(text) } else { self.type_text(text) };
            match first {
                Ok(()) => return Ok(Delivered::Typed),
                Err(e) => eprintln!("voicio: {} failed ({e})", if long { "pasting" } else { "typing" }),
            }
            let second = if long { self.type_text(text) } else { self.paste(text) };
            match second {
                Ok(()) => return Ok(Delivered::Typed),
                Err(e) => eprintln!("voicio: {} failed too ({e}); leaving the text on the clipboard", if long { "typing" } else { "pasting" }),
            }
        }
        self.copy(text).map(|()| Delivered::Clipboard)
    }

    /// Pastes `text` and gives the user their clipboard back (after Hex): save what was on the
    /// clipboard, paste, and restore it 500 ms later unless something else replaced it meanwhile.
    fn paste(&mut self, text: &str) -> Result<(), String> {
        use enigo::{Direction, Key};
        let saved = self.clipboard()?.get_text().ok();
        self.copy(text)?;
        // The talk key's modifiers may still be down: let go of them, so Ctrl+V is not Ctrl+Shift+V.
        // ponytail: a fixed 250 ms wait and explicit releases instead of reading the modifier state,
        // which needs a per-platform query; add one if a held modifier still leaks into the paste.
        std::thread::sleep(MODIFIER_SETTLE);
        if self.enigo.is_none() {
            self.enigo = Some(Enigo::new(&Settings::default()).map_err(|e| e.to_string())?);
        }
        let enigo = self.enigo.as_mut().expect("just created");
        for key in [Key::Shift, Key::Control, Key::Alt, Key::Meta] {
            let _ = enigo.key(key, Direction::Release);
        }
        #[cfg(target_os = "macos")]
        let command = Key::Meta;
        #[cfg(not(target_os = "macos"))]
        let command = Key::Control;
        let pasted = enigo
            .key(command, Direction::Press)
            .and_then(|()| enigo.key(Key::Unicode('v'), Direction::Click))
            .map_err(|e| e.to_string());
        let _ = enigo.key(command, Direction::Release);
        pasted?;
        if let Some(saved) = saved {
            std::thread::sleep(RESTORE_AFTER);
            let clipboard = self.clipboard()?;
            if clipboard.get_text().ok().as_deref() == Some(text) {
                clipboard.set_text(saved).map_err(|e| format!("clipboard: {e}"))?;
            }
        }
        Ok(())
    }

    fn clipboard(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.clipboard.is_none() {
            self.clipboard = Some(arboard::Clipboard::new().map_err(|e| format!("clipboard: {e}"))?);
        }
        Ok(self.clipboard.as_mut().expect("just created"))
    }

    fn type_text(&mut self, text: &str) -> Result<(), String> {
        if self.enigo.is_none() {
            // On macOS this prompts for the Accessibility permission the first time.
            self.enigo = Some(Enigo::new(&Settings::default()).map_err(|e| e.to_string())?);
        }
        let enigo = self.enigo.as_mut().expect("just created");
        enigo.text(text).map_err(|e| e.to_string())
    }

    pub fn copy(&mut self, text: &str) -> Result<(), String> {
        self.clipboard()?.set_text(text.to_owned()).map_err(|e| format!("clipboard: {e}"))
    }
}
