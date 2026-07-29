//! Durable trust grants: how much authority a project gets, and for how long.
//!
//! ADR-0044 decision 5 draws a line this module makes structural: *project
//! scope is not project trust*. `project_authority` answers "where may work
//! happen" and containment depends on it. This answers "how much may happen
//! there", which is a different question, decided by a different person at a
//! different time, and revoked independently.
//!
//! They live in separate files for that reason, and for a duller one: adding
//! trust must not be able to corrupt or version-bump the scope document that
//! containment reads.
//!
//! Before this, an autonomy profile was chosen per turn and died with it
//! (`optimus-host/src/chat.rs`). So routine engineering work re-asked for the
//! same authority every time, and nothing recorded that a human had ever
//! decided anything. A grant here survives restarts and says who decided,
//! when, and until when.
//!
//! **A grant can never be `UnrestrictedHost.`** Host-wide authority is a
//! break-glass decision taken in the moment, with the person present. Making
//! it durable would turn one impatient click into a standing permission, which
//! is the failure mode ADR-0044 exists to prevent. The store refuses to
//! persist it rather than trusting callers to remember.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use optimus_graph::AutonomyProfile;
use serde::{Deserialize, Serialize};

use crate::credential::{atomic_write_user_only, harden_user_only, verify_user_only};
use crate::{KernelError, Result};

pub const PROJECT_TRUST_VERSION: u32 = 1;
const TRUST_FILE: &str = "project-trust.json";
const TRUST_LOCK_FILE: &str = "project-trust.lock";

/// A human's standing decision about one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectTrustGrant {
    pub project_id: String,
    pub profile: AutonomyProfile,
    pub granted_unix: u64,
    /// `None` means "until revoked". An expiry that has passed reads as no
    /// grant at all — never as the profile it used to carry.
    pub expires_unix: Option<u64>,
    /// What the grant was for, kept so a later reader can tell whether it still
    /// applies. Not load-bearing.
    pub note: String,
}

impl ProjectTrustGrant {
    #[must_use]
    pub fn is_live_at(&self, now: u64) -> bool {
        self.expires_unix.is_none_or(|expiry| expiry > now)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrustDocument {
    version: u32,
    #[serde(default)]
    grants: BTreeMap<String, ProjectTrustGrant>,
}

impl Default for TrustDocument {
    fn default() -> Self {
        Self {
            version: PROJECT_TRUST_VERSION,
            grants: BTreeMap::new(),
        }
    }
}

/// Reads and writes `{home}/project-trust.json`, outside every project tree.
#[derive(Debug, Clone)]
pub struct ProjectTrustStore {
    home: PathBuf,
}

impl ProjectTrustStore {
    pub fn open(home: impl AsRef<Path>) -> Result<Self> {
        let home = home.as_ref().to_path_buf();
        std::fs::create_dir_all(&home)?;
        Ok(Self { home })
    }

    /// Record that `project_id` may run at `profile` until it expires.
    ///
    /// Replaces any existing grant, so re-granting at a lower profile is a
    /// narrowing rather than an accumulation.
    ///
    /// # Errors
    /// When the profile is `UnrestrictedHost`, when the id is malformed, or
    /// when the file cannot be written.
    pub fn grant(
        &self,
        project_id: &str,
        profile: AutonomyProfile,
        ttl_seconds: Option<u64>,
        note: impl Into<String>,
    ) -> Result<ProjectTrustGrant> {
        self.grant_at(project_id, profile, ttl_seconds, note, now_unix()?)
    }

    /// Remove a project's grant. Returns whether there was one.
    ///
    /// # Errors
    /// When the id is malformed or the file cannot be written.
    pub fn revoke(&self, project_id: &str) -> Result<bool> {
        validate_project_id(project_id)?;
        let _lock = self.lock_mutation()?;
        let mut document = self.load()?;
        let removed = document.grants.remove(project_id).is_some();
        if removed {
            self.save(&document)?;
        }
        Ok(removed)
    }

    /// The profile this project may currently run at, if any.
    ///
    /// `None` covers both "never granted" and "granted, now expired". A caller
    /// that gets `None` must fall back to whatever it would have done without
    /// a grant — never to the last profile it saw.
    ///
    /// # Errors
    /// When the id is malformed or the file cannot be read.
    pub fn effective_profile(&self, project_id: &str) -> Result<Option<AutonomyProfile>> {
        Ok(self.grant_for(project_id)?.map(|grant| grant.profile))
    }

    /// The live grant for a project, expiry already applied.
    ///
    /// # Errors
    /// When the id is malformed or the file cannot be read.
    pub fn grant_for(&self, project_id: &str) -> Result<Option<ProjectTrustGrant>> {
        validate_project_id(project_id)?;
        let now = now_unix()?;
        Ok(self
            .load()?
            .grants
            .remove(project_id)
            .filter(|grant| grant.is_live_at(now)))
    }

    /// Every live grant, expired ones filtered out.
    ///
    /// # Errors
    /// When the file cannot be read.
    pub fn list(&self) -> Result<Vec<ProjectTrustGrant>> {
        let now = now_unix()?;
        Ok(self
            .load()?
            .grants
            .into_values()
            .filter(|grant| grant.is_live_at(now))
            .collect())
    }

    fn grant_at(
        &self,
        project_id: &str,
        profile: AutonomyProfile,
        ttl_seconds: Option<u64>,
        note: impl Into<String>,
        now: u64,
    ) -> Result<ProjectTrustGrant> {
        validate_project_id(project_id)?;
        if matches!(profile, AutonomyProfile::UnrestrictedHost) {
            return Err(KernelError::Tool(
                "unrestricted_host cannot be granted durably; it is a per-turn decision".into(),
            ));
        }
        let expires_unix =
            match ttl_seconds {
                None => None,
                Some(ttl) => Some(now.checked_add(ttl).ok_or_else(|| {
                    KernelError::Tool("project trust grant expiry overflow".into())
                })?),
            };
        let grant = ProjectTrustGrant {
            project_id: project_id.to_string(),
            profile,
            granted_unix: now,
            expires_unix,
            note: note.into(),
        };

        let _lock = self.lock_mutation()?;
        let mut document = self.load()?;
        // Drop anything already expired while the file is open, so a store
        // nobody revokes does not grow without bound.
        document.grants.retain(|_, grant| grant.is_live_at(now));
        document
            .grants
            .insert(project_id.to_string(), grant.clone());
        self.save(&document)?;
        Ok(grant)
    }

    fn path(&self) -> PathBuf {
        self.home.join(TRUST_FILE)
    }

    fn lock_mutation(&self) -> Result<File> {
        let path = self.home.join(TRUST_LOCK_FILE);
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;

            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let file = options.open(&path)?;
        if !file.metadata()?.is_file() {
            return Err(KernelError::Tool(
                "project trust lock is not a regular file".into(),
            ));
        }
        harden_user_only(&path)?;
        verify_user_only(&path)?;
        file.lock_exclusive()?;
        Ok(file)
    }

    fn load(&self) -> Result<TrustDocument> {
        let path = self.path();
        if !path.exists() {
            return Ok(TrustDocument::default());
        }
        verify_user_only(&path)?;
        let bytes = std::fs::read(&path)?;
        let document: TrustDocument = serde_json::from_slice(&bytes).map_err(|error| {
            KernelError::Tool(format!("project trust store unreadable: {error}"))
        })?;
        if document.version != PROJECT_TRUST_VERSION {
            return Err(KernelError::Tool(format!(
                "project trust store version {} is not {PROJECT_TRUST_VERSION}",
                document.version
            )));
        }
        Ok(document)
    }

    fn save(&self, document: &TrustDocument) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(document).map_err(|error| {
            KernelError::Tool(format!("project trust store unwritable: {error}"))
        })?;
        atomic_write_user_only(&self.path(), &bytes)
    }
}

fn validate_project_id(project_id: &str) -> Result<()> {
    if project_id.is_empty() || project_id.len() > 128 {
        return Err(KernelError::Tool(
            "project id length is out of range".into(),
        ));
    }
    if !project_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(KernelError::Tool(
            "project id may only contain ascii alphanumerics, '-', '_' and '.'".into(),
        ));
    }
    Ok(())
}

