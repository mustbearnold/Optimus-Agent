//! Chat approval resolution: settling a paused tool call from the transcript.
//!
//! Split out of `lib.rs` under architectural law 21. Verbatim move — these stay
//! inherent `Kernel` methods, so no call site or visibility changed.
//!
//! Resolution records the decision and its tool result, then stops. It does not
//! finish the turn: the turn loop parked without finishing it, and the caller
//! resumes it from here so the request that provoked the approval gets answered
//! (ADR-0046).
//!
//! The invariant that matters is unchanged and is why the tool result is written
//! here rather than regenerated: **the approved call is never re-derived.** The
//! effect the user authorised is the effect that ran, and no model round trip
//! can substitute a different one. What the resumed turn decides is only what
//! happens *after* that fixed result.

use super::*;

/// bwrap's own chatter, which says nothing about why the command failed.
const SANDBOX_NOISE: &[&str] = &["Failed to create stream fd"];

/// A one-line reason from a failed effect's receipt, for the summary a human
/// reads on the tool row.
///
/// Observed: `Approved action failed: Run "bash" with args ["-lc","cu…` — the
/// user authorised a command, watched it fail, and was told only that it was
/// the command they had just seen. The reason (`curl: (6) Could not resolve
/// host`) was in the receipt the whole time. The last real stderr line is
/// where a shell puts its complaint; the exit code carries the rest.
fn failure_reason(receipt: Option<&Value>) -> Option<String> {
    let capture = receipt?.get("capture")?;
    let stderr = capture.get("stderr").and_then(Value::as_str).unwrap_or("");
    let last = stderr
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty() && !SANDBOX_NOISE.iter().any(|noise| line.contains(noise)));
    let exit = capture.get("exit_code").and_then(Value::as_i64);
    match (last, exit) {
        // Truncated so one runaway stderr line cannot push the summary off the
        // row it has to fit on.
        (Some(line), _) => Some(crate::compress::snippet_public(line, 160)),
        (None, Some(code)) => Some(format!("exit {code}")),
        (None, None) => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "decision")]
