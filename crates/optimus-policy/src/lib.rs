//! Deterministic capability broker and product autonomy profiles (ADR-0044).
//!
//! Authorization is separate from auditing: callers still hash exact effects and
//! emit durable receipts. This crate only answers whether a trust profile may
//! auto-authorize, must ask the user, deny, or report unavailability.

mod command_class;
mod developer_access;

pub use command_class::{capability_for_command, classify_command, CommandClass};
pub use developer_access::{
    DeveloperAccessGrant, DeveloperCapabilities, DeveloperScope,
    DEVELOPER_ACCESS_CONFIRMATION_VERSION,
};

use std::net::{IpAddr, Ipv4Addr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Product-facing autonomy profile: when Optimus asks.
///
/// Orthogonal to command FS containment (`CommandFsEnvelope` in optimus-graph).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyProfile {
    /// Recommended product default: ordinary project work auto-authorized.
    Standard,
    /// Renamed “Ask before effects”: pause project writes and commands.
    #[default]
    ReviewChanges,
    /// Analysis only; mutations denied.
    ReadOnly,
    /// Broader project-local autonomy (Advanced), still not unrestricted host.
    FullProject,
    /// Explicit local self-development authority, bounded by a persisted grant.
    DeveloperFullAccess,
    /// Expert break-glass marker; runtime should pair with unrestricted policy.
    UnrestrictedHost,
}

impl AutonomyProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::ReviewChanges => "review_changes",
            Self::ReadOnly => "read_only",
            Self::FullProject => "full_project",
            Self::DeveloperFullAccess => "developer_full_access",
            Self::UnrestrictedHost => "unrestricted_host",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "standard" | "std" => Some(Self::Standard),
            "review_changes" | "review" | "ask" => Some(Self::ReviewChanges),
            "read_only" | "readonly" | "read" | "read-only" => Some(Self::ReadOnly),
            "full_project" | "full-project" | "project_full" => Some(Self::FullProject),
            "developer_full_access" | "developer-full-access" | "developer" => {
                Some(Self::DeveloperFullAccess)
            }
            // Break-glass answers to words that cannot be misread as ordinary.
            // “full” and “host” used to land here — the composer's first menu
            // item said "Full access" and meant this (#118) — so a stale
            // sender of either now falls closed to ReviewChanges instead of
            // quietly receiving the whole machine.
            "unrestricted_host" | "unrestricted" => Some(Self::UnrestrictedHost),
            _ => None,
        }
    }

    /// Actor string stamped on durable exact-effect trust grants.
    pub fn trust_actor(self) -> String {
        format!("trust_profile:{}", self.as_str())
    }
}

/// Closed capability vocabulary (expand carefully; fall closed on unknown).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityId {
    FsProjectRead,
    FsProjectWrite,
    FsProjectRename,
    FsProjectDelete,
    ProcessProjectExecute,
    ProcessProjectServe,
    PackageSync,
    PackageAdd,
    NetworkPublicRead,
    NetworkRegistryRead,
    NetworkLocalhostOwned,
    NetworkPrivate,
    CredentialUse,
    CredentialReadRaw,
    GitLocalRead,
    GitLocalWrite,
    GitRemotePush,
    GitRemotePullRequest,
    BrowserPublicRead,
    BrowserLocalhostOwned,
    ExternalSend,
    ExternalPublish,
    ExternalDeploy,
    DataRemoteWrite,
    DataRemoteDelete,
    SystemModify,
    CommerceSpend,
}

