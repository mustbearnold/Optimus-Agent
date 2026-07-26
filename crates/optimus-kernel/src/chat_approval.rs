//! Chat approval resolution: settling a paused tool call from the transcript.
//!
//! Split out of `lib.rs` under architectural law 21. Verbatim move — these stay
//! inherent `Kernel` methods, so no call site or visibility changed.
//!
//! Invariant preserved from the original: resolution settles the accepted turn
//! deterministically with a tool result and an assistant receipt. It never asks
//! a provider to regenerate the paused call.

use super::*;

impl Kernel {
    /// Resolve a transcript approval against the full persisted runtime identity.
    ///
    /// This deterministically settles the accepted turn with a tool result and an
    /// assistant receipt. It never asks a provider to regenerate the paused call.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_chat_approval_exact(
        &mut self,
        run_id: Uuid,
        call_id: &str,
        job_id: JobId,
        expected_node_id: Uuid,
        expected_node_index: u32,
        expected_effect_sha256: &str,
        decision: ChatApprovalDecision,
    ) -> Result<ChatApprovalResolution> {
        if call_id.trim().is_empty()
            || expected_effect_sha256.len() != 64
            || !expected_effect_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(KernelError::Model(
                "chat approval requires a call id and 64-hex effect identity".into(),
            ));
        }
        if let ChatApprovalDecision::Deny { reason } = &decision {
            if reason.trim().is_empty() || reason.len() > 1024 {
                return Err(KernelError::Model(
                    "chat approval denial requires a bounded reason".into(),
                ));
            }
        }
        let turn = self
            .sessions
            .active_turn(self.session_id)?
            .ok_or_else(|| KernelError::Model("session has no approval-paused turn".into()))?;
        let manifest_id = self.executions.find_by_turn(turn.id)?.ok_or_else(|| {
            KernelError::Model("approval-paused turn has no execution manifest".into())
        })?;
        if manifest_id != run_id {
            return Err(KernelError::Model(
                "chat approval run identity is foreign to the active turn".into(),
            ));
        }
        let manifest = self.executions.manifest(manifest_id)?;
        if manifest.session_id != self.session_id
            || manifest.turn_id != turn.id
            || manifest.status != ExecutionStatus::Running
        {
            return Err(KernelError::Model(
                "chat approval execution is foreign or already terminal".into(),
            ));
        }
        let (binding, call) = self
            .executions
            .pending_chat_approval(manifest_id, call_id)?
            .ok_or_else(|| {
                KernelError::Model(format!(
                    "chat approval is missing or already resolved: {call_id}"
                ))
            })?;
        if binding.run_id != run_id
            || binding.call_id != call_id
            || binding.job_id != job_id
            || binding.node_id != expected_node_id
            || binding.node_index != expected_node_index
            || binding.effect_sha256 != expected_effect_sha256
        {
            return Err(KernelError::Model(
                "chat approval identity does not match the exact pending call".into(),
            ));
        }
        let descriptor = self.packs.resolve_loaded_tool(&call.name)?.clone();
        if descriptor.id != binding.tool_id {
            return Err(KernelError::Model(
                "chat approval tool identity changed while paused".into(),
            ));
        }
        let pending = self
            .runtime
            .list_pending_approvals()?
            .into_iter()
            .find(|pending| pending.job_id == job_id)
            .ok_or_else(|| {
                KernelError::Model("runtime no longer has the exact pending approval".into())
            })?;
        let current_node_id = pending.node_id.ok_or_else(|| {
            KernelError::Model("runtime pending approval lost node identity".into())
        })?;
        let current_node_index = pending
            .node_index
            .ok_or_else(|| KernelError::Model("runtime pending approval lost node index".into()))?;
        let current_effect_sha256 = format!("{:x}", Sha256::digest(pending.effect_json.as_bytes()));
        if current_node_id != binding.node_id
            || current_node_index != binding.node_index
            || current_effect_sha256 != binding.effect_sha256
        {
            return Err(KernelError::Model(
                "runtime approval target changed while paused".into(),
            ));
        }

        let (status, outcome, phase, assistant_receipt, turn_status, error_code) = match decision {
            ChatApprovalDecision::Approve => {
                self.runtime
                    .grant_approval(ApprovalGrant::for_job(job_id))?;
                let job_status = self.runtime.resume(job_id)?;
                if !matches!(job_status, JobStatus::Succeeded | JobStatus::Failed) {
                    return Err(KernelError::Model(format!(
                        "approved job did not reach a terminal outcome: {job_status:?}"
                    )));
                }
                let effect = self.runtime.latest_effect_outcome(job_id)?.ok_or_else(|| {
                    KernelError::Model(
                        "approved job completed without terminal effect provenance".into(),
                    )
                })?;
                if effect.node_id != binding.node_id || effect.effect_hash != binding.effect_sha256
                {
                    return Err(KernelError::Model(
                        "approved effect provenance does not match the pending binding".into(),
                    ));
                }
                let succeeded = job_status == JobStatus::Succeeded && effect.status == "succeeded";
                let mut outcome = if succeeded {
                    ToolOutcome::succeeded(
                        call.id.clone(),
                        descriptor.id.clone(),
                        format!("Completed: {}", binding.summary),
                        json!({
                            "ok": true,
                            "job": job_id.to_string(),
                            "status": "Succeeded"
                        }),
                        descriptor.replay,
                    )
                } else {
                    ToolOutcome::failed(
                        call.id.clone(),
                        descriptor.id.clone(),
                        format!("Approved action failed: {}", binding.summary),
                        ToolErrorDetail {
                            code: "approved_effect_failed".into(),
                            message: "The approved effect reached a failed terminal outcome."
                                .into(),
                            retryable: false,
                        },
                        descriptor.replay,
                    )
                };
                outcome.provenance = Some(DurableEffectProvenance {
                    job_id: effect.job_id.0,
                    node_id: effect.node_id,
                    effect_attempt_id: effect.attempt_id,
                    effect_sha256: effect.effect_hash,
                    receipt_sha256: effect.receipt_hash,
                });
                if succeeded {
                    (
                        ChatApprovalStatus::Approved,
                        outcome,
                        ToolLifecyclePhase::Succeeded,
                        format!("Approved and completed: {}.", binding.summary),
                        TurnStatus::Succeeded,
                        None,
                    )
                } else {
                    (
                        ChatApprovalStatus::Approved,
                        outcome,
                        ToolLifecyclePhase::Failed,
                        format!("Approved action failed: {}.", binding.summary),
                        TurnStatus::Failed,
                        Some("approved_effect_failed"),
                    )
                }
            }
            ChatApprovalDecision::Deny { reason } => {
                self.runtime
                    .deny_approval(ApprovalGrant::for_job(job_id), &reason)?;
                let job_status = self.runtime.cancel_job(job_id)?;
                if job_status != JobStatus::Cancelled {
                    return Err(KernelError::Model(format!(
                        "denied job did not cancel: {job_status:?}"
                    )));
                }
                let mut outcome = ToolOutcome::failed(
                    call.id.clone(),
                    descriptor.id.clone(),
                    format!("Denied: {}", binding.summary),
                    ToolErrorDetail {
                        code: "approval_denied".into(),
                        message: "The user denied this exact action.".into(),
                        retryable: false,
                    },
                    descriptor.replay,
                );
                outcome.kind = ToolOutcomeKind::Cancelled;
                outcome.data = json!({
                    "ok": false,
                    "approval_job": job_id.to_string(),
                    "status": "Cancelled"
                });
                (
                    ChatApprovalStatus::Denied,
                    outcome,
                    ToolLifecyclePhase::Cancelled,
                    format!("Denied: {}.", binding.summary),
                    TurnStatus::Cancelled,
                    Some("approval_denied"),
                )
            }
        };

        descriptor.validate_outcome(&outcome)?;
        self.executions
            .record_tool_call(manifest_id, &call, &outcome, 0, false)?;
        let result_json = serde_json::to_string(&outcome)?;
        let effect_links = if status == ChatApprovalStatus::Approved && outcome.provenance.is_some()
        {
            self.effect_link_for_tool_result(&call, &result_json)?
        } else {
            Vec::new()
        };
        let mut event = tool_lifecycle_event(
            manifest_id,
            &call,
            descriptor.id,
            phase,
            outcome.summary.clone(),
            Some(0),
            Some(outcome.clone()),
        );
        event.approval = Some(binding.clone());
        self.executions
            .record_tool_lifecycle_event(manifest_id, &event)?;
        self.messages.push(Message {
            role: Role::Tool,
            content: result_json,
            tool_call_id: Some(call.id.clone()),
            name: Some(call.name.clone()),
        });
        self.messages.push(Message {
            role: Role::Assistant,
            content: assistant_receipt.clone(),
            tool_call_id: None,
            name: None,
        });
        self.sessions.save_with_effect_links(
            self.session_id,
            &self.session_title,
            &pack_names(&self.packs),
            &self.messages,
            &effect_links,
        )?;
        self.sessions.finish_turn(
            turn.id,
            self.session_id,
            &self.session_title,
            &pack_names(&self.packs),
            &self.messages,
            turn_status,
            error_code,
        )?;
        let timing = self.executions.timing_summary(manifest_id)?;
        let total_ms = timing.model_ms.saturating_add(timing.tool_ms);
        self.executions.finish_timed(
            manifest_id,
            match turn_status {
                TurnStatus::Succeeded => ExecutionStatus::Succeeded,
                TurnStatus::Failed => ExecutionStatus::Failed,
                TurnStatus::Cancelled => ExecutionStatus::Cancelled,
                TurnStatus::Running => unreachable!("approval settlement is terminal"),
            },
            total_ms,
        )?;
        self.executions.record_timing_event(
            manifest_id,
            &TimingEvent {
                kind: TimingEventKind::TurnFinished,
                step: None,
                call_id: Some(call.id.clone()),
                name: Some(call.name.clone()),
                duration_ms: Some(0),
                elapsed_ms: total_ms,
                status: Some(
                    match turn_status {
                        TurnStatus::Succeeded => "succeeded",
                        TurnStatus::Failed => "failed",
                        TurnStatus::Cancelled => "cancelled",
                        TurnStatus::Running => unreachable!("approval settlement is terminal"),
                    }
                    .into(),
                ),
                suppressed: false,
            },
        )?;
        self.executions
            .finish_chat_approval(manifest_id, call_id, status)?;
        Ok(ChatApprovalResolution {
            binding,
            status,
            event,
            assistant_receipt,
        })
    }

    pub fn resolve_chat_approval(
        &mut self,
        run_id: Uuid,
        call_id: &str,
        job_id: JobId,
        decision: ChatApprovalDecision,
    ) -> Result<ChatApprovalResolution> {
        let (binding, _) = self
            .executions
            .pending_chat_approval(run_id, call_id)?
            .ok_or_else(|| {
                KernelError::Model(format!(
                    "chat approval is missing or already resolved: {call_id}"
                ))
            })?;
        let effect_sha256 = binding.effect_sha256.clone();
        self.resolve_chat_approval_exact(
            run_id,
            call_id,
            job_id,
            binding.node_id,
            binding.node_index,
            &effect_sha256,
            decision,
        )
    }
}
