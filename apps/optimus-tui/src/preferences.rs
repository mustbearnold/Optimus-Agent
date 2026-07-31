//! What the terminal face remembers between runs.
//!
//! Only the choices a user makes *about* the agent — which provider, which
//! model, how hard to think. Not transcripts, not sessions: those are durable
//! kernel state and already have a home. This file exists so that picking a
//! provider once does not become picking it every launch.
//!
//! Every failure here is swallowed deliberately. A preferences file is a
//! convenience, and a convenience that can refuse to start the program is a
//! defect — a missing, unreadable or malformed file all mean "use defaults".

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Let routing choose the best available provider for a first run. In a fresh
/// or disconnected home this deterministically resolves to offline, while an
/// explicit provider choice remains sticky.
pub const DEFAULT_PROVIDER: &str = "auto";

const FILE: &str = "tui-preferences.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub provider: String,
    /// Absent means "whatever the provider considers its default", which is not
    /// the same as any particular model id and must not be stored as one.
    pub model: Option<String>,
    /// Absent means the backend's own default effort, not "off" — `/thinking
    /// off` records `None` and so does never having chosen.
    pub thinking: Option<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            provider: DEFAULT_PROVIDER.to_string(),
            model: None,
            thinking: None,
        }
    }
}

impl Preferences {
    /// Read remembered choices, falling back to defaults for anything absent.
    pub fn load(home: &Path) -> Self {
        std::fs::read_to_string(path(home))
            .ok()
            .and_then(|text| serde_json::from_str::<Self>(&text).ok())
            .map(Self::sanitised)
            .unwrap_or_default()
    }

    /// Write the current choices, best effort.
    ///
    /// A caller cannot do anything useful with a failure — the user's actual
    /// request (switch provider) has already succeeded — so it is not reported.
    pub fn save(&self, home: &Path) {
        let Ok(text) = serde_json::to_string_pretty(self) else {
            return;
        };
        let _ = std::fs::create_dir_all(home);
        let _ = std::fs::write(path(home), text);
    }

    /// Repair anything a hand-edited or truncated file could contain.
    ///
    /// An empty provider would otherwise be sent on the wire as a real choice
    /// and rejected by the host, turning a bad file into a broken session.
    fn sanitised(mut self) -> Self {
        if self.provider.trim().is_empty() {
            self.provider = DEFAULT_PROVIDER.to_string();
        }
        if self.provider.eq_ignore_ascii_case("openai_compat") {
            self.provider = "open-ai-compat".to_string();
        }
        self.model = self
            .model
            .filter(|value| !value.trim().is_empty() && !value.trim().eq_ignore_ascii_case("auto"));
        if self.provider.eq_ignore_ascii_case("auto") {
            self.model = None;
        }
        self.thinking = self.thinking.filter(|value| !value.trim().is_empty());
        self
    }
}

fn path(home: &Path) -> PathBuf {
    home.join(FILE)
}

#[cfg(test)]
mod tests {
    use super::{Preferences, DEFAULT_PROVIDER};
    use tempfile::tempdir;

    #[test]
    fn a_first_run_delegates_provider_and_model_selection_to_auto() {
        let dir = tempdir().unwrap();
        let prefs = Preferences::load(dir.path());
        assert_eq!(prefs.provider, DEFAULT_PROVIDER);
        assert_eq!(prefs.model, None);
        assert_eq!(prefs.thinking, None);
    }

    #[test]
    fn a_choice_survives_the_next_launch() {
        let dir = tempdir().unwrap();
        Preferences {
            provider: "codex".into(),
            model: Some("gpt-5".into()),
            thinking: Some("high".into()),
        }
        .save(dir.path());

        let reloaded = Preferences::load(dir.path());
        assert_eq!(reloaded.provider, "codex");
        assert_eq!(reloaded.model.as_deref(), Some("gpt-5"));
        assert_eq!(reloaded.thinking.as_deref(), Some("high"));
    }

    #[test]
    fn a_malformed_file_starts_the_program_anyway() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("tui-preferences.json"), "{not json").unwrap();
        assert_eq!(Preferences::load(dir.path()), Preferences::default());
    }

    #[test]
    fn a_partial_file_keeps_the_fields_it_does_have() {
        // Forward compatibility both ways: an older file missing newer keys
        // must not discard the key it does carry.
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("tui-preferences.json"),
            r#"{"provider":"codex"}"#,
        )
        .unwrap();
        let prefs = Preferences::load(dir.path());
        assert_eq!(prefs.provider, "codex");
        assert_eq!(prefs.model, None);
    }

    #[test]
    fn a_blank_provider_is_repaired_rather_than_sent_on_the_wire() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("tui-preferences.json"),
            r#"{"provider":"  ","model":"","thinking":" "}"#,
        )
        .unwrap();
        let prefs = Preferences::load(dir.path());
        assert_eq!(prefs.provider, DEFAULT_PROVIDER);
        assert_eq!(prefs.model, None, "an empty model is not a model");
        assert_eq!(prefs.thinking, None);
    }

    #[test]
    fn an_auto_model_is_absence_not_a_literal_model_override() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("tui-preferences.json"),
            r#"{"provider":"codex","model":"Auto"}"#,
        )
        .unwrap();

        let prefs = Preferences::load(dir.path());
        assert_eq!(prefs.provider, "codex");
        assert_eq!(prefs.model, None);
    }

    #[test]
    fn automatic_provider_drops_a_stale_explicit_model() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("tui-preferences.json"),
            r#"{"provider":"auto","model":"gpt-5.6-sol"}"#,
        )
        .unwrap();

        let prefs = Preferences::load(dir.path());
        assert_eq!(prefs.provider, "auto");
        assert_eq!(prefs.model, None);
    }

    #[test]
    fn legacy_openai_provider_spelling_migrates_to_the_catalog_wire_id() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("tui-preferences.json"),
            r#"{"provider":"openai_compat","model":"gpt-4.1"}"#,
        )
        .unwrap();

        let prefs = Preferences::load(dir.path());
        assert_eq!(prefs.provider, "open-ai-compat");
        assert_eq!(prefs.model.as_deref(), Some("gpt-4.1"));
    }

    #[test]
    fn saving_into_a_home_that_does_not_exist_yet_still_works() {
        let dir = tempdir().unwrap();
        let home = dir.path().join("fresh");
        Preferences {
            provider: "codex".into(),
            ..Preferences::default()
        }
        .save(&home);
        assert_eq!(Preferences::load(&home).provider, "codex");
    }
}
