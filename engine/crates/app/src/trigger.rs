//! The dictation shortcut as a pure state machine, so the timing rules are unit-tested.
//!
//! Three behaviours, chosen in Settings (`Mode`):
//! - **Hold** (default): recording lasts exactly as long as the key is held. A press shorter than
//!   `MIN_HOLD` is an accident and is discarded with a hint; it never latches.
//! - **Toggle**: a press starts, the next press stops.
//! - **Hold or tap**: holding is push-to-talk, a quick tap latches until the next tap.
//!
//! Three real-world quirks are handled here:
//! - X11 sends repeated "press" events while a key is held: ignored, never mistaken for a second tap.
//! - X11 also sends a release immediately followed by a press for each repeat: a release is only
//!   acted on after `RELEASE_GRACE`, and a press inside that window cancels it.
//! - Double-bounce from a worn key: presses within `DEBOUNCE` of each other count once.

use std::time::{Duration, Instant};

pub const HOLD_TO_TALK: Duration = Duration::from_millis(300);
/// In `Hold` mode a shorter press is a slip of the finger, not a dictation.
pub const MIN_HOLD: Duration = Duration::from_millis(250);
const RELEASE_GRACE: Duration = Duration::from_millis(50);
/// After letting go of a held key the mic stays open this long, so the last word is not clipped
/// (Hex measured the final word surviving on 10 of 10 clips with a tail, 4 of 10 without).
const RELEASE_TAIL: Duration = Duration::from_millis(150);
const DEBOUNCE: Duration = Duration::from_millis(30);

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Mode {
    #[default]
    Hold,
    Toggle,
    HoldOrTap,
}

