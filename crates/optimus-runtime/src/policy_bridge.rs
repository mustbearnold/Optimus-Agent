//! ADR-0044 bridge: map graph autonomy profiles and effects onto the
//! `optimus-policy` capability broker.
//!
//! Split out of `lib.rs` so the runtime waist stays under the 800-line module
//! law (AGENTS.md law 21) instead of growing its grandfathered baseline.

use optimus_policy::{
    build_effect_request_for, build_owned_localhost_serve_request, AuthorityDecision,
    CapabilityBroker, OwnedLocalhostBinding,
};

use optimus_graph::GraphError;
use optimus_store::NewActionApproval;
use uuid::Uuid;

use crate::{
    mark_node_awaiting_approval, AutonomyProfile, Effect, JobId, PolicyMode, Result, Runtime,
    RuntimeError,
};

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
        Effect::ProjectServe {
            workspace_sha256,
            program,
            args,
            port,
            ttl_seconds,
        } => (
            "ProjectServe",
            Some(workspace_sha256.clone()),
            None,
            format!(
                "project serve {program} {} on 127.0.0.1:{port} for {ttl_seconds}s",
                args.join(" ")
            ),
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
        Effect::RunCommand { program, args }
        | Effect::ProjectRunCommand { program, args, .. }
        | Effect::ProjectServe { program, args, .. } => Some((program.as_str(), args.as_slice())),
        _ => None,
    }
}

/// The workspace scope a command effect is bound to, if project-scoped.
///
/// Bare `RunCommand` effects carry no scope (they run in the runtime's own
/// workspace root), so settlement falls back to the runtime workspace
/// identity for the consent lookup.
fn effect_workspace_scope(effect: &Effect) -> Option<String> {
    match effect {
        Effect::ProjectRunCommand {
            workspace_sha256, ..
        }
        | Effect::ProjectServe {
            workspace_sha256, ..
        } => Some(workspace_sha256.clone()),
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
        let profile = self.config.autonomy_profile;
        let (kind, root_hash, relative_path, summary) = effect_policy_view(effect);
        let mut request = build_effect_request_for(
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
        request.target.absolute_path = Some(
            request
                .target
                .relative_path
                .as_deref()
                .map_or_else(
                    || Ok(self.workspace_path().to_path_buf()),
                    |relative| self.effect_absolute_path(relative),
                )?
                .display()
                .to_string(),
        );
        request.developer_access = self.developer.grant.clone();
        Ok(CapabilityBroker.decide(profile, &request))
    }

    /// Broker decision over an owned-localhost lease the runtime has already
    /// proven (ADR-0060), returning the granting authority id.
    ///
    /// This is a second, separate decision from the effect gate above. The gate
    /// decides whether the serve process may start, and it necessarily runs
    /// before any listener exists, so it has no binding to present. This one
    /// decides whether the *lease* may be handed out, and it is the only place
    /// the proven binding is checked for internal coherence
    /// (`OwnedLocalhostBinding::is_valid_for` inside the broker).
    ///
    /// `Ask` is treated as satisfied rather than as a second pause. Reaching
    /// this point means the exact effect hash already carries a live approval,
    /// and that hash covers the program, arguments, port and TTL — so the human
    /// has already answered this exact question and asking again would only
    /// stall a process that is running.
    pub(crate) fn authorize_owned_localhost_lease(
        &self,
        effect_hash: &str,
        summary: String,
        binding: &OwnedLocalhostBinding,
    ) -> Result<Option<String>> {
        if self.config.policy != PolicyMode::SmartDeny {
            return Ok(None);
        }
        let request = build_owned_localhost_serve_request(
            effect_hash,
            self.owned_localhost_scope.clone(),
            summary,
            binding.clone(),
        );
        let mut request = request;
        request.target.absolute_path = Some(self.workspace_path().display().to_string());
        request.developer_access = self.developer.grant.clone();
        match CapabilityBroker.decide(self.config.autonomy_profile, &request) {
            AuthorityDecision::Allow { authority_id, .. } => Ok(Some(authority_id)),
            AuthorityDecision::Ask { .. } => Ok(None),
            AuthorityDecision::Deny { code, reason, .. } => {
                Err(RuntimeError::PolicyDenied { code, reason })
            }
            AuthorityDecision::Unavailable {
                capability, reason, ..
            } => Err(RuntimeError::PolicyDenied {
                code: "unavailable".into(),
                reason: format!("{}: {reason}", capability.as_str()),
            }),
        }
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
                let actor = self.config.autonomy_profile.trust_actor();
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
                // Session consent (spec-014 R7, ADR-0081): a live grant for
                // this session's (capability, CommandClass) pair under a live
                // Developer Full Access grant settles as an auto-grant with an
                // exact-effect audit row instead of pausing.
                if self.session_consent_auto_grants(job_id, node_id, effect, effect_hash)? {
                    return Ok(());
                }
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

    /// Whether a live session-consent grant covers `effect` (ADR-0081 R7).
    ///
    /// The class is re-derived via `classify_command` at settlement — never
    /// trusted from a stored string. OpaqueShell only matches the explicit
    /// (SystemModify, OpaqueShell) consent pair; every other class matches its
    /// own class grant. Scope and Developer Full Access liveness are
    /// revalidated at use time. When covered, the exact-effect audit row is
    /// written before returning `true`.
    fn session_consent_auto_grants(
        &self,
        job_id: JobId,
        node_id: Uuid,
        effect: &Effect,
        effect_hash: &str,
    ) -> Result<bool> {
        let Some(session_id) = self.config.consent_session_id.as_deref() else {
            return Ok(false);
        };
        // DFA liveness at use time (A5: DFA disable or terminal-execution
        // loss → ask again). Mirrors developer_runtime.rs: a grant is live
        // only while `enabled` AND `terminal_execution` both hold.
        if !self
            .developer
            .grant
            .as_ref()
            .is_some_and(|grant| grant.enabled && grant.capabilities.terminal_execution)
        {
            return Ok(false);
        }
        let Some((program, args)) = effect_command(effect) else {
            return Ok(false);
        };
        let class = optimus_policy::classify_command(program, args);
        // `class.capability()` already maps OpaqueShell → SystemModify, so the
        // explicit (SystemModify, OpaqueShell) pair is the natural key.
        let capability = class.capability().as_str();
        let scope = effect_workspace_scope(effect).unwrap_or_else(|| self.workspace_sha256());
        let now = Self::now_unix()?;
        if self
            .store
            .live_capability_grant(session_id, capability, class.as_str(), &scope, now)
            .map_err(GraphError::from)?
            .is_none()
        {
            return Ok(false);
        }
        // Auto-grant writes the exact-effect audit row, naming the consent.
        self.store
            .insert_action_approval(&NewActionApproval {
                id: Uuid::new_v4(),
                job_id: job_id.0,
                node_id,
                effect_hash: effect_hash.to_string(),
                actor: format!("session_consent:{}", class.as_str()),
                created_unix: now,
                expires_unix: now.saturating_add(3600),
            })
            .map_err(GraphError::from)?;
        Ok(true)
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
        let actor = AutonomyProfile::UnrestrictedHost.trust_actor();
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
