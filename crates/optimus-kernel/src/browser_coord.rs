//! Host-owned browser coordination bus (ADR-0040 SharedBrowserContract).
//!
//! Two trust domains publish navigation events. This is **not** a shared CDP
//! session, cookie jar, or storage partition.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const BROWSER_COORD_SCHEMA_VERSION: u16 = 1;
const MAX_EVENTS: usize = 64;

#[derive(Debug, Error)]
pub enum CoordError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, CoordError>;

/// Trust domain for a browser surface (never merged by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserTrustDomain {
    UserPreview,
    AgentEffector,
}

impl BrowserTrustDomain {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserPreview => "user_preview",
            Self::AgentEffector => "agent_effector",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordEventKind {
    Navigated,
    Snapshot,
    AnnotationCaptured,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordEvent {
    pub schema_version: u16,
    pub event_id: String,
    pub domain: BrowserTrustDomain,
    /// Stable per-domain session token — never shared across domains.
    pub domain_session_id: String,
    pub kind: CoordEventKind,
    pub url: String,
    pub title: Option<String>,
    pub unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordSnapshot {
    pub schema_version: u16,
    /// Explicit non-claims for product/ledger honesty.
    pub shared_cookie_jar: bool,
    pub shared_storage_partition: bool,
    pub shared_cdp_target: bool,
    pub preview_session_id: String,
    pub agent_session_id: String,
    pub last_preview_url: Option<String>,
    pub last_agent_url: Option<String>,
    pub last_preview_title: Option<String>,
    pub last_agent_title: Option<String>,
    pub events: Vec<CoordEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoordStoreFile {
    schema_version: u16,
    preview_session_id: String,
    agent_session_id: String,
    events: Vec<CoordEvent>,
}

/// Durable-enough host bus under `{home}/browser_coord.json`.
#[derive(Debug)]
pub struct BrowserCoordBus {
    path: PathBuf,
    store: CoordStoreFile,
}

impl BrowserCoordBus {
    pub fn open(home: impl AsRef<Path>) -> Result<Self> {
        let path = home.as_ref().join("browser_coord.json");
        let store = if path.exists() {
            let raw = fs::read_to_string(&path)?;
            serde_json::from_str(&raw)?
        } else {
            CoordStoreFile {
                schema_version: BROWSER_COORD_SCHEMA_VERSION,
                preview_session_id: Uuid::new_v4().to_string(),
                agent_session_id: Uuid::new_v4().to_string(),
                events: Vec::new(),
            }
        };
        // Invariant: domains must never share session identity.
        if store.preview_session_id == store.agent_session_id {
            return Err(CoordError::Msg(
                "invalid coord store: preview and agent session ids must differ".into(),
            ));
        }
        Ok(Self { path, store })
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_json::to_string_pretty(&self.store)?)?;
        Ok(())
    }

    pub fn record(
        &mut self,
        domain: BrowserTrustDomain,
        kind: CoordEventKind,
        url: impl Into<String>,
        title: Option<String>,
    ) -> Result<CoordEvent> {
        let url = url.into().trim().to_string();
        if url.is_empty() {
            return Err(CoordError::Msg("url must be non-empty".into()));
        }
        let domain_session_id = match domain {
            BrowserTrustDomain::UserPreview => self.store.preview_session_id.clone(),
            BrowserTrustDomain::AgentEffector => self.store.agent_session_id.clone(),
        };
        let event = CoordEvent {
            schema_version: BROWSER_COORD_SCHEMA_VERSION,
            event_id: Uuid::new_v4().to_string(),
            domain,
            domain_session_id,
            kind,
            url,
            title,
            unix_ms: now_ms(),
        };
        self.store.events.push(event.clone());
        if self.store.events.len() > MAX_EVENTS {
            let drain = self.store.events.len() - MAX_EVENTS;
            self.store.events.drain(0..drain);
        }
        self.save()?;
        Ok(event)
    }

    pub fn record_agent_navigate(
        &mut self,
        url: &str,
        title: Option<String>,
    ) -> Result<CoordEvent> {
        self.record(
            BrowserTrustDomain::AgentEffector,
            CoordEventKind::Navigated,
            url,
            title,
        )
    }

    pub fn record_preview_navigate(
        &mut self,
        url: &str,
        title: Option<String>,
    ) -> Result<CoordEvent> {
        self.record(
            BrowserTrustDomain::UserPreview,
            CoordEventKind::Navigated,
            url,
            title,
        )
    }

    pub fn snapshot(&self) -> CoordSnapshot {
        let last_preview = self
            .store
            .events
            .iter()
            .rev()
            .find(|e| e.domain == BrowserTrustDomain::UserPreview);
        let last_agent = self
            .store
            .events
            .iter()
            .rev()
            .find(|e| e.domain == BrowserTrustDomain::AgentEffector);
        CoordSnapshot {
            schema_version: BROWSER_COORD_SCHEMA_VERSION,
            shared_cookie_jar: false,
            shared_storage_partition: false,
            shared_cdp_target: false,
            preview_session_id: self.store.preview_session_id.clone(),
            agent_session_id: self.store.agent_session_id.clone(),
            last_preview_url: last_preview.map(|e| e.url.clone()),
            last_agent_url: last_agent.map(|e| e.url.clone()),
            last_preview_title: last_preview.and_then(|e| e.title.clone()),
            last_agent_title: last_agent.and_then(|e| e.title.clone()),
            events: self.store.events.clone(),
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn domains_never_share_session_or_cookie_claims() {
        let dir = tempdir().unwrap();
        let mut bus = BrowserCoordBus::open(dir.path()).unwrap();
        bus.record_preview_navigate("https://example.com/preview", Some("P".into()))
            .unwrap();
        bus.record_agent_navigate("https://example.com/agent", Some("A".into()))
            .unwrap();
        let snap = bus.snapshot();
        assert_ne!(snap.preview_session_id, snap.agent_session_id);
        assert!(!snap.shared_cookie_jar);
        assert!(!snap.shared_storage_partition);
        assert!(!snap.shared_cdp_target);
        assert_eq!(
            snap.last_preview_url.as_deref(),
            Some("https://example.com/preview")
        );
        assert_eq!(
            snap.last_agent_url.as_deref(),
            Some("https://example.com/agent")
        );
        // Events retain distinct domain session ids
        let p = snap
            .events
            .iter()
            .find(|e| e.domain == BrowserTrustDomain::UserPreview)
            .unwrap();
        let a = snap
            .events
            .iter()
            .find(|e| e.domain == BrowserTrustDomain::AgentEffector)
            .unwrap();
        assert_ne!(p.domain_session_id, a.domain_session_id);
    }

    #[test]
    fn persists_across_reopen() {
        let dir = tempdir().unwrap();
        {
            let mut bus = BrowserCoordBus::open(dir.path()).unwrap();
            bus.record_agent_navigate("https://example.com/x", None)
                .unwrap();
        }
        let bus = BrowserCoordBus::open(dir.path()).unwrap();
        assert_eq!(
            bus.snapshot().last_agent_url.as_deref(),
            Some("https://example.com/x")
        );
    }

    #[test]
    fn rejects_empty_url() {
        let dir = tempdir().unwrap();
        let mut bus = BrowserCoordBus::open(dir.path()).unwrap();
        assert!(bus.record_preview_navigate("  ", None).is_err());
    }
}
