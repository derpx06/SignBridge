//! Dictation history: text only, never audio. Newest first, capped, kept for a chosen number of
//! days (0 keeps nothing on disk), stored one JSON object per line in an owner-only file that is
//! rewritten atomically. The storage rules follow Hex's `history.rs`
//! (https://github.com/anomalyco/hex, MIT; see THIRD_PARTY_NOTICES.md).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const MAX_ENTRIES: usize = 1000;
const MAX_BYTES: usize = 2 << 20;
const DAY: u64 = 86_400;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    /// Unix seconds.
    pub at: u64,
    pub text: String,
}

pub struct History {
    path: Option<PathBuf>,
    keep_days: u32,
    /// Newest first.
    entries: Vec<Entry>,
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Default for History {
    fn default() -> Self {
        History::in_memory()
    }
}

impl History {
    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("io.github", "voicio", "voicio").map(|d| d.data_dir().join("history.jsonl"))
    }

    /// Loads (and prunes) the saved history. A damaged line is skipped; a damaged file is kept aside.
    pub fn load(path: Option<PathBuf>, keep_days: u32) -> History {
        let mut entries = Vec::new();
        if let Some(p) = path.as_ref().filter(|_| keep_days > 0) {
            if let Ok(text) = std::fs::read_to_string(p) {
                let mut bad = 0;
                for line in text.lines().filter(|l| !l.trim().is_empty()) {
                    match serde_json::from_str::<Entry>(line) {
                        Ok(e) => entries.push(e),
                        Err(_) => bad += 1,
                    }
                }
                if bad > 0 && entries.is_empty() {
                    crate::durable::set_aside(p);
                }
            }
        }
        entries.reverse(); // the file is oldest first; same-second entries keep their order
        let mut h = History { path, keep_days, entries };
        if h.prune(now()) {
            h.save();
        }
        h
    }

    pub fn in_memory() -> History {
        History { path: None, keep_days: 0, entries: Vec::new() }
    }

    /// Applies retention. Returns whether anything was dropped.
    fn prune(&mut self, now: u64) -> bool {
        let before = self.entries.len();
        if self.keep_days > 0 {
            let oldest = now.saturating_sub(self.keep_days as u64 * DAY);
            self.entries.retain(|e| e.at >= oldest);
        }
        self.entries.truncate(MAX_ENTRIES);
        let mut bytes = 0;
        let keep = self.entries.iter().take_while(|e| {
            bytes += e.text.len();
            bytes <= MAX_BYTES
        }).count();
        self.entries.truncate(keep);
        self.entries.len() != before
    }

    pub fn set_keep_days(&mut self, days: u32) {
        self.keep_days = days;
        if days == 0 {
            if let Some(p) = &self.path {
                let _ = std::fs::remove_file(p);
            }
        } else if self.prune(now()) {
            self.save();
        } else {
            self.save();
        }
    }

    pub fn add(&mut self, text: &str) {
        self.add_at(text, now());
    }

    /// Adds a dictation made at `at` (Unix seconds), as the newest entry.
    pub fn add_at(&mut self, text: &str, at: u64) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.entries.insert(0, Entry { at, text: text.to_string() });
        self.prune(now());
        self.save();
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
            self.save();
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.save();
    }

    /// Entries whose text contains every word of `query` (case-insensitive), newest first, with
    /// their position in the full list.
    pub fn search(&self, query: &str) -> Vec<(usize, &Entry)> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                let text = e.text.to_lowercase();
                words.iter().all(|w| text.contains(w))
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    fn save(&self) {
        let Some(path) = self.path.as_ref().filter(|_| self.keep_days > 0) else { return };
        let mut text = String::new();
        for e in self.entries.iter().rev() {
            if let Ok(line) = serde_json::to_string(e) {
                text.push_str(&line);
                text.push('\n');
            }
        }
        if let Err(e) = crate::durable::write(path, text.as_bytes()) {
            eprintln!("voicio: could not save the history: {e}");
        }
    }
}

/// "just now", "5 min ago", "3 h ago", "yesterday", "4 days ago", or "long ago".
pub fn ago(at: u64, now: u64) -> String {
    let s = now.saturating_sub(at);
    match s {
        0..=44 => "just now".into(),
        45..=3599 => format!("{} min ago", (s / 60).max(1)),
        3600..=86_399 => format!("{} h ago", s / 3600),
        86_400..=172_799 => "yesterday".into(),
        172_800..=2_591_999 => format!("{} days ago", s / 86_400),
        _ => "long ago".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("voicio-hist-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.join("history.jsonl")
    }

    #[test]
    fn entries_survive_a_restart_newest_first() {
        let p = temp("restart");
        let mut h = History::load(Some(p.clone()), 30);
        h.add("first");
        h.add("second");
        let h2 = History::load(Some(p), 30);
        assert_eq!(h2.search("").iter().map(|(_, e)| e.text.as_str()).collect::<Vec<_>>(), ["second", "first"]);
    }

    #[test]
    fn zero_days_keeps_nothing_on_disk() {
        let p = temp("zero");
        let mut h = History::load(Some(p.clone()), 0);
        h.add("secret");
        assert_eq!(h.len(), 1, "still shown while Voicio runs");
        assert!(!p.exists(), "but never written");
    }

    #[test]
    fn old_entries_are_pruned_and_the_list_is_capped() {
        let mut h = History { path: None, keep_days: 7, entries: vec![] };
        h.entries.push(Entry { at: 100 * DAY, text: "new".into() });
        h.entries.push(Entry { at: 10 * DAY, text: "old".into() });
        assert!(h.prune(100 * DAY + 5));
        assert_eq!(h.len(), 1);
        h.keep_days = 0;
        for i in 0..MAX_ENTRIES + 20 {
            h.entries.push(Entry { at: i as u64, text: "x".into() });
        }
        h.prune(0);
        assert_eq!(h.len(), MAX_ENTRIES);
    }

    #[test]
    fn search_matches_all_words_in_any_order_ignoring_case() {
        let mut h = History::in_memory();
        h.add("Send the Report to Priya");
        h.add("buy milk");
        assert_eq!(h.search("priya report").len(), 1);
        assert_eq!(h.search("MILK")[0].1.text, "buy milk");
        assert!(h.search("nothing here").is_empty());
    }

    #[test]
    fn a_damaged_file_is_set_aside_and_the_app_starts_empty() {
        let p = temp("damaged");
        std::fs::write(&p, "{{{not json\n???\n").unwrap();
        let h = History::load(Some(p.clone()), 30);
        assert_eq!(h.len(), 0);
        assert!(!p.exists());
    }

    #[test]
    fn relative_times_read_naturally() {
        assert_eq!(ago(100, 110), "just now");
        assert_eq!(ago(0, 300), "5 min ago");
        assert_eq!(ago(0, 7200), "2 h ago");
        assert_eq!(ago(0, 100_000), "yesterday");
        assert_eq!(ago(0, 400_000), "4 days ago");
    }
}