pub enum ChatApprovalDecision {
    Approve,
    Deny { reason: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatApprovalStatus {
    Approved,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatApprovalResolution {
    pub binding: ToolApprovalBinding,
    pub status: ChatApprovalStatus,
    pub event: ToolLifecycleEvent,
    /// R5: a second node re-parked on the same job while the first settled.
    /// The host must skip resuming the turn: the resolved call is finished,
    /// a new `{base}:node{n}` binding is pending, and the next card renders
    /// via record -> get_session projection -> reload.
    pub still_pending: bool,
    /// R5: the synthesized next binding (present when `still_pending`).
    pub pending_binding: Option<ToolApprovalBinding>,
    /// R5: the `ApprovalRequired` lifecycle event for the second card.
    pub pending_event: Option<ToolLifecycleEvent>,
}

impl Kernel {
    /// Resolve a transcript approval against the full persisted runtime identity.
    ///
    /// Records the tool result for the exact bound call and clears the pending
    /// approval. The accepted turn stays Running; resume it to get an answer.
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

        // The clock starts here, not when the tool was first called: the pause
        // in between is the human reading the card, and charging their thinking
        // time to the command would misreport every approved action.
        let settling = std::time::Instant::now();
        // R5: a multi-node job re-parks when a later node needs approval while
        // the settled node's effect already ran. The settled outcome is still
        // recorded as the bound call's result, that call's approval is
        // finished, and a new `{base}:node{n}` binding is synthesized. The
        // host skips resuming; the second card renders via the record.
        let mut re_parked = false;
        let (status, outcome, phase) = match decision {
            ChatApprovalDecision::Approve => {
                self.runtime
                    .grant_approval(ApprovalGrant::for_job(job_id))?;
                let job_status = self.runtime.resume(job_id)?;
                re_parked = job_status == JobStatus::AwaitingApproval;
                if !(re_parked || matches!(job_status, JobStatus::Succeeded | JobStatus::Failed)) {
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
                // The settled node's success derives from its effect status
                // alone when the job re-parked; the job status is no longer a
                // terminal verdict on this call (R5).
                let succeeded = effect.status == "succeeded";
                // What the effect produced, not merely that it ran. The turn
                // resumes from this result, and a model handed only a status
                // would narrate an outcome it never observed (ADR-0046).
                let receipt = effect
                    .receipt_json
                    .as_deref()
                    .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok());
                let mut outcome = if succeeded {
                    ToolOutcome::succeeded(
                        call.id.clone(),
                        descriptor.id.clone(),
                        format!("Completed: {}", binding.summary),
                        json!({
                            "ok": true,
                            "job": job_id.to_string(),
                            "status": "Succeeded",
                            "receipt": receipt,
                        }),
                        descriptor.replay,
                    )
                } else {
                    let mut failure = ToolOutcome::failed(
                        call.id.clone(),
                        descriptor.id.clone(),
                        format!(
                            "Approved action failed{}: {}",
                            failure_reason(receipt.as_ref())
                                .map(|reason| format!(" ({reason})"))
                                .unwrap_or_default(),
                            binding.summary
                        ),
                        ToolErrorDetail {
                            code: "approved_effect_failed".into(),
                            message: "The approved effect reached a failed terminal outcome."
                                .into(),
                            retryable: false,
                        },
                        descriptor.replay,
                    );
                    // The receipt says *why* it failed; that is the half worth
                    // resuming on.
                    failure.data = json!({
                        "ok": false,
                        "job": job_id.to_string(),
                        "status": "Failed",
                        "receipt": receipt,
                    });
                    failure
                };
                outcome.provenance = Some(DurableEffectProvenance {
                    job_id: effect.job_id.0,
                    node_id: effect.node_id,
                    effect_attempt_id: effect.attempt_id,
                    effect_sha256: effect.effect_hash,
                    receipt_sha256: effect.receipt_hash,
                });
                if succeeded {
                    self.enrich_workspace_tool_data(
                        descriptor.invocation,
                        &call.arguments,
                        &mut outcome.data,
                    );
                }
                if succeeded {
                    (
                        ChatApprovalStatus::Approved,
                        outcome,
                        ToolLifecyclePhase::Succeeded,
                    )
                } else {
                    (
                        ChatApprovalStatus::Approved,
                        outcome,
                        ToolLifecyclePhase::Failed,
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
                    "status": "Cancelled",
                    // The turn resumes on this, so the model can acknowledge the
                    // refusal or pick another route. Reporting the denial is the
                    // agent's to do, not the surface's (ADR-0046).
                    "denied_reason": reason,
                });
                (
                    ChatApprovalStatus::Denied,
                    outcome,
                    ToolLifecyclePhase::Cancelled,
                )
            }
        };

        descriptor.validate_outcome(&outcome)?;
        let duration_ms = turn_loop::elapsed_ms(settling);
        self.executions
            .record_tool_call(manifest_id, &call, &outcome, duration_ms, false)?;
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
            Some(duration_ms),
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
            reasoning_content: None,
        });
        // No assistant message is written here. Whatever is said about this
        // outcome is the agent's to say, once it has seen the result — the
        // product speaking in its voice about work it had not observed is the
        // thing ADR-0046 removed.
        self.sessions.save_with_effect_links(
            self.session_id,
            &self.session_title,
            &pack_names(&self.packs),
            &self.messages,
            &effect_links,
        )?;
        // The turn and its manifest stay Running, exactly as `run_turn_loop`
        // left them when it parked. Finishing them here is what stranded the
        // request that provoked the approval; the caller now resumes the turn
        // through `resume_pending_turn_with_sink`, and that path finishes it
        // once, when the model actually stops.
        self.executions
            .finish_chat_approval(manifest_id, call_id, status)?;
        if re_parked {
            // R5: the approved call settled; a later node on the same job
            // parked. Finish is done above; now synthesize the next card:
            // `{base}:node{n}`, a cloned ToolCall that is never nested inside
            // another call, recorded with its own approval row and lifecycle
            // event in ONE transaction. The turn is left Running, exactly as
            // the loop parked it, so the second card renders via the record
            // (get_session projection) and resolving it resumes the turn.
            let pending = self
                .runtime
                .list_pending_approvals()?
                .into_iter()
                .find(|pending| pending.job_id == job_id)
                .ok_or_else(|| {
                    KernelError::Model("re-parked job lost its next pending approval".into())
                })?;
            let node_id = pending.node_id.ok_or_else(|| {
                KernelError::Model("re-parked pending approval lost node identity".into())
            })?;
            let node_index = pending.node_index.ok_or_else(|| {
                KernelError::Model("re-parked pending approval lost node index".into())
            })?;
            let next_effect_sha256 =
                format!("{:x}", Sha256::digest(pending.effect_json.as_bytes()));
            let next_call_id = format!("{}:node{}", binding.call_id, node_index);
            // A clone of the original call, id replaced: the second card is the
            // same tool continuing, never a nested call (R5).
            let mut next_call = call.clone();
            next_call.id = next_call_id.clone();
            let next_summary = exact_action_summary(&pending.effect_json);
            let next_binding = ToolApprovalBinding {
                run_id: manifest_id,
                call_id: next_call_id,
                tool_id: binding.tool_id.clone(),
                job_id,
                node_id,
                node_index,
                effect_sha256: next_effect_sha256,
                summary: next_summary,
            };
            let mut next_event = tool_lifecycle_event(
                manifest_id,
                &next_call,
                binding.tool_id.clone(),
                ToolLifecyclePhase::ApprovalRequired,
                next_binding.summary.clone(),
                None,
                None,
            );
            next_event.approval = Some(next_binding.clone());
            self.executions.record_chat_approval_required(
                manifest_id,
                &next_call,
                &next_event,
                &next_binding,
            )?;
            return Ok(ChatApprovalResolution {
                binding: next_binding.clone(),
                status,
                event,
                still_pending: true,
                pending_binding: Some(next_binding),
                pending_event: Some(next_event),
            });
        }
        Ok(ChatApprovalResolution {
            binding,
            status,
            event,
            still_pending: false,
            pending_binding: None,
            pending_event: None,
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

#[cfg(test)]
mod failure_reason_tests {
    use super::failure_reason;
    use serde_json::json;

    fn receipt(exit: i64, stderr: &str) -> serde_json::Value {
        json!({ "capture": { "exit_code": exit, "stderr": stderr, "stdout": "" } })
    }

    #[test]
    fn the_shells_own_complaint_is_what_the_user_needs_to_read() {
        let r = receipt(
            6,
            "Failed to create stream fd: No such file or directory\n\
             curl: (6) Could not resolve host: github.com\n",
        );
        assert_eq!(
            failure_reason(Some(&r)).as_deref(),
            Some("curl: (6) Could not resolve host: github.com")
        );
    }

    #[test]
    fn sandbox_chatter_is_not_a_reason_and_never_stands_in_for_one() {
        // bwrap prints this on every confined spawn, successful or not.
        // Reporting it as the cause would be worse than reporting nothing.
        let r = receipt(6, "Failed to create stream fd: No such file or directory\n");
        assert_eq!(failure_reason(Some(&r)).as_deref(), Some("exit 6"));
    }

    #[test]
    fn a_silent_failure_still_names_its_exit_code() {
        assert_eq!(
            failure_reason(Some(&receipt(127, ""))).as_deref(),
            Some("exit 127")
        );
    }

    #[test]
    fn no_receipt_means_no_invented_reason() {
        assert_eq!(failure_reason(None), None);
        assert_eq!(failure_reason(Some(&json!({}))), None);
    }

    #[test]
    fn a_runaway_stderr_line_cannot_push_the_summary_off_the_row() {
        let r = receipt(1, &"x".repeat(500));
        let reason = failure_reason(Some(&r)).unwrap();
        assert!(reason.chars().count() <= 160, "{}", reason.chars().count());
        assert!(reason.ends_with('…'));
    }
}
