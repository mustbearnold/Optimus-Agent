//! Deterministic capability broker and product autonomy profiles (ADR-0044).
//!
//! Authorization is separate from auditing: callers still hash exact effects and
//! emit durable receipts. This crate only answers whether a trust profile may
//! auto-authorize, must ask the user, deny, or report unavailability.

mod command_class;

pub use command_class::{capability_for_command, classify_command, CommandClass};

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
            Self::UnrestrictedHost => "unrestricted_host",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "standard" | "std" => Some(Self::Standard),
            "review_changes" | "review" | "ask" => Some(Self::ReviewChanges),
            "read_only" | "readonly" | "read" => Some(Self::ReadOnly),
            "full_project" | "full-project" | "project_full" => Some(Self::FullProject),
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionTarget {
    pub summary: String,
    pub project_root_hash: Option<String>,
    pub relative_path: Option<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppliedConstraints {
    pub project_root_hash: Option<String>,
    pub profile: AutonomyProfile,
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
        // Hard fences first.
        if matches!(request.capability, CapabilityId::CommerceSpend)
            || matches!(request.sensitivity, Sensitivity::Credential)
                && matches!(request.capability, CapabilityId::CredentialReadRaw)
            || matches!(request.externality, Externality::HostSystem)
                && matches!(request.capability, CapabilityId::SystemModify)
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
pub fn capability_for_effect_kind(kind: &str) -> Option<CapabilityId> {
    match kind {
        "WriteFile" | "ProjectWriteFile" | "Mkdir" | "ProjectMkdir" | "PatchFile"
        | "ProjectPatchFile" => Some(CapabilityId::FsProjectWrite),
        "RenamePath" | "ProjectRenamePath" => Some(CapabilityId::FsProjectRename),
        "DeletePath" | "ProjectDeletePath" => Some(CapabilityId::FsProjectDelete),
        "RunCommand" | "ProjectRunCommand" => Some(CapabilityId::ProcessProjectExecute),
        "AssertFileEquals" => Some(CapabilityId::FsProjectRead),
        _ => None,
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
        CapabilityId::FsProjectDelete => Reversibility::Checkpointed,
        // A dependency change is recoverable from the lockfile in git; running
        // arbitrary code and changing the host are not.
        CapabilityId::PackageSync | CapabilityId::PackageAdd => Reversibility::Checkpointed,
        CapabilityId::ProcessProjectExecute | CapabilityId::SystemModify => {
            Reversibility::Irreversible
        }
        _ => Reversibility::Reversible,
    };
    let externality = match class {
        Some(CommandClass::HostInstall) => Externality::HostSystem,
        Some(class) if class.reaches_registry() => Externality::PublicNetwork,
        _ => Externality::ProjectLocal,
    };
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
        },
        effect_digest: EffectDigest {
            sha256_hex: effect_hash.to_string(),
        },
        reversibility,
        sensitivity: Sensitivity::Ordinary,
        externality,
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
            },
            effect_digest: EffectDigest {
                sha256_hex: "a".repeat(64),
            },
            reversibility: Reversibility::Reversible,
            sensitivity: Sensitivity::Ordinary,
            externality: Externality::ProjectLocal,
        }
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
