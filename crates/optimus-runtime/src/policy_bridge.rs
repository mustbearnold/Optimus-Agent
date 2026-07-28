//! ADR-0044 bridge: map graph autonomy profiles and effects onto the
//! `optimus-policy` capability broker.
//!
//! Split out of `lib.rs` so the runtime waist stays under the 800-line module
//! law (AGENTS.md law 21) instead of growing its grandfathered baseline.

use optimus_policy::{
    build_effect_request_for, AuthorityDecision, AutonomyProfile as PolicyAutonomy,
    CapabilityBroker,
};

use optimus_graph::GraphError;
use optimus_store::NewActionApproval;
use uuid::Uuid;

use crate::{
    mark_node_awaiting_approval, AutonomyProfile, Effect, JobId, Result, Runtime, RuntimeError,
};

fn policy_autonomy(profile: AutonomyProfile) -> PolicyAutonomy {
    match profile {
        AutonomyProfile::Standard => PolicyAutonomy::Standard,
        AutonomyProfile::ReviewChanges => PolicyAutonomy::ReviewChanges,
        AutonomyProfile::ReadOnly => PolicyAutonomy::ReadOnly,
        AutonomyProfile::FullProject => PolicyAutonomy::FullProject,
        AutonomyProfile::UnrestrictedHost => PolicyAutonomy::UnrestrictedHost,
    }
}

/// Stable effect kind name + audit fields for the capability broker.
fn effect_policy_view(effect: &Effect) -> (&'static str, Option<String>, Option<String>, String) {
    match effect {
        Effect::WriteFile {
            relative_path,
            contents,
        } => (
            "WriteFile",
            None,
            Some(relative_path.clone()),
            format!("write {relative_path} ({} bytes)", contents.len()),
        ),
        Effect::ProjectWriteFile {
            workspace_sha256,
            relative_path,
            contents,
        } => (
            "ProjectWriteFile",
            Some(workspace_sha256.clone()),
            Some(relative_path.clone()),
            format!("project write {relative_path} ({} bytes)", contents.len()),
        ),
        Effect::Mkdir { relative_path } => (
            "Mkdir",
            None,
            Some(relative_path.clone()),
            format!("mkdir {relative_path}"),
        ),
        Effect::ProjectMkdir {
            workspace_sha256,
            relative_path,
        } => (
            "ProjectMkdir",
            Some(workspace_sha256.clone()),
            Some(relative_path.clone()),
            format!("project mkdir {relative_path}"),
        ),
        Effect::DeletePath { relative_path } => (
            "DeletePath",
            None,
            Some(relative_path.clone()),
            format!("delete {relative_path}"),
        ),
        Effect::ProjectDeletePath {
            workspace_sha256,
            relative_path,
        } => (
            "ProjectDeletePath",
            Some(workspace_sha256.clone()),
            Some(relative_path.clone()),
            format!("project delete {relative_path}"),
        ),
        Effect::RenamePath {
            from_relative_path,
            to_relative_path,
        } => (
            "RenamePath",
            None,
            Some(from_relative_path.clone()),
            format!("rename {from_relative_path} → {to_relative_path}"),
        ),
        Effect::ProjectRenamePath {
            workspace_sha256,
            from_relative_path,
            to_relative_path,
        } => (
            "ProjectRenamePath",
            Some(workspace_sha256.clone()),
            Some(from_relative_path.clone()),
            format!("project rename {from_relative_path} → {to_relative_path}"),
        ),
        Effect::PatchFile {
            relative_path,
            old_string,
            new_string,
        } => (
            "PatchFile",
            None,
            Some(relative_path.clone()),
            format!(
                "patch {relative_path} ({}→{} bytes)",
                old_string.len(),
                new_string.len()
            ),
        ),
        Effect::ProjectPatchFile {
            workspace_sha256,
            relative_path,
            old_string,
            new_string,
        } => (
            "ProjectPatchFile",
            Some(workspace_sha256.clone()),
            Some(relative_path.clone()),
            format!(
                "project patch {relative_path} ({}→{} bytes)",
                old_string.len(),
                new_string.len()
            ),
        ),
        Effect::AssertFileEquals {
            relative_path,
            expected,
        } => (
            "AssertFileEquals",
            None,
            Some(relative_path.clone()),
            format!("assert {relative_path} ({} bytes)", expected.len()),
        ),
        Effect::RunCommand { program, args } => (
            "RunCommand",
            None,
            None,
            format!("run {program} {}", args.join(" ")),
        ),
        Effect::ProjectRunCommand {
            workspace_sha256,
            program,
            args,
        } => (
            "ProjectRunCommand",
            Some(workspace_sha256.clone()),
            None,
            format!("project run {program} {}", args.join(" ")),
        ),
    }
}