fn now_unix() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| KernelError::Tool(format!("system clock before epoch: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, ProjectTrustStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ProjectTrustStore::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn a_grant_survives_a_new_store_over_the_same_home() {
        let (dir, store) = store();
        store
            .grant("optimus", AutonomyProfile::Standard, None, "routine work")
            .unwrap();

        let reopened = ProjectTrustStore::open(dir.path()).unwrap();
        assert_eq!(
            reopened.effective_profile("optimus").unwrap(),
            Some(AutonomyProfile::Standard)
        );
    }

    #[test]
    fn unrestricted_host_cannot_be_made_durable() {
        let (_dir, store) = store();
        let refused = store
            .grant("optimus", AutonomyProfile::UnrestrictedHost, None, "yolo")
            .unwrap_err()
            .to_string();
        assert!(refused.contains("per-turn"), "got: {refused}");
        assert_eq!(store.effective_profile("optimus").unwrap(), None);
    }

    #[test]
    fn an_expired_grant_reads_as_no_grant_not_as_its_profile() {
        let (_dir, store) = store();
        let now = now_unix().unwrap();
        // Granted an hour ago with a one-second life.
        store
            .grant_at(
                "optimus",
                AutonomyProfile::FullProject,
                Some(1),
                "short lease",
                now - 3600,
            )
            .unwrap();

        assert_eq!(store.effective_profile("optimus").unwrap(), None);
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn revoking_is_explicit_and_reports_whether_it_did_anything() {
        let (_dir, store) = store();
        store
            .grant("optimus", AutonomyProfile::Standard, None, "routine")
            .unwrap();

        assert!(store.revoke("optimus").unwrap(), "first revoke removes it");
        assert!(!store.revoke("optimus").unwrap(), "second finds nothing");
        assert_eq!(store.effective_profile("optimus").unwrap(), None);
    }

    #[test]
    fn re_granting_narrows_rather_than_accumulating() {
        let (_dir, store) = store();
        store
            .grant("optimus", AutonomyProfile::FullProject, None, "wide")
            .unwrap();
        store
            .grant("optimus", AutonomyProfile::ReadOnly, None, "narrowed")
            .unwrap();

        assert_eq!(
            store.effective_profile("optimus").unwrap(),
            Some(AutonomyProfile::ReadOnly)
        );
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn a_grant_records_who_decided_and_when() {
        let (_dir, store) = store();
        let before = now_unix().unwrap();
        let grant = store
            .grant(
                "optimus",
                AutonomyProfile::Standard,
                Some(86_400),
                "approved for the P40 engineering loop",
            )
            .unwrap();

        assert!(grant.granted_unix >= before);
        assert_eq!(grant.expires_unix, Some(grant.granted_unix + 86_400));
        assert!(grant.note.contains("P40"));
    }

    #[test]
    fn a_malformed_project_id_is_refused_before_anything_is_written() {
        let (dir, store) = store();
        assert!(store
            .grant("../escape", AutonomyProfile::Standard, None, "no")
            .is_err());
        assert!(
            !dir.path().join(TRUST_FILE).exists(),
            "a refused grant leaves no file behind"
        );
    }
}
