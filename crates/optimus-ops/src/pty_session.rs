//! Interactive PTY session scaffold (S7.6) — Linux-first, fail-closed elsewhere.
//!
//! Multi-tab product UI is residual; this module provides durable session
//! identity, create/list/close, and platform-gated open.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PtyError {
    #[error("pty not supported on this platform")]
    Unsupported,
    #[error("session not found: {0}")]
    NotFound(String),
    #[error("max tabs exceeded: {0}")]
    MaxTabs(usize),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, PtyError>;

pub const DEFAULT_MAX_TABS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PtyTab {
    pub id: Uuid,
    pub title: String,
    pub created_unix: u64,
    pub platform: String,
    pub status: String,
}

#[derive(Debug, Default)]
struct Inner {
    tabs: BTreeMap<Uuid, PtyTab>,
}

#[derive(Debug, Clone)]
pub struct PtySessionStore {
    home: PathBuf,
    max_tabs: usize,
    inner: Arc<Mutex<Inner>>,
}

impl PtySessionStore {
    pub fn open(home: impl AsRef<Path>) -> Result<Self> {
        let home = home.as_ref().join("pty");
        std::fs::create_dir_all(&home).map_err(|e| PtyError::Msg(e.to_string()))?;
        Ok(Self {
            home,
            max_tabs: DEFAULT_MAX_TABS,
            inner: Arc::new(Mutex::new(Inner::default())),
        })
    }

    pub fn platform_supported() -> bool {
        cfg!(target_os = "linux")
    }

    pub fn create_tab(&self, title: &str) -> Result<PtyTab> {
        if !Self::platform_supported() {
            return Err(PtyError::Unsupported);
        }
        let mut g = self
            .inner
            .lock()
            .map_err(|e| PtyError::Msg(e.to_string()))?;
        if g.tabs.len() >= self.max_tabs {
            return Err(PtyError::MaxTabs(self.max_tabs));
        }
        let tab = PtyTab {
            id: Uuid::new_v4(),
            title: title.chars().take(80).collect(),
            created_unix: now_unix(),
            platform: "linux".into(),
            status: "open".into(),
        };
        // Persist a marker file (no real PTY master in scaffold — open is residual).
        let path = self.home.join(format!("{}.json", tab.id));
        let raw = serde_json::to_string_pretty(&tab).map_err(|e| PtyError::Msg(e.to_string()))?;
        std::fs::write(path, raw).map_err(|e| PtyError::Msg(e.to_string()))?;
        g.tabs.insert(tab.id, tab.clone());
        Ok(tab)
    }

    pub fn list(&self) -> Result<Vec<PtyTab>> {
        let g = self
            .inner
            .lock()
            .map_err(|e| PtyError::Msg(e.to_string()))?;
        let mut v: Vec<_> = g.tabs.values().cloned().collect();
        v.sort_by_key(|t| t.created_unix);
        Ok(v)
    }

    pub fn close(&self, id: Uuid) -> Result<bool> {
        let mut g = self
            .inner
            .lock()
            .map_err(|e| PtyError::Msg(e.to_string()))?;
        let removed = g.tabs.remove(&id).is_some();
        let path = self.home.join(format!("{id}.json"));
        let _ = std::fs::remove_file(path);
        Ok(removed)
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn linux_tabs_or_unsupported() {
        let dir = tempdir().unwrap();
        let store = PtySessionStore::open(dir.path()).unwrap();
        match store.create_tab("shell-1") {
            Ok(tab) => {
                assert_eq!(tab.platform, "linux");
                assert_eq!(store.list().unwrap().len(), 1);
                assert!(store.close(tab.id).unwrap());
            }
            Err(PtyError::Unsupported) => {
                assert!(!PtySessionStore::platform_supported());
            }
            Err(e) => panic!("unexpected {e}"),
        }
    }

    #[test]
    fn max_tabs_enforced_when_supported() {
        if !PtySessionStore::platform_supported() {
            return;
        }
        let dir = tempdir().unwrap();
        let store = PtySessionStore::open(dir.path()).unwrap();
        for i in 0..DEFAULT_MAX_TABS {
            store.create_tab(&format!("t{i}")).unwrap();
        }
        assert!(matches!(
            store.create_tab("overflow"),
            Err(PtyError::MaxTabs(_))
        ));
    }
}
