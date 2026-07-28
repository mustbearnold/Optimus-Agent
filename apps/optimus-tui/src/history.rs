//! Prompts already sent, and walking back through them.
//!
//! Recall is a cursor over a bounded, newest-first ring. Stepping off the
//! newest end restores the draft the human was typing before they started
//! browsing — losing it is the thing that makes history feel hostile.
//!
//! Persistence follows [`crate::preferences`]: best effort, never reported.
//! A prompt is a thing you typed, not a secret store, but the file lives in
//! the Optimus home beside the rest of the session state rather than in a
//! shared shell history.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE: &str = "tui-history.json";
/// Enough to walk back through a working session; small enough that the file
/// stays trivial to read and rewrite on every submit.
const CAP: usize = 200;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct History {
    /// Newest first.
    entries: Vec<String>,
    /// Position while browsing: `None` means "on the live draft".
    #[serde(skip)]
    cursor: Option<usize>,
    /// The draft stashed when browsing began.
    #[serde(skip)]
    stashed: String,
}

impl History {
    pub fn load(home: &Path) -> Self {
        std::fs::read_to_string(path(home))
            .ok()
            .and_then(|text| serde_json::from_str::<Self>(&text).ok())
            .map(Self::sanitised)
            .unwrap_or_default()
    }

    pub fn save(&self, home: &Path) {
        let Ok(text) = serde_json::to_string_pretty(self) else {
            return;
        };
        let _ = std::fs::create_dir_all(home);
        let _ = std::fs::write(path(home), text);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Record a submitted prompt and leave browsing.
    ///
    /// A repeat of the newest entry is not recorded: re-running the same
    /// command should not push the rest of history out of reach.
    pub fn record(&mut self, prompt: &str) {
        self.cursor = None;
        self.stashed.clear();
        let prompt = prompt.trim();
        if prompt.is_empty() || self.entries.first().map(String::as_str) == Some(prompt) {
            return;
        }
        self.entries.insert(0, prompt.to_string());
        self.entries.truncate(CAP);
    }

    /// Step to an older entry. Returns the text to show, or `None` when there
    /// is nothing older (the draft stays untouched).
    pub fn older(&mut self, draft: &str) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        let next = match self.cursor {
            None => {
                self.stashed = draft.to_string();
                0
            }
            Some(index) if index + 1 < self.entries.len() => index + 1,
            Some(_) => return None,
        };
        self.cursor = Some(next);
        self.entries.get(next).cloned()
    }

    /// Step to a newer entry, or back to the stashed draft at the newest end.
    pub fn newer(&mut self) -> Option<String> {
        match self.cursor {
            None => None,
            Some(0) => {
                self.cursor = None;
                Some(std::mem::take(&mut self.stashed))
            }
            Some(index) => {
                self.cursor = Some(index - 1);
                self.entries.get(index - 1).cloned()
            }
        }
    }

    /// Leave browsing without moving the draft — any edit does this, so a
    /// recalled prompt the human changed stays changed.
    pub fn release(&mut self) {
        self.cursor = None;
        self.stashed.clear();
    }

    fn sanitised(mut self) -> Self {
        self.entries.retain(|entry| !entry.trim().is_empty());
        self.entries.truncate(CAP);
        self
    }
}

fn path(home: &Path) -> PathBuf {
    home.join(FILE)
}

#[cfg(test)]
mod tests {
    use super::{History, CAP};

    fn history_of(prompts: &[&str]) -> History {
        let mut history = History::default();
        for prompt in prompts {
            history.record(prompt);
        }
        history
    }

    #[test]
    fn walking_back_and_forward_restores_the_draft_that_was_interrupted() {
        let mut history = history_of(&["first", "second"]);
        assert_eq!(history.older("half-typed"), Some("second".into()));
        assert_eq!(history.older("half-typed"), Some("first".into()));
        // Nothing older: the draft must not be blanked.
        assert_eq!(history.older("half-typed"), None);
        assert_eq!(history.newer(), Some("second".into()));
        assert_eq!(history.newer(), Some("half-typed".into()));
        assert_eq!(history.newer(), None);
    }

    #[test]
    fn a_repeated_prompt_does_not_push_the_rest_out_of_reach() {
        let mut history = history_of(&["build", "test", "test"]);
        assert_eq!(history.len(), 2);
        assert_eq!(history.older(""), Some("test".into()));
        assert_eq!(history.older(""), Some("build".into()));
    }

    #[test]
    fn recording_leaves_browsing_so_the_next_recall_starts_fresh() {
        let mut history = history_of(&["one"]);
        history.older("draft");
        history.record("two");
        assert_eq!(history.older("new draft"), Some("two".into()));
        assert_eq!(history.newer(), Some("new draft".into()));
    }

    #[test]
    fn the_ring_is_bounded_and_survives_a_damaged_file() {
        let mut history = History::default();
        for i in 0..CAP + 20 {
            history.record(&format!("prompt {i}"));
        }
        assert_eq!(history.len(), CAP);

        let home = tempfile::tempdir().expect("tempdir");
        std::fs::write(home.path().join("tui-history.json"), "{not json").expect("write");
        assert_eq!(History::load(home.path()).len(), 0);
    }

    #[test]
    fn entries_round_trip_through_the_home_but_browsing_state_does_not() {
        let home = tempfile::tempdir().expect("tempdir");
        let mut history = history_of(&["alpha", "beta"]);
        history.older("draft");
        history.save(home.path());

        let mut reloaded = History::load(home.path());
        assert_eq!(reloaded.len(), 2);
        // A fresh launch starts on the live draft, not mid-browse.
        assert_eq!(reloaded.newer(), None);
        assert_eq!(reloaded.older(""), Some("beta".into()));
    }
}