impl CapabilityId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FsProjectRead => "fs.project.read",
            Self::FsProjectWrite => "fs.project.write",
            Self::FsProjectRename => "fs.project.rename",
            Self::FsProjectDelete => "fs.project.delete",
            Self::ProcessProjectExecute => "process.project.execute",
            Self::ProcessProjectServe => "process.project.serve",
            Self::PackageSync => "package.sync",
            Self::PackageAdd => "package.add",
            Self::NetworkPublicRead => "network.public.read",
            Self::NetworkRegistryRead => "network.registry.read",
            Self::NetworkLocalhostOwned => "network.localhost.owned",
            Self::NetworkPrivate => "network.private",
            Self::CredentialUse => "credential.use",
            Self::CredentialReadRaw => "credential.read_raw",
            Self::GitLocalRead => "git.local.read",
            Self::GitLocalWrite => "git.local.write",
            Self::GitRemotePush => "git.remote.push",
            Self::GitRemotePullRequest => "git.remote.pull_request",
            Self::BrowserPublicRead => "browser.public.read",
            Self::BrowserLocalhostOwned => "browser.localhost.owned",
            Self::ExternalSend => "external.send",
            Self::ExternalPublish => "external.publish",
            Self::ExternalDeploy => "external.deploy",
            Self::DataRemoteWrite => "data.remote.write",
            Self::DataRemoteDelete => "data.remote.delete",
            Self::SystemModify => "system.modify",
            Self::CommerceSpend => "commerce.spend",
        }
    }

    pub fn is_project_mutation(self) -> bool {
        matches!(
            self,
            Self::FsProjectWrite
                | Self::FsProjectRename
                | Self::FsProjectDelete
                | Self::ProcessProjectExecute
                | Self::ProcessProjectServe
                | Self::PackageSync
                | Self::PackageAdd
                | Self::GitLocalWrite
        )
    }

    pub fn is_external_or_sensitive(self) -> bool {
        matches!(
            self,
            Self::NetworkPrivate
                | Self::CredentialReadRaw
                | Self::GitRemotePush
                | Self::GitRemotePullRequest
                | Self::ExternalSend
                | Self::ExternalPublish
                | Self::ExternalDeploy
                | Self::DataRemoteWrite
                | Self::DataRemoteDelete
                | Self::SystemModify
                | Self::CommerceSpend
        )
    }

    fn requires_owned_localhost_binding(self) -> bool {
        matches!(
            self,
            Self::ProcessProjectServe | Self::NetworkLocalhostOwned | Self::BrowserLocalhostOwned
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Reversibility {
    Reversible,
    Checkpointed,
    Irreversible,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Ordinary,
    Sensitive,
    Credential,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Externality {
    ProjectLocal,
    OwnedLocalhost,
    PublicNetwork,
    PrivateNetwork,
    RemoteService,
    HostSystem,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectDigest {
    pub sha256_hex: String,
}

/// Exact constraint envelope for one candidate Optimus-owned localhost lease.
///
/// An IP address is used rather than a hostname so `localhost` resolution cannot
/// widen the grant. The broker validates envelope coherence only: a trusted
/// runtime caller must prove process/listener ownership, liveness, generation,
/// and expiry before requesting a decision with this value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnedLocalhostBinding {
    pub lease_id: Uuid,
    /// Initially only plain HTTP is supported. Encoding the scheme keeps a
    /// future HTTPS widening from silently inheriting an HTTP authority.
    pub scheme: OwnedLocalhostScheme,
    pub host: IpAddr,
    pub port: u16,
    pub project_scope_id: String,
    pub project_root_hash: String,
    pub session_id: Uuid,
    pub run_id: Uuid,
    pub owner_job_id: Uuid,
    pub owner_attempt_id: Uuid,
    pub process_tree_id: String,
    pub generation: u64,
    pub expires_unix: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OwnedLocalhostScheme {
    Http,
}

impl OwnedLocalhostBinding {
    fn is_valid_for(&self, request: &ActionRequest) -> bool {
        self.scheme == OwnedLocalhostScheme::Http
            && !self.lease_id.is_nil()
            && self.host == IpAddr::V4(Ipv4Addr::LOCALHOST)
            && self.port != 0
            && !self.project_scope_id.is_empty()
            && request.project_scope_id.as_deref() == Some(self.project_scope_id.as_str())
            && self.project_root_hash.len() == 64
            && self
                .project_root_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            && request.target.project_root_hash.as_deref() == Some(self.project_root_hash.as_str())
            && request.externality == Externality::OwnedLocalhost
            && !self.session_id.is_nil()
            && !self.run_id.is_nil()
            && request.run_id == Some(self.run_id)
            && !self.owner_job_id.is_nil()
            && !self.owner_attempt_id.is_nil()
            && !self.process_tree_id.trim().is_empty()
            && self.process_tree_id.trim() == self.process_tree_id
            && self.process_tree_id.len() <= 256
            && self.generation != 0
            && self.expires_unix != 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionTarget {
    pub summary: String,
    pub project_root_hash: Option<String>,
    pub relative_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub absolute_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_localhost: Option<OwnedLocalhostBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionRequest {
    pub run_id: Option<Uuid>,
    pub actor: String,
    pub tool_id: Option<String>,
    pub project_scope_id: Option<String>,
    pub capability: CapabilityId,
    pub target: ActionTarget,
    pub effect_digest: EffectDigest,
    pub reversibility: Reversibility,
    pub sensitivity: Sensitivity,
    pub externality: Externality,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub developer_access: Option<DeveloperAccessGrant>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppliedConstraints {
    pub project_root_hash: Option<String>,
    pub profile: AutonomyProfile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_localhost: Option<OwnedLocalhostBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionReason {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalRequest {
    pub capability: CapabilityId,
    pub summary: String,
    pub reason: DecisionReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryAction {
    pub label: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthorityDecision {
    Allow {
        authority_id: String,
        constraints: AppliedConstraints,
        reason: DecisionReason,
    },
    Ask {
        request: ApprovalRequest,
    },
    Deny {
        code: String,
        reason: String,
        recovery: Option<RecoveryAction>,
    },
    Unavailable {
        capability: CapabilityId,
        reason: String,
        recovery: Option<RecoveryAction>,
    },
}

impl AuthorityDecision {
    pub fn is_allow(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }

    pub fn is_ask(&self) -> bool {
        matches!(self, Self::Ask { .. })
    }
}

/// Deterministic capability broker. Pure: no I/O, no model calls.
#[derive(Debug, Default, Clone, Copy)]
pub struct CapabilityBroker;

impl CapabilityBroker {
    pub fn decide(&self, profile: AutonomyProfile, request: &ActionRequest) -> AuthorityDecision {
        if request.target.owned_localhost.is_some()
            && !request.capability.requires_owned_localhost_binding()
        {
            return AuthorityDecision::Deny {
                code: "unexpected_owned_localhost_binding".into(),
                reason: "Owned-localhost constraints cannot be attached to another capability."
                    .into(),
                recovery: None,
            };
        }
        if request.capability.requires_owned_localhost_binding()
            && !request
                .target
                .owned_localhost
                .as_ref()
                .is_some_and(|binding| binding.is_valid_for(request))
        {
            return AuthorityDecision::Deny {
                code: "invalid_owned_localhost_binding".into(),
                reason: format!(
                    "{} requires coherent localhost lease constraints for the same project and run.",
                    request.capability.as_str()
                ),
                recovery: Some(RecoveryAction {
                    label: "Acquire an owned localhost lease".into(),
                    detail: "Prove the exact listener and process-tree ownership before requesting this capability.".into(),
                }),
            };
        }

        // Hard fences first.
        let developer_grant = (profile == AutonomyProfile::DeveloperFullAccess)
            .then_some(request.developer_access.as_ref())
            .flatten();
        if matches!(request.capability, CapabilityId::CommerceSpend)
            || (matches!(request.sensitivity, Sensitivity::Credential)
                && matches!(request.capability, CapabilityId::CredentialReadRaw)
                && developer_grant.is_none())
            || (matches!(request.externality, Externality::HostSystem)
                && matches!(request.capability, CapabilityId::SystemModify)
                && developer_grant.is_none())
        {
            if matches!(profile, AutonomyProfile::UnrestrictedHost) {
                return self.allow(profile, request, "unrestricted_host_break_glass");
            }
            return self.ask(
                request,
                "hard_fence",
                "This action always requires explicit user authority.",
            );
        }

        match profile {
            AutonomyProfile::UnrestrictedHost => {
                self.allow(profile, request, "unrestricted_host_profile")
            }
            AutonomyProfile::ReadOnly => {
                if request.capability.is_project_mutation()
                    || request.capability.is_external_or_sensitive()
                    || matches!(
                        request.capability,
                        CapabilityId::ProcessProjectExecute | CapabilityId::ProcessProjectServe
                    )
                {
                    AuthorityDecision::Deny {
                        code: "read_only_profile".into(),
                        reason: "Read only profile forbids mutations and command execution.".into(),
                        recovery: Some(RecoveryAction {
                            label: "Switch to Standard or Review changes".into(),
                            detail: "Choose an autonomy profile that permits project work.".into(),
                        }),
                    }
                } else {
                    self.allow(profile, request, "read_only_read")
                }
            }
            AutonomyProfile::ReviewChanges => {
                if request.capability.is_project_mutation()
                    || request.capability.is_external_or_sensitive()
                    || matches!(
                        request.capability,
                        CapabilityId::ProcessProjectExecute
                            | CapabilityId::ProcessProjectServe
                            | CapabilityId::PackageSync
                            | CapabilityId::PackageAdd
                    )
                {
                    self.ask(
                        request,
                        "review_changes",
                        "Review changes asks before project writes and command execution.",
                    )
                } else {
                    self.allow(profile, request, "review_changes_read")
                }
            }
            AutonomyProfile::Standard => self.decide_standard(request),
            AutonomyProfile::FullProject => self.decide_full_project(request),
            AutonomyProfile::DeveloperFullAccess => {
                self.decide_developer_full_access(request, developer_grant)
            }
        }
    }

    fn decide_standard(&self, request: &ActionRequest) -> AuthorityDecision {
        if request.capability.is_external_or_sensitive()
            || matches!(
                request.externality,
                Externality::RemoteService | Externality::HostSystem | Externality::PrivateNetwork
            )
        {
            return self.ask(
                request,
                "standard_boundary",
                "Standard asks for external, private-network, or host-system effects.",
            );
        }

        // Unusually destructive: irreversible delete without checkpoint flag.
        if matches!(request.capability, CapabilityId::FsProjectDelete)
            && matches!(request.reversibility, Reversibility::Irreversible)
        {
            return self.ask(
                request,
                "standard_destructive_delete",
                "Standard asks for irreversible or bulk deletion.",
            );
        }

        match request.capability {
            CapabilityId::FsProjectRead
            | CapabilityId::FsProjectWrite
            | CapabilityId::FsProjectRename
            | CapabilityId::FsProjectDelete
            | CapabilityId::ProcessProjectExecute
            | CapabilityId::ProcessProjectServe
            | CapabilityId::PackageSync
            | CapabilityId::PackageAdd
            | CapabilityId::NetworkPublicRead
            | CapabilityId::NetworkRegistryRead
            | CapabilityId::NetworkLocalhostOwned
            | CapabilityId::BrowserPublicRead
            | CapabilityId::BrowserLocalhostOwned
            | CapabilityId::GitLocalRead
            | CapabilityId::GitLocalWrite
            | CapabilityId::CredentialUse => {
                self.allow(AutonomyProfile::Standard, request, "standard_project_trust")
            }
            _ => self.ask(
                request,
                "standard_unknown_or_elevated",
                "Standard does not auto-authorize this capability.",
            ),
        }
    }

    fn decide_full_project(&self, request: &ActionRequest) -> AuthorityDecision {
        if request.capability.is_external_or_sensitive()
            || matches!(
                request.externality,
                Externality::RemoteService | Externality::HostSystem
            )
        {
            return self.ask(
                request,
                "full_project_boundary",
                "Full project still asks for external and host-system effects.",
            );
        }
        if matches!(
            request.externality,
            Externality::ProjectLocal | Externality::OwnedLocalhost | Externality::PublicNetwork
        ) || request.capability.is_project_mutation()
            || matches!(
                request.capability,
                CapabilityId::FsProjectRead
                    | CapabilityId::NetworkPublicRead
                    | CapabilityId::NetworkLocalhostOwned
                    | CapabilityId::BrowserLocalhostOwned
                    | CapabilityId::GitLocalRead
                    | CapabilityId::GitLocalWrite
                    | CapabilityId::PackageSync
                    | CapabilityId::PackageAdd
                    | CapabilityId::ProcessProjectExecute
                    | CapabilityId::ProcessProjectServe
            )
        {
            return self.allow(AutonomyProfile::FullProject, request, "full_project_trust");
        }
        self.ask(
            request,
            "full_project_elevated",
            "Full project does not auto-authorize this capability.",
        )
    }

    fn allow(
        &self,
        profile: AutonomyProfile,
        request: &ActionRequest,
        code: &str,
    ) -> AuthorityDecision {
        AuthorityDecision::Allow {
            authority_id: format!(
                "{}:{}:{}",
                profile.as_str(),
                request.capability.as_str(),
                request.effect_digest.sha256_hex
            ),
            constraints: AppliedConstraints {
                project_root_hash: request.target.project_root_hash.clone(),
                profile,
                owned_localhost: request.target.owned_localhost.clone(),
            },
            reason: DecisionReason {
                code: code.into(),
                message: format!(
                    "Authorized by {} for {}",
                    profile.as_str(),
                    request.capability.as_str()
                ),
            },
        }
    }

    fn ask(&self, request: &ActionRequest, code: &str, message: &str) -> AuthorityDecision {
        AuthorityDecision::Ask {
            request: ApprovalRequest {
                capability: request.capability,
                summary: request.target.summary.clone(),
                reason: DecisionReason {
                    code: code.into(),
                    message: message.into(),
                },
            },
        }
    }
}

/// Map Work Graph effect kind names to capabilities (stringly to avoid a graph dep).
///
/// `ProjectServe` maps to `ProcessProjectExecute`, **not** `ProcessProjectServe`.
/// The effect gate decides whether this process may start at all, and it runs
/// before any listener exists, so it has no binding to present;
/// [`build_effect_request_for`] always emits `owned_localhost: None` and the
/// broker denies every binding-requiring capability without one. The lease
/// authority is a separate decision over a proven binding — see
/// [`build_owned_localhost_serve_request`].
pub fn capability_for_effect_kind(kind: &str) -> Option<CapabilityId> {
    match kind {
        "WriteFile" | "ProjectWriteFile" | "Mkdir" | "ProjectMkdir" | "PatchFile"
        | "ProjectPatchFile" => Some(CapabilityId::FsProjectWrite),
        "RenamePath" | "ProjectRenamePath" => Some(CapabilityId::FsProjectRename),
        "DeletePath" | "ProjectDeletePath" => Some(CapabilityId::FsProjectDelete),
        "RunCommand" | "ProjectRunCommand" | "ProjectServe" => {
            Some(CapabilityId::ProcessProjectExecute)
        }
        "AssertFileEquals" => Some(CapabilityId::FsProjectRead),
        _ => None,
    }
}

/// Authority request for an Optimus-owned localhost lease over a listener whose
/// ownership the runtime has already proven (ADR-0060).
///
/// Separate from [`build_effect_request_for`] because a `ProcessProjectServe`
/// decision is only coherent with the exact binding attached. Passing the
/// binding through the broker is what forces it past
/// [`OwnedLocalhostBinding::is_valid_for`]: a lease whose project scope, root
/// hash, run, or process-tree identity does not cohere is denied here rather
/// than reaching a consumer.
pub fn build_owned_localhost_serve_request(
    effect_hash: &str,
    project_scope_id: String,
    summary: String,
    binding: OwnedLocalhostBinding,
) -> ActionRequest {
    ActionRequest {
        run_id: Some(binding.run_id),
        actor: "runtime".into(),
        tool_id: None,
        project_scope_id: Some(project_scope_id),
        capability: CapabilityId::ProcessProjectServe,
        target: ActionTarget {
            summary,
            project_root_hash: Some(binding.project_root_hash.clone()),
            relative_path: None,
            absolute_path: None,
            owned_localhost: Some(binding),
        },
        effect_digest: EffectDigest {
            sha256_hex: effect_hash.to_string(),
        },
        // Revoking the lease terminates the process tree that holds the
        // listener, so the effect of granting one is undoable in full.
        reversibility: Reversibility::Reversible,
        sensitivity: Sensitivity::Ordinary,
        externality: Externality::OwnedLocalhost,
        developer_access: None,
    }
}

pub fn build_effect_request(
    kind: &str,
    effect_hash: &str,
    project_root_hash: Option<String>,
    summary: String,
    relative_path: Option<String>,
) -> Option<ActionRequest> {
    build_effect_request_for(
        kind,
        effect_hash,
        project_root_hash,
        summary,
        relative_path,
        None,
    )
}

/// As [`build_effect_request`], but with the command behind a `RunCommand`
/// effect so it can be classified.
///
/// Without `command`, every command collapses to `ProcessProjectExecute` and
/// the broker cannot tell `cargo test` from `cargo add some-crate` or from
/// `npm install -g`. With it, the request names what the command actually
/// reaches, and the approval prompt can say so.
pub fn build_effect_request_for(
    kind: &str,
    effect_hash: &str,
    project_root_hash: Option<String>,
    summary: String,
    relative_path: Option<String>,
    command: Option<(&str, &[String])>,
) -> Option<ActionRequest> {
    let mut capability = capability_for_effect_kind(kind)?;
    let mut class = None;
    if matches!(capability, CapabilityId::ProcessProjectExecute) {
        if let Some((program, args)) = command {
            let classified = classify_command(program, args);
            capability = classified.capability();
            class = Some(classified);
        }
    }
    let reversibility = match capability {
        // Delete effects cannot claim checkpoint recovery until the runtime
        // actually creates and retains a checkpoint manifest.
        CapabilityId::FsProjectDelete => Reversibility::Irreversible,
        // A dependency change is recoverable from the lockfile in git; running
        // arbitrary code and changing the host are not.
        CapabilityId::PackageSync | CapabilityId::PackageAdd => Reversibility::Checkpointed,
        CapabilityId::ProcessProjectExecute | CapabilityId::SystemModify => {
            Reversibility::Irreversible
        }
        _ => Reversibility::Reversible,
    };
    let externality = class.map_or(Externality::ProjectLocal, CommandClass::externality);
    Some(ActionRequest {
        run_id: None,
        actor: "runtime".into(),
        tool_id: None,
        project_scope_id: None,
        capability,
        target: ActionTarget {
            summary,
            project_root_hash,
            relative_path,
            absolute_path: None,
            owned_localhost: None,
        },
        effect_digest: EffectDigest {
            sha256_hex: effect_hash.to_string(),
        },
        reversibility,
        sensitivity: Sensitivity::Ordinary,
        externality,
        developer_access: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(cap: CapabilityId) -> ActionRequest {
        ActionRequest {
            run_id: None,
            actor: "test".into(),
            tool_id: None,
            project_scope_id: Some("proj".into()),
            capability: cap,
            target: ActionTarget {
                summary: "test".into(),
                project_root_hash: Some("abc".into()),
                relative_path: Some("src/App.tsx".into()),
                absolute_path: Some("/tmp/project/src/App.tsx".into()),
                owned_localhost: None,
            },
            effect_digest: EffectDigest {
                sha256_hex: "a".repeat(64),
            },
            reversibility: Reversibility::Reversible,
            sensitivity: Sensitivity::Ordinary,
            externality: Externality::ProjectLocal,
            developer_access: None,
        }
    }

    fn owned_localhost_binding() -> OwnedLocalhostBinding {
        OwnedLocalhostBinding {
            lease_id: Uuid::from_u128(10),
            scheme: OwnedLocalhostScheme::Http,
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 4173,
            project_scope_id: "project-a".into(),
            project_root_hash: "a".repeat(64),
            session_id: Uuid::from_u128(1),
            run_id: Uuid::from_u128(2),
            owner_job_id: Uuid::from_u128(3),
            owner_attempt_id: Uuid::from_u128(4),
            process_tree_id: "optimus-command-42.service".into(),
            generation: 1,
            expires_unix: 4_102_444_800,
        }
    }

    fn owned_localhost_req(capability: CapabilityId) -> ActionRequest {
        let binding = owned_localhost_binding();
        let mut request = req(capability);
        request.run_id = Some(binding.run_id);
        request.project_scope_id = Some(binding.project_scope_id.clone());
        request.target.project_root_hash = Some(binding.project_root_hash.clone());
        request.externality = Externality::OwnedLocalhost;
        request.target.owned_localhost = Some(binding);
        request
    }

    #[test]
    fn standard_allows_project_write() {
        let d = CapabilityBroker.decide(
            AutonomyProfile::Standard,
            &req(CapabilityId::FsProjectWrite),
        );
        assert!(d.is_allow());
    }

    #[test]
    fn review_asks_project_write() {
        let d = CapabilityBroker.decide(
            AutonomyProfile::ReviewChanges,
            &req(CapabilityId::FsProjectWrite),
        );
        assert!(d.is_ask());
    }

    #[test]
    fn read_only_denies_write() {
        let d = CapabilityBroker.decide(
            AutonomyProfile::ReadOnly,
            &req(CapabilityId::FsProjectWrite),
        );
        assert!(matches!(d, AuthorityDecision::Deny { .. }));
    }

    #[test]
    fn developer_full_access_requires_a_valid_grant() {
        let denied = CapabilityBroker.decide(
            AutonomyProfile::DeveloperFullAccess,
            &req(CapabilityId::FsProjectWrite),
        );
        assert!(matches!(
            denied,
            AuthorityDecision::Deny {
                code,
                ..
            } if code == "developer_access_not_enabled"
        ));

        let mut request = req(CapabilityId::FsProjectWrite);
        let mut grant = DeveloperAccessGrant {
            enabled: true,
            confirmation_version: DEVELOPER_ACCESS_CONFIRMATION_VERSION,
            issued_unix: 1,
            scope: DeveloperScope::SelectedRepository {
                root: "/tmp/project".into(),
                root_hash: None,
            },
            pause_before_destructive: false,
            ..Default::default()
        };
        request.developer_access = Some(grant.clone());
        assert!(CapabilityBroker
            .decide(AutonomyProfile::DeveloperFullAccess, &request)
            .is_allow());

        grant.capabilities.workspace_files = false;
        request.developer_access = Some(grant);
        assert!(matches!(
            CapabilityBroker.decide(AutonomyProfile::DeveloperFullAccess, &request),
            AuthorityDecision::Deny {
                code,
                ..
            } if code == "developer_access_scope_or_capability"
        ));
    }

    #[test]
    fn developer_full_access_never_answers_with_a_toggle_the_user_cannot_reach() {
        // Observed live: every `terminal` call in a self-development session came
        // back "Developer Full Access does not grant system.modify in the
        // selected scope", advising the user to enable the capability. They
        // could not: `system.modify` answers to `production_systems`, `validate`
        // refuses any grant that sets it, and the UI never offers the switch
        // (ADR-0076). The mode meant for self-development made the most ordinary
        // development command impossible.
        let mut request = req(CapabilityId::SystemModify);
        request.externality = Externality::HostSystem;
        request.target.absolute_path = Some("/tmp/project/run.sh".into());
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
        assert!(
            !grant.capabilities.production_systems,
            "a valid grant can never claim production systems"
        );
        request.developer_access = Some(grant);

        let decision = CapabilityBroker.decide(AutonomyProfile::DeveloperFullAccess, &request);
        assert!(
            decision.is_ask(),
            "an unreachable toggle must ask, not deny: {decision:?}"
        );

        // And asking is not a widening: without the grant at all, the hard fence
        // already asks for this exact request. Turning the mode on must never
        // remove an approval path that existed without it.
        request.developer_access = None;
        assert!(CapabilityBroker
            .decide(AutonomyProfile::DeveloperFullAccess, &request)
            .is_ask());
    }

    #[test]
    fn a_reachable_toggle_still_denies_and_names_the_switch_to_turn_on() {
        let mut request = req(CapabilityId::ExternalSend);
        request.target.absolute_path = Some("/tmp/project/file".into());
        request.developer_access = Some(DeveloperAccessGrant {
            enabled: true,
            confirmation_version: DEVELOPER_ACCESS_CONFIRMATION_VERSION,
            issued_unix: 1,
            scope: DeveloperScope::SelectedRepository {
                root: "/tmp/project".into(),
                root_hash: None,
            },
            ..Default::default()
        });

        let AuthorityDecision::Deny { recovery, .. } =
            CapabilityBroker.decide(AutonomyProfile::DeveloperFullAccess, &request)
        else {
            panic!("a toggle the user can turn on stays a denial");
        };
        assert_eq!(
            recovery.expect("a denial must say what to do").detail,
            "Turn on External services in Developer Full Access settings."
        );
    }

    #[test]
    fn leaving_the_granted_scope_is_denied_and_says_so() {
        let mut request = req(CapabilityId::FsProjectWrite);
        request.target.absolute_path = Some("/tmp/elsewhere/file".into());
        request.developer_access = Some(DeveloperAccessGrant {
            enabled: true,
            confirmation_version: DEVELOPER_ACCESS_CONFIRMATION_VERSION,
            issued_unix: 1,
            scope: DeveloperScope::SelectedRepository {
                root: "/tmp/project".into(),
                root_hash: None,
            },
            ..Default::default()
        });

        let AuthorityDecision::Deny { reason, .. } =
            CapabilityBroker.decide(AutonomyProfile::DeveloperFullAccess, &request)
        else {
            panic!("the granted scope is a boundary, not a prompt");
        };
        assert!(reason.contains("Selected repository"), "{reason}");
    }

    #[test]
    fn developer_full_access_pause_toggle_stays_an_approval_boundary() {
        let mut request = req(CapabilityId::FsProjectDelete);
        request.developer_access = Some(DeveloperAccessGrant {
            enabled: true,
            confirmation_version: DEVELOPER_ACCESS_CONFIRMATION_VERSION,
            issued_unix: 1,
            scope: DeveloperScope::SelectedRepository {
                root: "/tmp/project".into(),
                root_hash: None,
            },
            ..Default::default()
        });
        assert!(CapabilityBroker
            .decide(AutonomyProfile::DeveloperFullAccess, &request)
            .is_ask());
    }

    #[test]
    fn standard_allows_exact_owned_localhost_capabilities_and_copies_constraints() {
        for capability in [
            CapabilityId::ProcessProjectServe,
            CapabilityId::NetworkLocalhostOwned,
            CapabilityId::BrowserLocalhostOwned,
        ] {
            let request = owned_localhost_req(capability);
            let expected = request.target.owned_localhost.clone();
            let decision = CapabilityBroker.decide(AutonomyProfile::Standard, &request);
            match decision {
                AuthorityDecision::Allow { constraints, .. } => {
                    assert_eq!(constraints.owned_localhost, expected, "{capability:?}");
                }
                other => panic!("expected localhost allow for {capability:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn owned_localhost_capabilities_deny_without_a_binding() {
        for capability in [
            CapabilityId::ProcessProjectServe,
            CapabilityId::NetworkLocalhostOwned,
            CapabilityId::BrowserLocalhostOwned,
        ] {
            let mut request = req(capability);
            request.externality = Externality::OwnedLocalhost;
            let decision = CapabilityBroker.decide(AutonomyProfile::Standard, &request);
            assert!(
                matches!(
                    decision,
                    AuthorityDecision::Deny { ref code, .. }
                        if code == "invalid_owned_localhost_binding"
                ),
                "{capability:?}: {decision:?}"
            );
        }
    }

    #[test]
    fn owned_localhost_constraints_cannot_ride_an_unrelated_capability() {
        let mut request = req(CapabilityId::FsProjectRead);
        request.target.owned_localhost = Some(owned_localhost_binding());
        let decision = CapabilityBroker.decide(AutonomyProfile::Standard, &request);
        assert!(matches!(
            decision,
            AuthorityDecision::Deny { ref code, .. }
                if code == "unexpected_owned_localhost_binding"
        ));
    }

    #[test]
    fn owned_localhost_binding_fails_closed_when_malformed_or_transferred() {
        let base = owned_localhost_req(CapabilityId::BrowserLocalhostOwned);
        let mut malformed = Vec::new();

        let mut request = base.clone();
        request.target.owned_localhost.as_mut().unwrap().lease_id = Uuid::nil();
        malformed.push(request);

        let mut request = base.clone();
        request.target.owned_localhost.as_mut().unwrap().host =
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        malformed.push(request);

        let mut request = base.clone();
        request.target.owned_localhost.as_mut().unwrap().port = 0;
        malformed.push(request);

        let mut request = base.clone();
        request.project_scope_id = Some("project-b".into());
        malformed.push(request);

        let mut request = base.clone();
        request.target.project_root_hash = Some("other-root".into());
        malformed.push(request);

        let mut request = base.clone();
        request.externality = Externality::ProjectLocal;
        malformed.push(request);

        let mut request = base.clone();
        request.target.owned_localhost.as_mut().unwrap().session_id = Uuid::nil();
        malformed.push(request);

        let mut request = base.clone();
        request
            .target
            .owned_localhost
            .as_mut()
            .unwrap()
            .owner_job_id = Uuid::nil();
        malformed.push(request);

        let mut request = base.clone();
        request
            .target
            .owned_localhost
            .as_mut()
            .unwrap()
            .owner_attempt_id = Uuid::nil();
        malformed.push(request);

        let mut request = base.clone();
        request
            .target
            .owned_localhost
            .as_mut()
            .unwrap()
            .process_tree_id = " ".into();
        malformed.push(request);

        let mut request = base.clone();
        request.target.owned_localhost.as_mut().unwrap().generation = 0;
        malformed.push(request);

        let mut request = base.clone();
        request
            .target
            .owned_localhost
            .as_mut()
            .unwrap()
            .expires_unix = 0;
        malformed.push(request);

        let mut request = base;
        request.run_id = Some(Uuid::from_u128(3));
        malformed.push(request);

        for request in malformed {
            let decision = CapabilityBroker.decide(AutonomyProfile::Standard, &request);
            assert!(matches!(
                decision,
                AuthorityDecision::Deny { ref code, .. }
                    if code == "invalid_owned_localhost_binding"
            ));
        }
    }

    #[test]
    fn read_only_denies_owned_project_serve_with_a_valid_binding() {
        let decision = CapabilityBroker.decide(
            AutonomyProfile::ReadOnly,
            &owned_localhost_req(CapabilityId::ProcessProjectServe),
        );
        assert!(matches!(
            decision,
            AuthorityDecision::Deny { ref code, .. } if code == "read_only_profile"
        ));
    }

    #[test]
    fn review_changes_asks_for_owned_project_serve_with_a_valid_binding() {
        let decision = CapabilityBroker.decide(
            AutonomyProfile::ReviewChanges,
            &owned_localhost_req(CapabilityId::ProcessProjectServe),
        );
        assert!(decision.is_ask());
    }

    /// The effect gate runs before a listener exists, so a serve effect cannot
    /// present a binding there. Mapping it to `ProcessProjectServe` would make
    /// every serve deny as `invalid_owned_localhost_binding` — a fail-closed
    /// that looks like policy but is really a wiring bug.
    #[test]
    fn serve_effect_kind_gates_as_process_execute_not_as_a_lease() {
        assert_eq!(
            capability_for_effect_kind("ProjectServe"),
            Some(CapabilityId::ProcessProjectExecute)
        );

        let request = build_effect_request_for(
            "ProjectServe",
            &"c".repeat(64),
            Some("d".repeat(64)),
            "project serve python3 -m http.server".into(),
            None,
            Some(("python3", &["-m".to_string(), "http.server".to_string()])),
        )
        .expect("serve effects map to a capability");
        assert_eq!(request.capability, CapabilityId::ProcessProjectExecute);
        assert_eq!(request.target.owned_localhost, None);
        assert!(CapabilityBroker
            .decide(AutonomyProfile::Standard, &request)
            .is_allow());
    }

    #[test]
    fn a_proven_lease_request_carries_its_binding_through_the_broker() {
        let binding = owned_localhost_binding();
        let request = build_owned_localhost_serve_request(
            &"c".repeat(64),
            binding.project_scope_id.clone(),
            "serve on 127.0.0.1:4173".into(),
            binding.clone(),
        );

        assert_eq!(request.capability, CapabilityId::ProcessProjectServe);
        assert_eq!(request.externality, Externality::OwnedLocalhost);
        assert_eq!(request.run_id, Some(binding.run_id));
        assert_eq!(
            request.target.project_root_hash.as_deref(),
            Some(binding.project_root_hash.as_str())
        );

        match CapabilityBroker.decide(AutonomyProfile::Standard, &request) {
            AuthorityDecision::Allow { constraints, .. } => {
                assert_eq!(constraints.owned_localhost, Some(binding));
            }
            other => panic!("expected a lease allow, got {other:?}"),
        }
    }

    /// A lease whose identity does not cohere with its own request is refused
    /// here rather than reaching a consumer, so an incoherent binding can never
    /// be handed out even if the runtime built one.
    #[test]
    fn an_incoherent_proven_lease_request_is_denied() {
        let mut binding = owned_localhost_binding();
        binding.project_root_hash = "not-a-root-hash".into();
        let request = build_owned_localhost_serve_request(
            &"c".repeat(64),
            binding.project_scope_id.clone(),
            "serve on 127.0.0.1:4173".into(),
            binding,
        );
        assert!(matches!(
            CapabilityBroker.decide(AutonomyProfile::Standard, &request),
            AuthorityDecision::Deny { ref code, .. } if code == "invalid_owned_localhost_binding"
        ));
    }

    #[test]
    fn old_serialized_targets_and_constraints_default_the_binding_to_none() {
        let target: ActionTarget = serde_json::from_value(serde_json::json!({
            "summary": "legacy",
            "project_root_hash": "abc",
            "relative_path": null
        }))
        .unwrap();
        assert_eq!(target.owned_localhost, None);

        let constraints: AppliedConstraints = serde_json::from_value(serde_json::json!({
            "project_root_hash": "abc",
            "profile": "standard"
        }))
        .unwrap();
        assert_eq!(constraints.owned_localhost, None);
    }

    #[test]
    fn standard_asks_git_push() {
        let mut r = req(CapabilityId::GitRemotePush);
        r.externality = Externality::RemoteService;
        let d = CapabilityBroker.decide(AutonomyProfile::Standard, &r);
        assert!(d.is_ask());
    }

    #[test]
    fn parse_legacy_access_aliases() {
        assert_eq!(
            AutonomyProfile::parse("ask"),
            Some(AutonomyProfile::ReviewChanges)
        );
        assert_eq!(
            AutonomyProfile::parse("read"),
            Some(AutonomyProfile::ReadOnly)
        );
        // Hyphenated spellings are accepted for the other profiles
        // ("full-project", "developer-full-access"); "read-only" must parse
        // symmetrically instead of quietly failing closed.
        assert_eq!(
            AutonomyProfile::parse("read-only"),
            Some(AutonomyProfile::ReadOnly)
        );
        assert_eq!(
            AutonomyProfile::parse("READ-ONLY"),
            Some(AutonomyProfile::ReadOnly)
        );
        assert_eq!(
            AutonomyProfile::parse("standard"),
            Some(AutonomyProfile::Standard)
        );
    }

    /// The composer's old first menu item said "Full access" and parsed to
    /// unrestricted host (#118). An ordinary-sounding word must never carry
    /// break-glass again: unknown falls to no profile, and a caller that gets
    /// `None` fails closed to ReviewChanges.
    #[test]
    fn an_ordinary_sounding_word_does_not_mean_break_glass() {
        for raw in ["full", "host", "all", "everything"] {
            assert_eq!(AutonomyProfile::parse(raw), None, "{raw}");
        }
        for raw in ["unrestricted_host", "unrestricted", "UNRESTRICTED_HOST"] {
            assert_eq!(
                AutonomyProfile::parse(raw),
                Some(AutonomyProfile::UnrestrictedHost),
                "{raw}"
            );
        }
    }
}
