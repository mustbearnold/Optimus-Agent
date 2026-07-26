//! Hermes session / skills / memory importers (S7.8–S7.9).
//!
//! Read-only import of Hermes-shaped JSON fixtures into Optimus home layout.
//! Never mutates Hermes source trees.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, ImportError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HermesSessionFixture {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub messages: Vec<HermesMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HermesMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HermesSkillFixture {
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HermesMemoryFixture {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ImportReport {
    pub sessions: usize,
    pub skills: usize,
    pub memory_claims: usize,
    pub notes: Vec<String>,
}

/// Import Hermes session fixtures from a directory of `*.session.json`.
pub fn import_sessions(
    home: impl AsRef<Path>,
    source_dir: impl AsRef<Path>,
) -> Result<ImportReport> {
    let home = home.as_ref();
    let out_dir = home.join("imports").join("hermes").join("sessions");
    fs::create_dir_all(&out_dir)?;
    let mut report = ImportReport::default();
    let source = source_dir.as_ref();
    if !source.is_dir() {
        return Err(ImportError::Msg(format!(
            "source not a directory: {}",
            source.display()
        )));
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path)?;
        let Ok(fixture) = serde_json::from_str::<HermesSessionFixture>(&raw) else {
            continue; // skill/memory fixtures share the directory in tests
        };
        let id = if Uuid::parse_str(&fixture.id).is_ok() {
            fixture.id.clone()
        } else {
            Uuid::new_v4().to_string()
        };
        let dest = out_dir.join(format!("{id}.json"));
        let normalized = serde_json::json!({
            "id": id,
            "title": fixture.title,
            "source": "hermes",
            "messages": fixture.messages,
        });
        fs::write(dest, serde_json::to_string_pretty(&normalized)?)?;
        report.sessions += 1;
    }
    report
        .notes
        .push("sessions imported as Optimus import records (not live session.db merge)".into());
    Ok(report)
}

/// Import skill fixtures `*.skill.json` into `{home}/imports/hermes/skills`.
pub fn import_skills(home: impl AsRef<Path>, source_dir: impl AsRef<Path>) -> Result<ImportReport> {
    let home = home.as_ref();
    let out_dir = home.join("imports").join("hermes").join("skills");
    fs::create_dir_all(&out_dir)?;
    let mut report = ImportReport::default();
    for entry in fs::read_dir(source_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path)?;
        let Ok(fixture) = serde_json::from_str::<HermesSkillFixture>(&raw) else {
            continue;
        };
        if fixture.name.trim().is_empty() || fixture.body.trim().is_empty() {
            continue;
        }
        let dest = out_dir.join(format!("{}.json", sanitize(&fixture.name)));
        fs::write(dest, serde_json::to_string_pretty(&fixture)?)?;
        report.skills += 1;
    }
    report
        .notes
        .push("skills staged under imports/; promote via skills console/API separately".into());
    Ok(report)
}

/// Import memory claim fixtures into `{home}/imports/hermes/memory`.
pub fn import_memory(home: impl AsRef<Path>, source_dir: impl AsRef<Path>) -> Result<ImportReport> {
    let home = home.as_ref();
    let out_dir = home.join("imports").join("hermes").join("memory");
    fs::create_dir_all(&out_dir)?;
    let mut report = ImportReport::default();
    for entry in fs::read_dir(source_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path)?;
        // Accept single claim or array; skip non-memory fixtures.
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let claims: Vec<HermesMemoryFixture> = if value.is_array() {
            match serde_json::from_value(value) {
                Ok(c) => c,
                Err(_) => continue,
            }
        } else {
            match serde_json::from_value(value) {
                Ok(c) => vec![c],
                Err(_) => continue,
            }
        };
        for claim in claims {
            let dest = out_dir.join(format!("{}.json", Uuid::new_v4()));
            fs::write(dest, serde_json::to_string_pretty(&claim)?)?;
            report.memory_claims += 1;
        }
    }
    report
        .notes
        .push("memory claims staged; apply via memory console with evidence fence".into());
    Ok(report)
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect()
}

/// Write a minimal fixture pack for tests under `dir`.
pub fn write_test_fixtures(dir: impl AsRef<Path>) -> Result<PathBuf> {
    let dir = dir.as_ref();
    fs::create_dir_all(dir)?;
    fs::write(
        dir.join("demo.session.json"),
        serde_json::to_string_pretty(&HermesSessionFixture {
            id: Uuid::new_v4().to_string(),
            title: "Hermes demo".into(),
            messages: vec![
                HermesMessage {
                    role: "user".into(),
                    content: "hello".into(),
                },
                HermesMessage {
                    role: "assistant".into(),
                    content: "hi".into(),
                },
            ],
        })?,
    )?;
    fs::write(
        dir.join("demo.skill.json"),
        serde_json::to_string_pretty(&HermesSkillFixture {
            name: "demo-skill".into(),
            body: "do the thing".into(),
        })?,
    )?;
    fs::write(
        dir.join("demo.memory.json"),
        serde_json::to_string_pretty(&HermesMemoryFixture {
            subject: "user".into(),
            predicate: "likes".into(),
            object: "tea".into(),
        })?,
    )?;
    Ok(dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn import_session_skill_memory_fixtures() {
        let src = tempdir().unwrap();
        let home = tempdir().unwrap();
        write_test_fixtures(src.path()).unwrap();
        // sessions named *.json — skill and memory also match; importers are selective by schema
        let sessions = import_sessions(home.path(), src.path()).unwrap();
        assert!(sessions.sessions >= 1);
        let skills = import_skills(home.path(), src.path()).unwrap();
        assert!(skills.skills >= 1);
        let mem = import_memory(home.path(), src.path()).unwrap();
        assert!(mem.memory_claims >= 1);
        assert!(
            home.path()
                .join("imports/hermes/sessions")
                .read_dir()
                .unwrap()
                .count()
                >= 1
        );
    }
}
