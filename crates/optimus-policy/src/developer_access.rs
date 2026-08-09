//! Explicit local authority for Optimus self-development.
//!
//! This is deliberately a data-and-predicate module. It does not touch the
//! filesystem, spawn processes, or infer authority from a UI label. The host
//! validates and persists a grant; the capability broker evaluates it for each
//! exact effect.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    ActionRequest, ActionTarget, AuthorityDecision, AutonomyProfile, CapabilityBroker,
    CapabilityId, RecoveryAction, Reversibility,
};

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
                    if !optimus_crypto::is_sha256_hex(hash) {
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

    /// The toggle a capability answers to, named as the settings UI names it.
    ///
    /// A denial that says "enable the capability" without saying which one leaves
    /// the user to guess across eight switches.
    pub fn governing_toggle(capability: CapabilityId) -> &'static str {
        match capability {
            CapabilityId::FsProjectRead
            | CapabilityId::FsProjectWrite
            | CapabilityId::FsProjectRename
            | CapabilityId::FsProjectDelete
            | CapabilityId::GitLocalRead
            | CapabilityId::GitLocalWrite => "Workspace files",
            CapabilityId::ProcessProjectExecute => "Terminal execution and Process management",
            CapabilityId::ProcessProjectServe => "Process management",
            CapabilityId::PackageSync | CapabilityId::PackageAdd => {
                "Terminal execution and Package installation"
            }
            CapabilityId::NetworkPublicRead
            | CapabilityId::NetworkRegistryRead
            | CapabilityId::NetworkLocalhostOwned
            | CapabilityId::NetworkPrivate
            | CapabilityId::BrowserPublicRead
            | CapabilityId::BrowserLocalhostOwned => "Network access",
            CapabilityId::CredentialUse | CapabilityId::CredentialReadRaw => "Secrets",
            CapabilityId::GitRemotePush
            | CapabilityId::GitRemotePullRequest
            | CapabilityId::ExternalSend
            | CapabilityId::ExternalPublish
            | CapabilityId::ExternalDeploy
            | CapabilityId::DataRemoteWrite
            | CapabilityId::DataRemoteDelete => "External services",
            CapabilityId::SystemModify => "Production systems",
            CapabilityId::CommerceSpend => "Spending",
        }
    }

    /// Whether the user could turn this capability on and retry.
    ///
    /// Production systems and spending are fences this mode does not open:
    /// [`DeveloperAccessGrant::validate`] refuses a grant that claims production
    /// access, and the settings UI never offers the switch (ADR-0076). Telling
    /// the user to enable one of those is advice they cannot take, so the broker
    /// must not answer with it — see
    /// [`crate::CapabilityBroker::decide`], which asks for these instead.
    pub fn is_user_enablable(capability: CapabilityId) -> bool {
        !matches!(
            capability,
            CapabilityId::SystemModify | CapabilityId::CommerceSpend
        )
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
                // Pop only a real component: `..` must never remove the root
                // (or a Windows prefix). Popping past the root would turn an
                // absolute path into a relative one, so `path_within` would
                // deny a path that genuinely resolves inside the root.
                if normalized.file_name().is_some() {
                    normalized.pop();
                }
            }
            std::path::Component::Normal(part) => normalized.push(part),
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
        }
    }
    normalized
}

/// The Developer Full Access branch of [`CapabilityBroker::decide`].
///
/// It lives beside the grant it reads rather than in the broker's match, so the
/// data model and the decision that interprets it move together. Split out of
/// `lib.rs` under architectural law 21.
impl CapabilityBroker {
    pub(crate) fn decide_developer_full_access(
        &self,
        request: &ActionRequest,
        grant: Option<&DeveloperAccessGrant>,
    ) -> AuthorityDecision {
        let Some(grant) = grant else {
            return AuthorityDecision::Deny {
                code: "developer_access_not_enabled".into(),
                reason: "Developer Full Access requires an active local grant.".into(),
                recovery: Some(RecoveryAction {
                    label: "Enable Developer Full Access".into(),
                    detail: "Confirm a scope and capability toggles in Optimus settings first."
                        .into(),
                }),
            };
        };
        if let Err(reason) = grant.validate() {
            return AuthorityDecision::Deny {
                code: "developer_access_invalid".into(),
                reason,
                recovery: Some(RecoveryAction {
                    label: "Review Developer Full Access".into(),
                    detail: "Revoke and enable the mode again with a valid scope.".into(),
                }),
            };
        }
        // Out of scope is the boundary the user drew, and the advice for it is
        // advice they can act on: widen the scope or work inside it.
        if !grant.scope.allows_target(&request.target) {
            return AuthorityDecision::Deny {
                code: "developer_access_scope_or_capability".into(),
                reason: format!(
                    "Developer Full Access does not grant {} outside {}.",
                    request.capability.as_str(),
                    grant.scope.label()
                ),
                recovery: Some(RecoveryAction {
                    label: "Adjust the Developer Full Access scope".into(),
                    detail: "Select a scope that contains this target, or work inside the active \
                             scope."
                        .into(),
                }),
            };
        }
        if !grant.capabilities.allows(request.capability) {
            // A toggle the user cannot reach is not a decision they declined.
            // `Production systems` is refused by `DeveloperAccessGrant::validate`
            // and never offered by the UI (ADR-0076), yet `OpaqueShell` — any
            // `sh -c` — maps onto it, so the most ordinary development command
            // came back denied with "enable the capability", which was
            // impossible. Worse, the same command *asks* when no grant exists:
            // turning the mode on removed an approval path instead of adding
            // one. Asking restores that path without widening anything, and the
            // broker still authorizes the exact effect.
            if !DeveloperCapabilities::is_user_enablable(request.capability) {
                return self.ask(
                    request,
                    "developer_access_needs_explicit_authority",
                    "Developer Full Access does not cover this capability, so it needs your \
                     approval for this exact action.",
                );
            }
            return AuthorityDecision::Deny {
                code: "developer_access_scope_or_capability".into(),
                reason: format!(
                    "Developer Full Access does not grant {}.",
                    request.capability.as_str()
                ),
                recovery: Some(RecoveryAction {
                    label: "Adjust the Developer Full Access grant".into(),
                    detail: format!(
                        "Turn on {} in Developer Full Access settings.",
                        DeveloperCapabilities::governing_toggle(request.capability)
                    ),
                }),
            };
        }
        if grant.should_pause(request.capability, request.reversibility) {
            return self.ask(
                request,
                "developer_pause_before_destructive",
                "Developer Full Access is active, but pause before destructive actions is enabled.",
            );
        }
        self.allow(
            AutonomyProfile::DeveloperFullAccess,
            request,
            "developer_full_access_grant",
        )
    }
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
    fn parent_components_inside_the_root_resolve_without_popping_the_root() {
        // Regression: `normalize_path` used to `pop()` on every `..`, so a
        // parent component aimed at the root itself removed it and turned an
        // absolute path into a relative one. `/../tmp/project/src/lib.rs`
        // resolves to `/tmp/project/src/lib.rs` — inside the root — but the
        // old normalizer produced the relative `tmp/project/src/lib.rs`,
        // which `starts_with` compared against the absolute root and denied.
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
        assert!(grant.allows(
            CapabilityId::FsProjectWrite,
            &target("/../tmp/project/src/lib.rs")
        ));
        assert!(grant.allows(
            CapabilityId::FsProjectWrite,
            &target("/tmp/project/sub/../src/lib.rs")
        ));
        // Escapes still resolve outside the root and stay denied.
        assert!(!grant.allows(
            CapabilityId::FsProjectWrite,
            &target("/tmp/project/../../etc/passwd")
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
