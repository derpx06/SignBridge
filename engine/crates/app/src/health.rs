//! "Is everything Voicio needs in place?" as plain facts in, a short list of problems out, each
//! with what to do about it. The Home page shows the first one; `voicio doctor` can print all.
//! Pure on purpose, so every case is unit-tested and nothing ever fails silently.

use crate::desktop::Desktop;

#[derive(Debug, Clone)]
pub struct Facts {
    pub desktop: Desktop,
    pub has_mic: bool,
    pub models_ready: bool,
    pub extension_attached: bool,
    pub portal_typing_allowed: bool,
    pub xwayland: bool,
    /// The desktop's shortcut service accepted the shortcut (or a loaded extension owns it).
    pub shortcut_ready: bool,
    /// The desktop's shortcut service said no (or never answered) the last time Voicio asked.
    pub shortcut_denied: bool,
    /// What the GNOME keybinding fallback did after the portal failed.
    pub fallback: Option<crate::hotkey::Fallback>,
}

/// A button the banner can offer next to the explanation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// Ask the desktop for the shortcut again (shows its prompt again).
    AskShortcut,
    /// Ask the desktop for permission to type again.
    AskTyping,
}

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::AskShortcut => "Show the prompt again",
            Action::AskTyping => "Allow typing",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    /// What is wrong, in a sentence a person can act on.
    pub problem: String,
    /// What to do about it.
    pub fix: String,
    pub action: Option<Action>,
}

fn issue(problem: &str, fix: &str) -> Issue {
    Issue { problem: problem.into(), fix: fix.into(), action: None }
}

fn with(mut i: Issue, action: Action) -> Issue {
    i.action = Some(action);
    i
}

/// Problems in order of importance (the first one blocks dictation outright).
pub fn check(f: &Facts) -> Vec<Issue> {
    let mut out = Vec::new();
    use crate::hotkey::Fallback;
    if let Some(Fallback::Active { note: Some(note), .. }) = &f.fallback {
        out.push(issue("Voicio is using a different shortcut.", note));
    }
    if let Some(Fallback::Failed(why)) = &f.fallback {
        out.push(issue(&format!("The shortcut could not be set up: {why}."), "Bind `voicio toggle` to a key in your desktop's keyboard settings (Sway/Hyprland: `bindsym F9 exec voicio toggle` in the config; KDE: Shortcuts > Add Command). If GNOME says the key is taken, choose another in Settings."));
    } else if matches!(f.desktop, Desktop::Gnome | Desktop::Wayland) && !f.shortcut_ready {
        out.push(with(
            if f.shortcut_denied {
                issue(
                    "The shortcut was not allowed.",
                    "Press the button and choose Allow when your desktop asks. If no prompt appears, set the shortcut in your desktop's keyboard settings to `voicio toggle`.",
                )
            } else {
                issue(
                    "The shortcut is not active yet.",
                    "If your desktop shows a prompt to allow Voicio's shortcut, press Allow. Missed it? Press the button to see it again.",
                )
            },
            Action::AskShortcut,
        ));
    }
    if !f.has_mic {
        out.push(issue("No microphone found.", "Plug in or enable a microphone, then try again."));
    }
    if !f.models_ready {
        out.push(issue("The speech models are not installed.", "Open Models and press \"Get what I need\" (one download, then it works offline)."));
    }
    match f.desktop {
        Desktop::Gnome if !f.extension_attached && !f.portal_typing_allowed => out.push(with(
            issue(
                "Voicio is not allowed to type for you yet.",
                "Press the button and choose Allow when GNOME asks, or enable the Voicio GNOME extension (log out and in after installing it).",
            ),
            Action::AskTyping,
        )),
        Desktop::Wayland if !f.portal_typing_allowed => out.push(with(
            issue(
                "Typing into other apps needs your permission.",
                "Press the button and choose Allow when the desktop asks. If you use Sway or Hyprland, installing `wtype` avoids the prompt.",
            ),
            Action::AskTyping,
        )),
        _ => {}
    }
    if matches!(f.desktop, Desktop::Gnome | Desktop::Wayland) && !f.xwayland && !f.extension_attached {
        out.push(issue("There is no on-screen pill on this session.", "Dictation still works. Enable the GNOME extension or XWayland to see the pill."));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(desktop: Desktop) -> Facts {
        Facts { desktop, has_mic: true, models_ready: true, extension_attached: false, portal_typing_allowed: true, xwayland: true, shortcut_ready: true, shortcut_denied: false, fallback: None }
    }

    #[test]
    fn a_healthy_setup_reports_nothing() {
        for d in [Desktop::Gnome, Desktop::Wayland, Desktop::X11, Desktop::Windows, Desktop::MacOs] {
            assert!(check(&ok(d)).is_empty(), "{d:?}");
        }
    }

    #[test]
    fn the_most_blocking_problem_comes_first() {
        let issues = check(&Facts { has_mic: false, models_ready: false, ..ok(Desktop::X11) });
        assert!(issues[0].problem.contains("microphone"));
        assert!(issues[1].problem.contains("models"));
    }

    #[test]
    fn gnome_without_the_extension_or_permission_explains_both_ways_out() {
        let issues = check(&Facts { portal_typing_allowed: false, ..ok(Desktop::Gnome) });
        assert_eq!(issues.len(), 1);
        assert!(issues[0].fix.contains("Allow") && issues[0].fix.contains("extension"));
        assert_eq!(issues[0].action, Some(Action::AskTyping));
        // the extension being active makes the permission irrelevant
        assert!(check(&Facts { portal_typing_allowed: false, extension_attached: true, ..ok(Desktop::Gnome) }).is_empty());
    }

    #[test]
    fn an_unbound_shortcut_is_reported_on_wayland_but_not_elsewhere() {
        let first = &check(&Facts { shortcut_ready: false, ..ok(Desktop::Gnome) })[0];
        assert!(first.problem.contains("shortcut"));
        assert_eq!(first.action, Some(Action::AskShortcut), "a missed prompt can always be shown again");
        let denied = &check(&Facts { shortcut_ready: false, shortcut_denied: true, ..ok(Desktop::Gnome) })[0];
        assert!(denied.problem.contains("not allowed") && denied.fix.contains("voicio toggle"));
        assert!(check(&Facts { shortcut_ready: false, ..ok(Desktop::X11) }).is_empty(), "X11 registers it directly");
    }

    #[test]
    fn a_working_fallback_key_is_not_a_problem_but_a_swap_is_explained() {
        use crate::hotkey::Fallback;
        let base = Facts { shortcut_ready: true, fallback: Some(Fallback::Active { key: "F9".into(), note: None }), ..ok(Desktop::Gnome) };
        assert!(check(&base).is_empty());
        let swapped = check(&Facts { fallback: Some(Fallback::Active { key: "Super+Alt+Space".into(), note: Some("F9 is already used by hex, so Voicio uses Super+Alt+Space.".into()) }), ..base.clone() });
        assert!(swapped[0].fix.contains("Super+Alt+Space"));
        let failed = check(&Facts { shortcut_ready: false, fallback: Some(Fallback::Failed("F9 is already used by another app".into())), ..ok(Desktop::Gnome) });
        assert!(failed[0].problem.contains("already used") && failed[0].action.is_none());
    }

    #[test]
    fn no_pill_is_reported_but_never_as_blocking() {
        let issues = check(&Facts { xwayland: false, ..ok(Desktop::Wayland) });
        assert_eq!(issues.len(), 1);
        assert!(issues[0].fix.starts_with("Dictation still works"));
    }
}