/// The program and arguments behind a command effect, for the classifier.
///
/// `run <something>` is not a capability until you know what the something is:
/// `cargo test`, `cargo add some-crate` and `npm install -g` are three
/// different acts that were all `ProcessProjectExecute` before this.
fn effect_command(effect: &Effect) -> Option<(&str, &[String])> {
    match effect {
        Effect::RunCommand { program, args } | Effect::ProjectRunCommand { program, args, .. } => {
            Some((program.as_str(), args.as_slice()))
        }
        _ => None,
    }
}

impl Runtime {
    /// Capability-broker decision for a high-risk effect under SmartDeny (ADR-0044).
    fn authorize_high_risk_effect(
        &self,
        effect: &Effect,
        effect_hash: &str,
    ) -> Result<AuthorityDecision> {
        let profile = policy_autonomy(self.config.autonomy_profile);
        let (kind, root_hash, relative_path, summary) = effect_policy_view(effect);
        let request = build_effect_request_for(
            kind,
            effect_hash,
            root_hash,
            summary,
            relative_path,
            effect_command(effect),
        )
        .ok_or_else(|| {
            RuntimeError::Effector(format!("no capability mapping for effect kind {kind}"))
        })?;
        Ok(CapabilityBroker.decide(profile, &request))
    }

    /// Settle the broker decision for a high-risk effect: record a durable trust
    /// grant, pause for approval, or deny (ADR-0044).
    pub(crate) fn settle_high_risk_authority(
        &self,
        job_id: JobId,
        node_id: Uuid,
        node_idx: u32,
        effect: &Effect,
        effect_hash: &str,
    ) -> Result<()> {
        match self.authorize_high_risk_effect(effect, effect_hash)? {
            AuthorityDecision::Allow { .. } => {
                // Durable exact-effect trust grant (audit without human pause).
                let now = Self::now_unix()?;
                let actor = policy_autonomy(self.config.autonomy_profile).trust_actor();
                self.store
                    .insert_action_approval(&NewActionApproval {
                        id: Uuid::new_v4(),
                        job_id: job_id.0,
                        node_id,
                        effect_hash: effect_hash.to_string(),
                        actor,
                        created_unix: now,
                        expires_unix: now.saturating_add(3600),
                    })
                    .map_err(GraphError::from)?;
            }
            AuthorityDecision::Ask { .. } => {
                mark_node_awaiting_approval(&self.store, job_id, node_id)?;
                return Err(RuntimeError::NeedsApproval {
                    job_id,
                    node_index: node_idx,
                });
            }
            AuthorityDecision::Deny { code, reason, .. } => {
                return Err(RuntimeError::PolicyDenied { code, reason });
            }
            AuthorityDecision::Unavailable {
                capability, reason, ..
            } => {
                return Err(RuntimeError::PolicyDenied {
                    code: "unavailable".into(),
                    reason: format!("{}: {reason}", capability.as_str()),
                });
            }
        }
        Ok(())
    }

    /// Release approvals that are open right now because the operator activated
    /// yolo (`UnrestrictedHost`).
    ///
    /// Typing yolo mid-pause is a request to unblock the thing on screen, so the
    /// pause is dropped — but each release still writes a durable action-approval
    /// receipt naming the yolo actor rather than a human. ADR-0044 requires every
    /// exact action to stay recorded; only the human gate is waived.
    ///
    /// Returns the number of approvals released.
    pub fn release_open_approvals_under_yolo(&self) -> Result<usize> {
        let actor = PolicyAutonomy::UnrestrictedHost.trust_actor();
        let now = Self::now_unix()?;
        let mut released = 0usize;
        for pending in self.list_pending_approvals()? {
            let Some(node_id) = pending.node_id else {
                continue;
            };
            if pending.has_grant {
                continue;
            }
            self.store
                .insert_action_approval(&NewActionApproval {
                    id: Uuid::new_v4(),
                    job_id: pending.job_id.0,
                    node_id,
                    effect_hash: Self::effect_hash(&pending.effect_json),
                    actor: actor.clone(),
                    created_unix: now,
                    expires_unix: now.saturating_add(3600),
                })
                .map_err(GraphError::from)?;
            released += 1;
        }
        Ok(released)
    }
}
