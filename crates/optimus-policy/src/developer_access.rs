//! Explicit local authority for Optimus self-development.
//!
//! This is deliberately a data-and-predicate module. It does not touch the
//! filesystem, spawn processes, or infer authority from a UI label. The host
//! validates and persists a grant; the capability broker evaluates it for each
//! exact effect.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{ActionTarget, CapabilityId, Reversibility};

/// Bump when the confirmation wording or meaning of a grant changes.
pub const DEVELOPER_ACCESS_CONFIRMATION_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeveloperScope {
    /// The normal choice: one repository or project root and its descendants.
    SelectedRepository {
        root: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root_hash: Option<String>,
    },
    /// Multiple explicitly selected roots. Each root is checked independently.
    SelectedDirectories { roots: Vec<String> },
    /// Advanced opt-in. The broker still applies capability toggles and pause
    /// policy; this scope only widens the path predicate.
    EntireLocalMachine,
}

impl Default for DeveloperScope {
    fn default() -> Self {
        Self::SelectedRepository {
            root: String::new(),
            root_hash: None,
        }
    }
}

impl DeveloperScope {
    pub fn label(&self) -> &'static str {
        match self {
            Self::SelectedRepository { .. } => "Selected repository",
            Self::SelectedDirectories { .. } => "Selected directories",
            Self::EntireLocalMachine => "Entire local machine",
        }
    }

    pub fn roots(&self) -> Vec<PathBuf> {
        match self {
            Self::SelectedRepository { root, .. } => vec![PathBuf::from(root)],
            Self::SelectedDirectories { roots } => roots.iter().map(PathBuf::from).collect(),
            Self::EntireLocalMachine => Vec::new(),
        }
    }

    /// Validate the serialized scope before it becomes active authority.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::SelectedRepository { root, root_hash } => {
                validate_root(root, "repository root")?;
                if let Some(hash) = root_hash {
                    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                        return Err("repository root hash must be 64 hexadecimal characters".into());
                    }
                }
            }
            Self::SelectedDirectories { roots } => {
                if roots.is_empty() {
                    return Err("at least one directory must be selected".into());
                }
                if roots.len() > 64 {
                    return Err("no more than 64 directories may be selected".into());
                }
                for root in roots {
                    validate_root(root, "selected directory")?;
                }
            }
            Self::EntireLocalMachine => {}
        }
        Ok(())
    }

    pub fn allows_target(&self, target: &ActionTarget) -> bool {
        match self {
            Self::EntireLocalMachine => true,
            Self::SelectedRepository { root, root_hash } => {
                if let (Some(expected), Some(actual)) =
                    (root_hash.as_deref(), target.project_root_hash.as_deref())
                {
                    if expected == actual && target.absolute_path.is_none() {
                        return true;
                    }
                }
                target
                    .absolute_path
                    .as_deref()
                    .is_some_and(|path| path_within(path, root))
            }
            Self::SelectedDirectories { roots } => target
                .absolute_path
                .as_deref()
                .is_some_and(|path| roots.iter().any(|root| path_within(path, root))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeveloperCapabilities {
    #[serde(default = "enabled_by_default")]
    pub workspace_files: bool,
    #[serde(default = "enabled_by_default")]
    pub terminal_execution: bool,
    #[serde(default = "enabled_by_default")]
    pub process_management: bool,
    #[serde(default = "enabled_by_default")]
    pub package_installation: bool,
    #[serde(default = "enabled_by_default")]
    pub network_access: bool,
    #[serde(default)]
    pub external_services: bool,
    #[serde(default)]
    pub production_systems: bool,
    #[serde(default)]
    pub secrets: bool,
}

impl Default for DeveloperCapabilities {
    fn default() -> Self {
        Self {
            workspace_files: true,
            terminal_execution: true,
            process_management: true,
            package_installation: true,
            network_access: true,
            external_services: false,
            production_systems: false,
            secrets: false,
        }
    }
}

impl DeveloperCapabilities {
    pub fn allows(&self, capability: CapabilityId) -> bool {
        match capability {
            CapabilityId::FsProjectRead
            | CapabilityId::FsProjectWrite
            | CapabilityId::FsProjectRename
            | CapabilityId::FsProjectDelete
            | CapabilityId::GitLocalRead
            | CapabilityId::GitLocalWrite => self.workspace_files,
            CapabilityId::ProcessProjectExecute => {
                self.terminal_execution && self.process_management
            }
            CapabilityId::ProcessProjectServe => self.process_management,
            CapabilityId::PackageSync | CapabilityId::PackageAdd => {
                self.terminal_execution && self.package_installation
            }
            CapabilityId::NetworkPublicRead
            | CapabilityId::NetworkRegistryRead
            | CapabilityId::NetworkLocalhostOwned
            | CapabilityId::NetworkPrivate
            | CapabilityId::BrowserPublicRead
            | CapabilityId::BrowserLocalhostOwned => self.network_access,
            CapabilityId::CredentialUse | CapabilityId::CredentialReadRaw => self.secrets,
            CapabilityId::GitRemotePush
            | CapabilityId::GitRemotePullRequest
            | CapabilityId::ExternalSend
            | CapabilityId::ExternalPublish
            | CapabilityId::ExternalDeploy
            | CapabilityId::DataRemoteWrite
            | CapabilityId::DataRemoteDelete => self.external_services,
            CapabilityId::SystemModify => self.production_systems,
            // Spending is always a separate human decision, even in a local
            // development grant.
            CapabilityId::CommerceSpend => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeveloperAccessGrant {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub scope: DeveloperScope,
    #[serde(default)]
    pub capabilities: DeveloperCapabilities,
    #[serde(default = "enabled_by_default")]
    pub pause_before_destructive: bool,
    #[serde(default = "enabled_by_default")]
    pub checkpoint_on_mutation: bool,
    #[serde(default)]
    pub issued_unix: u64,
    #[serde(default)]
    pub confirmation_version: u32,
}

impl Default for DeveloperAccessGrant {
    fn default() -> Self {
        Self {
            enabled: false,
            scope: DeveloperScope::default(),
            capabilities: DeveloperCapabilities::default(),
            pause_before_destructive: true,
            checkpoint_on_mutation: true,
            issued_unix: 0,
            confirmation_version: 0,
        }
    }
}

impl DeveloperAccessGrant {
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        if self.confirmation_version != DEVELOPER_ACCESS_CONFIRMATION_VERSION {
            return Err("Developer Full Access requires the current confirmation".into());
        }
        if self.issued_unix == 0 {
            return Err("Developer Full Access grant is missing its issue time".into());
        }
        if self.capabilities.production_systems {
            return Err("Developer Full Access cannot grant production systems".into());
        }
        self.scope.validate()
    }

    pub fn allows(&self, capability: CapabilityId, target: &ActionTarget) -> bool {
        self.enabled && self.capabilities.allows(capability) && self.scope.allows_target(target)
    }

    pub fn should_pause(&self, capability: CapabilityId, reversibility: Reversibility) -> bool {
        self.pause_before_destructive
            && (matches!(reversibility, Reversibility::Irreversible)
                || matches!(
                    capability,
                    CapabilityId::FsProjectDelete
                        | CapabilityId::SystemModify
                        | CapabilityId::ExternalDeploy
                        | CapabilityId::DataRemoteDelete
                ))
    }

    pub fn public_json(&self) -> serde_json::Value {
        let roots = match &self.scope {
            DeveloperScope::SelectedRepository { root, .. } => vec![root.clone()],
            DeveloperScope::SelectedDirectories { roots } => roots.clone(),
            DeveloperScope::EntireLocalMachine => Vec::new(),
        };
        serde_json::json!({
            "enabled": self.enabled,
            "scope": self.scope,
            "scope_label": self.scope.label(),
            "roots": roots,
            "capabilities": self.capabilities,
            "pause_before_destructive": self.pause_before_destructive,
            "checkpoint_on_mutation": self.checkpoint_on_mutation,
            "confirmation_version": self.confirmation_version,
        })
    }
}

fn enabled_by_default() -> bool {
    true
}

fn validate_root(raw: &str, label: &str) -> Result<(), String> {
    let path = Path::new(raw);
    if !path.is_absolute() {
        return Err(format!("{label} must be an absolute path"));
    }
    if raw.len() > 4096 || raw.trim().is_empty() {
        return Err(format!("{label} is invalid"));
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(format!("{label} must not contain parent traversal"));
    }
    Ok(())
}

fn path_within(candidate: &str, root: &str) -> bool {
    let candidate = normalize_path(Path::new(candidate));
    let root = normalize_path(Path::new(root));
    candidate == root || candidate.starts_with(root)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::RootDir => normalized.push(Path::new("/")),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Normal(part) => normalized.push(part),
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(path: &str) -> ActionTarget {
        ActionTarget {
            summary: "test".into(),
            project_root_hash: None,
            relative_path: None,
            absolute_path: Some(path.into()),
            owned_localhost: None,
        }
    }

    #[test]
    fn default_grant_is_disabled_and_safe() {
        let grant = DeveloperAccessGrant::default();
        assert!(!grant.enabled);
        assert!(grant.validate().is_ok());
        assert!(!grant.allows(CapabilityId::FsProjectWrite, &target("/tmp/project/file")));
    }

    #[test]
    fn selected_repository_does_not_match_a_prefix_sibling() {
        let grant = DeveloperAccessGrant {
            enabled: true,
            confirmation_version: DEVELOPER_ACCESS_CONFIRMATION_VERSION,
            issued_unix: 1,
            scope: DeveloperScope::SelectedRepository {
                root: "/tmp/project".into(),
                root_hash: None,
            },
            ..Default::default()
        };
        assert!(grant.validate().is_ok());
        assert!(grant.allows(
            CapabilityId::FsProjectWrite,
            &target("/tmp/project/src/lib.rs")
        ));
        assert!(!grant.allows(
            CapabilityId::FsProjectWrite,
            &target("/tmp/project-two/src")
        ));
        assert!(!grant.allows(
            CapabilityId::FsProjectWrite,
            &target("/tmp/project/../outside")
        ));
    }

    #[test]
    fn a_matching_root_hash_does_not_override_an_explicit_path() {
        let grant = DeveloperAccessGrant {
            enabled: true,
            confirmation_version: DEVELOPER_ACCESS_CONFIRMATION_VERSION,
            issued_unix: 1,
            scope: DeveloperScope::SelectedRepository {
                root: "/tmp/project".into(),
                root_hash: Some("a".repeat(64)),
            },
            ..Default::default()
        };
        let mut outside = target("/tmp/other/file");
        outside.project_root_hash = Some("a".repeat(64));
        assert!(!grant.allows(CapabilityId::FsProjectWrite, &outside));
        outside.absolute_path = None;
        assert!(grant.allows(CapabilityId::FsProjectWrite, &outside));
    }

    #[test]
    fn capability_toggles_and_destructive_pause_are_enforced() {
        let grant = DeveloperAccessGrant {
            enabled: true,
            confirmation_version: DEVELOPER_ACCESS_CONFIRMATION_VERSION,
            issued_unix: 1,
            scope: DeveloperScope::EntireLocalMachine,
            capabilities: DeveloperCapabilities {
                terminal_execution: true,
                process_management: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(grant.allows(CapabilityId::ProcessProjectExecute, &target("/tmp/project")));
        assert!(grant.should_pause(
            CapabilityId::ProcessProjectExecute,
            Reversibility::Irreversible
        ));
        assert!(!grant.capabilities.allows(CapabilityId::ExternalDeploy));
    }
}