impl Mode {
    pub fn parse(text: &str) -> Mode {
        match text {
            "toggle" => Mode::Toggle,
            "hold-or-tap" => Mode::HoldOrTap,
            _ => Mode::Hold,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Dictation {
    Idle,
    Listening,
    Busy, // finishing: a press cannot start anything yet
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Action {
    None,
    Start,
    Stop,
    /// The key was let go too soon (Hold mode): throw the recording away and say why.
    Discard,
    /// The key was pressed while the last dictation is still being finished.
    Busy,
}

#[derive(Default)]
pub struct Trigger {
    mode: Mode,
    held: bool,
    /// When the dictation this press started began; `None` once the press was a "stop" tap.
    started_at: Option<Instant>,
    last_press: Option<Instant>,
    /// A release waiting out the grace period: (when it happened, when it takes effect).
    release: Option<(Instant, Instant)>,
}

impl Trigger {
    pub fn new(mode: Mode) -> Trigger {
        Trigger { mode, ..Trigger::default() }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    /// The key that started the current dictation is still down.
    pub fn holding(&self) -> bool {
        self.held && self.started_at.is_some()
    }

    pub fn press(&mut self, now: Instant, dictation: Dictation) -> Action {
        if self.release.take().is_some() {
            self.held = true; // key repeat: the "release" was not real, the key is still down
            return Action::None;
        }
        if self.held || self.last_press.is_some_and(|t| now.duration_since(t) < DEBOUNCE) {
            return Action::None;
        }
        self.held = true;
        self.last_press = Some(now);
        match dictation {
            Dictation::Idle => {
                self.started_at = Some(now);
                Action::Start
            }
            Dictation::Listening => {
                self.started_at = None;
                Action::Stop
            }
            Dictation::Busy => {
                self.started_at = None;
                Action::Busy
            }
        }
    }

    pub fn release(&mut self, now: Instant) {
        if !self.held {
            return;
        }
        self.held = false;
        if let Some(started) = self.started_at {
            let held_to_talk = self.mode != Mode::Toggle && now.duration_since(started) >= MIN_HOLD.min(HOLD_TO_TALK);
            self.release = Some((now, now + if held_to_talk { RELEASE_TAIL } else { RELEASE_GRACE }));
        }
    }

    /// Call when time passes: acts on a release whose grace period has ended.
    pub fn due(&mut self, now: Instant) -> Action {
        let Some((released, effective)) = self.release else { return Action::None };
        if now < effective {
            return Action::None;
        }
        self.release = None;
        let Some(started) = self.started_at.take() else { return Action::None };
        let held_for = released.duration_since(started);
        match self.mode {
            Mode::Hold if held_for >= MIN_HOLD => Action::Stop,
            Mode::Hold => Action::Discard,
            Mode::HoldOrTap if held_for >= HOLD_TO_TALK => Action::Stop,
            // a tap: keep listening until the next press
            Mode::HoldOrTap | Mode::Toggle => Action::None,
        }
    }

    /// The next moment `due` has something to do, so the caller can wake up in time.
    pub fn deadline(&self) -> Option<Instant> {
        self.release.map(|(_, effective)| effective)
    }

    /// The dictation ended some other way (cancel, error, silence): forget the key state.
    /// The key may still be down (a cancel while holding), so `held` survives: a repeat event
    /// must not start a new dictation.
    pub fn reset(&mut self) {
        *self = Trigger { mode: self.mode, held: self.held, ..Trigger::default() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    fn new(mode: Mode) -> (Trigger, Instant) {
        (Trigger::new(mode), Instant::now())
    }

    // ---- Hold (the default): listen exactly while the key is down ----

    #[test]
    fn hold_records_while_held_and_stops_after_the_tail() {
        let (mut t, b) = new(Mode::Hold);
        assert_eq!(t.press(at(b, 0), Dictation::Idle), Action::Start);
        assert!(t.holding());
        t.release(at(b, 2000));
        assert_eq!(t.due(at(b, 2100)), Action::None, "the mic stays open for the tail");
        assert_eq!(t.due(at(b, 2160)), Action::Stop);
    }

    #[test]
    fn a_quick_tap_never_latches_in_hold_mode() {
        let (mut t, b) = new(Mode::Hold);
        assert_eq!(t.press(at(b, 0), Dictation::Idle), Action::Start);
        t.release(at(b, 120));
        assert_eq!(t.due(at(b, 200)), Action::Discard, "too short: thrown away, not left recording");
    }

    #[test]
    fn a_press_while_finishing_says_busy_instead_of_doing_nothing() {
        let (mut t, b) = new(Mode::Hold);
        assert_eq!(t.press(at(b, 0), Dictation::Busy), Action::Busy);
        t.release(at(b, 800));
        assert_eq!(t.due(at(b, 900)), Action::None);
    }

    #[test]
    fn a_cancel_while_the_key_is_down_does_not_restart_on_key_repeat() {
        let (mut t, b) = new(Mode::Hold);
        assert_eq!(t.press(at(b, 0), Dictation::Idle), Action::Start);
        t.reset(); // the dictation was cancelled, the key is still down
        assert_eq!(t.press(at(b, 500), Dictation::Idle), Action::None, "key repeat is not a new press");
        t.release(at(b, 900));
        assert_eq!(t.due(at(b, 1100)), Action::None);
        assert_eq!(t.press(at(b, 2000), Dictation::Idle), Action::Start, "a real new press works again");
    }

    #[test]
    fn key_repeat_never_acts_as_a_second_press() {
        let (mut t, b) = new(Mode::Hold);
        assert_eq!(t.press(at(b, 0), Dictation::Idle), Action::Start);
        assert_eq!(t.press(at(b, 500), Dictation::Listening), Action::None);
        assert_eq!(t.press(at(b, 540), Dictation::Listening), Action::None);
    }

    #[test]
    fn x11_release_then_press_repeat_is_ignored() {
        let (mut t, b) = new(Mode::Hold);
        assert_eq!(t.press(at(b, 0), Dictation::Idle), Action::Start);
        t.release(at(b, 600));
        assert_eq!(t.press(at(b, 610), Dictation::Listening), Action::None); // cancels the release
        t.release(at(b, 1500));
        assert_eq!(t.due(at(b, 1660)), Action::Stop); // the real release ends it
    }

    #[test]
    fn a_second_press_within_debounce_counts_once() {
        let (mut t, b) = new(Mode::Hold);
        assert_eq!(t.press(at(b, 0), Dictation::Idle), Action::Start);
        t.release(at(b, 5));
        assert_eq!(t.due(at(b, 60)), Action::Discard);
        assert_eq!(t.press(at(b, 25), Dictation::Idle), Action::None); // bounce: too soon after the first
    }

    // ---- Toggle and hold-or-tap, for people who prefer them ----

    #[test]
    fn toggle_starts_on_one_press_and_stops_on_the_next() {
        let (mut t, b) = new(Mode::Toggle);
        assert_eq!(t.press(at(b, 0), Dictation::Idle), Action::Start);
        t.release(at(b, 2000)); // even a long hold does not stop it
        assert_eq!(t.due(at(b, 2200)), Action::None);
        assert_eq!(t.press(at(b, 5000), Dictation::Listening), Action::Stop);
    }

    #[test]
    fn hold_or_tap_latches_on_a_tap_and_stops_on_release_after_a_hold() {
        let (mut t, b) = new(Mode::HoldOrTap);
        assert_eq!(t.press(at(b, 0), Dictation::Idle), Action::Start);
        t.release(at(b, 100));
        assert_eq!(t.due(at(b, 200)), Action::None, "a tap keeps listening");
        assert_eq!(t.press(at(b, 3000), Dictation::Listening), Action::Stop);
        t.release(at(b, 3100));
        assert_eq!(t.due(at(b, 3300)), Action::None);

        let (mut t, b) = new(Mode::HoldOrTap);
        assert_eq!(t.press(at(b, 0), Dictation::Idle), Action::Start);
        t.release(at(b, 2000));
        assert_eq!(t.due(at(b, 2160)), Action::Stop);
    }

    #[test]
    fn modes_parse_from_the_settings_text() {
        assert_eq!(Mode::parse("hold"), Mode::Hold);
        assert_eq!(Mode::parse("toggle"), Mode::Toggle);
        assert_eq!(Mode::parse("hold-or-tap"), Mode::HoldOrTap);
        assert_eq!(Mode::parse("nonsense"), Mode::Hold, "unknown text falls back to the safe default");
    }
}
