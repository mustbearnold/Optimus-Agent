//! The recorded turn loop: execution manifest lifecycle, provider round trips,
//! tool-call iteration, and terminal outcome settlement.
//!
//! Split out of `lib.rs` under architectural law 21. Verbatim move — these stay
//! inherent `Kernel` methods, so no call site or visibility changed. Kept
//! separate from `execution.rs`, which owns the manifest store rather than the
//! loop that drives it.

use super::*;

impl Kernel {
    pub(crate) fn begin_execution_manifest(
        &self,
        turn_id: Uuid,
        model: &dyn ModelProvider,
        prompt: &str,
    ) -> Result<RecordedExecution> {
        let (provider, model_id) = model.identity();
        let tools = serde_json::to_vec(&self.tool_schemas())?;
        let policy = format!(
            "max_steps={};max_tool_calls_per_step={};schema_budget={};fast={};thinking={:?}",
            self.config.max_steps,
            self.config.max_tool_calls_per_step,
            self.config.pack_budget.max_schema_tokens,
            self.config.fast_mode,
            self.config.thinking_level
        );
        let (manifest_id, trace_context) = self.executions.begin_traced(
            self.session_id,
            turn_id,
            &provider,
            &model_id,
            prompt.as_bytes(),
            &tools,
            policy.as_bytes(),
        )?;
        Ok(RecordedExecution {
            manifest_id,
            trace_context,
        })
    }

    pub(crate) fn run_recorded_turn(
        &mut self,
        model: &mut dyn ModelProvider,
        sink: &mut dyn FnMut(StreamEvent),
        cancellation: &CancellationToken,
        turn_id: Uuid,
        execution: RecordedExecution,
    ) -> Result<TurnResult> {
        let started = Instant::now();
        let mut timings = TimingAccumulator::default();
        let start_event = timing_event(TimingEventKind::TurnStarted, started, None, None, None);
        self.executions
            .record_timing_event(execution.manifest_id, &start_event)?;
        sink(StreamEvent::Timing(start_event));
        let mut result =
            self.run_turn_loop(model, sink, cancellation, execution, started, &mut timings);
        let total_ms = elapsed_ms(started);
        if let Ok(turn) = &mut result {
            turn.timings = TurnTimings {
                total_ms,
                first_response_ms: timings.first_response_ms,
                model_ms: timings.model_ms,
                tool_ms: timings.tool_ms,
            };
        }
        // SmartDeny is a durable pause, not a terminal failure. The accepted turn and
        // execution manifest remain running until the exact bound call is resolved.
        if matches!(
            result,
            Err(KernelError::Runtime(RuntimeError::NeedsApproval { .. }))
        ) {
            return result;
        }
        let (status, error_code) = match &result {
            Ok(_) => (TurnStatus::Succeeded, None),
            Err(KernelError::Cancelled) => (TurnStatus::Cancelled, Some("turn_cancelled")),
            Err(error) => (TurnStatus::Failed, Some(kernel_error_code(error))),
        };
        self.sessions.finish_turn(
            turn_id,
            self.session_id,
            &self.session_title,
            &pack_names(&self.packs),
            &self.messages,
            status,
            error_code,
        )?;
        self.executions.finish_timed(
            execution.manifest_id,
            match status {
                TurnStatus::Succeeded => ExecutionStatus::Succeeded,
                TurnStatus::Failed => ExecutionStatus::Failed,
                TurnStatus::Cancelled => ExecutionStatus::Cancelled,
                TurnStatus::Running => unreachable!("turn settlement is terminal"),
            },
            total_ms,
        )?;
        let terminal_status = match status {
            TurnStatus::Succeeded => "succeeded",
            TurnStatus::Failed => "failed",
            TurnStatus::Cancelled => "cancelled",
            TurnStatus::Running => unreachable!("turn settlement is terminal"),
        }
        .to_string();
        let mut terminal = timing_event(
            TimingEventKind::TurnFinished,
            started,
            Some(total_ms),
            None,
            None,
        );
        terminal.status = Some(terminal_status);
        self.executions
            .record_timing_event(execution.manifest_id, &terminal)?;
        sink(StreamEvent::Timing(terminal));
        result
    }

    pub(crate) fn run_turn_loop(
        &mut self,
        model: &mut dyn ModelProvider,
        sink: &mut dyn FnMut(StreamEvent),
        cancellation: &CancellationToken,
        execution: RecordedExecution,
        turn_started: Instant,
        timings: &mut TimingAccumulator,
    ) -> Result<TurnResult> {
        let mut steps = 0u32;
        let mut tool_trace = Vec::new();
        let mut invoked_tools = Vec::new();
        let mut compressed = false;
        let mut evidence_signatures = BTreeSet::new();
        let mut synthesis_only = false;

        loop {
            check_cancellation(cancellation)?;
            if steps >= self.config.max_steps {
                let _ = self.save_session();
                return Err(KernelError::MaxSteps(self.config.max_steps));
            }
            steps += 1;

            if compress::compress_messages(&mut self.messages, &self.config.compression) {
                compressed = true;
            }

            let tools = if synthesis_only {
                Vec::new()
            } else {
                self.tool_schemas()
            };
            let advertised_tool_ids: BTreeSet<ToolId> =
                tools.iter().map(|tool| tool.id.clone()).collect();
            let effort = apply_fast_mode(
                normalize_thinking_level(self.config.thinking_level.as_deref()),
                self.config.fast_mode,
            );
            let mut request_messages = self.messages.clone();
            if synthesis_only {
                request_messages.push(Message {
                    role: Role::System,
                    content: "Tool-loop guard active: synthesis-only step. Answer from the evidence already present and do not request more tools.".into(),
                    tool_call_id: None,
                    name: None,
                });
            }
            let req = CompletionRequest {
                messages: request_messages,
                tools,
                reasoning_effort: effort,
                fast_mode: self.config.fast_mode,
            };
            let recorded_request = req.clone();
            sink(StreamEvent::Status(format!("model step {steps}")));
            // program P24: thinking stream is separate from assistant TextDelta.
            if let Some(level) = self.config.thinking_level.as_deref() {
                sink(StreamEvent::ThinkingDelta(format!(
                    "Reasoning effort: {level} (step {steps})\n"
                )));
            }
            let model_started = Instant::now();
            let model_start_event = timing_event(
                TimingEventKind::ModelStarted,
                turn_started,
                None,
                Some(steps),
                None,
            );
            self.executions
                .record_timing_event(execution.manifest_id, &model_start_event)?;
            sink(StreamEvent::Timing(model_start_event));
            let observe_first_response = timings.first_response_ms.is_none();
            let first_observed = Cell::new(false);
            let first_elapsed = Cell::new(None);
            let mut timed_sink = |event| {
                if observe_first_response && !first_observed.replace(true) {
                    let elapsed = elapsed_ms(turn_started);
                    first_elapsed.set(Some(elapsed));
                    let first_event = timing_event(
                        TimingEventKind::FirstResponse,
                        turn_started,
                        None,
                        Some(steps),
                        None,
                    );
                    sink(StreamEvent::Timing(first_event));
                }
                sink(event);
            };
            let response = model.complete_streaming_cancellable(req, &mut timed_sink, cancellation);
            if observe_first_response && !first_observed.get() && response.is_ok() {
                let elapsed = elapsed_ms(turn_started);
                first_elapsed.set(Some(elapsed));
                let first_event = timing_event(
                    TimingEventKind::FirstResponse,
                    turn_started,
                    None,
                    Some(steps),
                    None,
                );
                self.executions
                    .record_timing_event(execution.manifest_id, &first_event)?;
                sink(StreamEvent::Timing(first_event));
            } else if observe_first_response && first_observed.get() {
                let mut first_event = timing_event(
                    TimingEventKind::FirstResponse,
                    turn_started,
                    None,
                    Some(steps),
                    None,
                );
                first_event.elapsed_ms = first_elapsed.get().unwrap_or_default();
                self.executions
                    .record_timing_event(execution.manifest_id, &first_event)?;
            }
            if observe_first_response {
                timings.first_response_ms = first_elapsed.get();
            }
            let model_duration_ms = elapsed_ms(model_started);
            timings.model_ms = timings.model_ms.saturating_add(model_duration_ms);
            let mut model_finish_event = timing_event(
                TimingEventKind::ModelFinished,
                turn_started,
                Some(model_duration_ms),
                Some(steps),
                None,
            );
            model_finish_event.status = Some(
                match &response {
                    Ok(_) => "succeeded",
                    Err(KernelError::Cancelled) => "cancelled",
                    Err(_) => "failed",
                }
                .into(),
            );
            self.executions
                .record_timing_event(execution.manifest_id, &model_finish_event)?;
            sink(StreamEvent::Timing(model_finish_event));
            let resp = response?;
            let (provider, model_id) = model.identity();
            self.executions.record_model_call(
                execution.manifest_id,
                steps,
                (&provider, &model_id),
                &recorded_request,
                &resp,
                model_duration_ms,
            )?;

            if !resp.tool_calls.is_empty() {
                if resp.tool_calls.len() > HARD_MAX_TOOL_CALLS_PER_STEP {
                    return Err(KernelError::Model(format!(
                        "model returned {} tool calls; hard per-step limit is {}",
                        resp.tool_calls.len(),
                        HARD_MAX_TOOL_CALLS_PER_STEP
                    )));
                }
                // Validate the entire response against the exact schema set advertised for
                // this model step before any sibling call can mutate state or perform effects.
                let mut seen_call_ids = BTreeSet::new();
                for call in &resp.tool_calls {
                    let provider_call_id = call.id.trim();
                    if provider_call_id.is_empty() {
                        return Err(KernelError::Model("tool_call missing non-empty id".into()));
                    }
                    if !seen_call_ids.insert(provider_call_id.to_string()) {
                        return Err(KernelError::Model(format!(
                            "tool_call has duplicate id: {provider_call_id}"
                        )));
                    }
                    if call.name.trim().is_empty() {
                        return Err(KernelError::Model("tool_call missing function name".into()));
                    }
                    let call_id = ToolId::new(&call.name);
                    if !advertised_tool_ids.contains(&call_id) {
                        match self.packs.resolve_loaded_tool(&call.name) {
                            Err(error @ PackError::UnknownTool(_))
                            | Err(error @ PackError::ToolUnavailable(_)) => {
                                return Err(error.into());
                            }
                            _ => {
                                return Err(PackError::ToolNotAdvertised(call.name.clone()).into());
                            }
                        }
                    }
                    self.packs
                        .resolve_loaded_tool(&call.name)?
                        .validate_arguments(&call.arguments)?;
                }
                self.messages.push(Message {
                    role: Role::Assistant,
                    content: serde_json::to_string(&resp.tool_calls)?,
                    tool_call_id: None,
                    name: None,
                });
                let execution_budget = self.config.max_tool_calls_per_step.max(1);
                for (call_index, call) in resp.tool_calls.into_iter().enumerate() {
                    check_cancellation(cancellation)?;
                    let descriptor = self.packs.resolve_loaded_tool(&call.name)?.clone();
                    let over_budget = call_index >= execution_budget;
                    let duplicate = if over_budget {
                        false
                    } else {
                        evidence_tool_signature(&call)
                            .is_some_and(|value| !evidence_signatures.insert(value))
                    };
                    let suppressed = over_budget || duplicate;
                    if suppressed {
                        synthesis_only = true;
                    }
                    let tool_started = Instant::now();
                    let start_event = timing_event(
                        TimingEventKind::ToolStarted,
                        turn_started,
                        None,
                        Some(steps),
                        Some(&call),
                    );
                    self.executions
                        .record_timing_event(execution.manifest_id, &start_event)?;
                    sink(StreamEvent::Timing(start_event));
                    let lifecycle = tool_lifecycle_event(
                        execution.manifest_id,
                        &call,
                        descriptor.id.clone(),
                        ToolLifecyclePhase::Started,
                        if over_budget {
                            "Checking tool-call budget"
                        } else if duplicate {
                            "Checking duplicate tool evidence"
                        } else {
                            "Running"
                        },
                        None,
                        None,
                    );
                    self.executions
                        .record_tool_lifecycle_event(execution.manifest_id, &lifecycle)?;
                    sink(StreamEvent::Tool(Box::new(lifecycle)));
                    let (tool_id, mut outcome) = if suppressed {
                        (
                            descriptor.id.clone(),
                            ToolOutcome::failed(
                                call.id.clone(),
                                descriptor.id.clone(),
                                format!("{} call suppressed", descriptor.id.as_str()),
                                ToolErrorDetail {
                                    code: if over_budget {
                                        "tool_call_budget_suppressed"
                                    } else {
                                        "duplicate_tool_call_suppressed"
                                    }
                                    .into(),
                                    message: if over_budget {
                                        "The turn has enough tool evidence; synthesize from completed calls without another effect."
                                    } else {
                                        "Equivalent evidence already exists in this turn; synthesize from it without another tool call."
                                    }
                                    .into(),
                                    retryable: false,
                                },
                                descriptor.replay,
                            ),
                        )
                    } else {
                        match self.dispatch_tool(&call) {
                            Ok((tool_id, result)) => {
                                let summary =
                                    format!("{}: {}", descriptor.id.as_str(), summarize(&result));
                                let data = serde_json::from_str(&result)
                                    .unwrap_or_else(|_| json!({"text": result}));
                                (
                                    tool_id,
                                    ToolOutcome::succeeded(
                                        call.id.clone(),
                                        descriptor.id.clone(),
                                        summary,
                                        data,
                                        descriptor.replay,
                                    ),
                                )
                            }
                            Err(KernelError::Packs(ref pack_error)) => (
                                descriptor.id.clone(),
                                pack_error_tool_outcome(&call, &descriptor, pack_error),
                            ),
                            Err(
                                error @ KernelError::Runtime(RuntimeError::NeedsApproval {
                                    job_id,
                                    node_index,
                                }),
                            ) => {
                                let tool_duration_ms = elapsed_ms(tool_started);
                                let pending = self
                                    .runtime
                                    .list_pending_approvals()?
                                    .into_iter()
                                    .find(|pending| {
                                        pending.job_id == job_id
                                            && pending.node_index == Some(node_index)
                                    })
                                    .ok_or_else(|| {
                                        KernelError::Model(format!(
                                            "runtime approval identity disappeared for job {job_id} node {node_index}"
                                        ))
                                    })?;
                                let node_id = pending.node_id.ok_or_else(|| {
                                    KernelError::Model(format!(
                                        "runtime approval is missing node identity for job {job_id}"
                                    ))
                                })?;
                                if pending.effect_json.is_empty() {
                                    return Err(KernelError::Model(format!(
                                        "runtime approval is missing exact effect for job {job_id}"
                                    )));
                                }
                                let summary = exact_action_summary(&pending.effect_json);
                                let binding = ToolApprovalBinding {
                                    run_id: execution.manifest_id,
                                    call_id: call.id.clone(),
                                    tool_id: descriptor.id.clone(),
                                    job_id,
                                    node_id,
                                    node_index,
                                    effect_sha256: format!(
                                        "{:x}",
                                        Sha256::digest(pending.effect_json.as_bytes())
                                    ),
                                    summary: summary.clone(),
                                };
                                let mut finish_event = timing_event(
                                    TimingEventKind::ToolFinished,
                                    turn_started,
                                    Some(tool_duration_ms),
                                    Some(steps),
                                    Some(&call),
                                );
                                finish_event.status = Some("approval_required".into());
                                self.executions
                                    .record_timing_event(execution.manifest_id, &finish_event)?;
                                let mut lifecycle = tool_lifecycle_event(
                                    execution.manifest_id,
                                    &call,
                                    descriptor.id.clone(),
                                    ToolLifecyclePhase::ApprovalRequired,
                                    summary,
                                    Some(tool_duration_ms),
                                    None,
                                );
                                lifecycle.approval = Some(binding.clone());
                                self.executions.record_chat_approval_required(
                                    execution.manifest_id,
                                    &call,
                                    &lifecycle,
                                    &binding,
                                )?;
                                sink(StreamEvent::Tool(Box::new(lifecycle)));
                                sink(StreamEvent::Timing(finish_event));
                                self.sessions.save(
                                    self.session_id,
                                    &self.session_title,
                                    &pack_names(&self.packs),
                                    &self.messages,
                                )?;
                                return Err(error);
                            }
                            Err(error) if is_control_plane_tool_error(&error) => return Err(error),
                            Err(_) => (
                                descriptor.id.clone(),
                                ToolOutcome::failed(
                                    call.id.clone(),
                                    descriptor.id.clone(),
                                    format!("{} failed", descriptor.id.as_str()),
                                    ToolErrorDetail {
                                        code: "tool_execution_failed".into(),
                                        message: "tool execution failed".into(),
                                        retryable: false,
                                    },
                                    descriptor.replay,
                                ),
                            ),
                        }
                    };
                    let tool_duration_ms = elapsed_ms(tool_started);
                    if !suppressed {
                        timings.tool_ms = timings.tool_ms.saturating_add(tool_duration_ms);
                    }
                    if let Some(job_id) = outcome.data.get("job").and_then(Value::as_str) {
                        let job_uuid = Uuid::parse_str(job_id).map_err(|error| {
                            KernelError::Tool(format!(
                                "durable tool returned invalid job identity: {error}"
                            ))
                        })?;
                        let effect = self
                            .runtime
                            .latest_effect_outcome(optimus_runtime::job_id(job_uuid))?
                            .ok_or_else(|| {
                                KernelError::Tool(format!(
                                    "durable tool returned job {job_uuid} without terminal provenance"
                                ))
                            })?;
                        outcome.provenance = Some(DurableEffectProvenance {
                            job_id: effect.job_id.0,
                            node_id: effect.node_id,
                            effect_attempt_id: effect.attempt_id,
                            effect_sha256: effect.effect_hash,
                            receipt_sha256: effect.receipt_hash,
                        });
                    }
                    descriptor.validate_outcome(&outcome)?;
                    self.executions.record_tool_call(
                        execution.manifest_id,
                        &call,
                        &outcome,
                        tool_duration_ms,
                        suppressed,
                    )?;
                    let mut finish_event = timing_event(
                        TimingEventKind::ToolFinished,
                        turn_started,
                        Some(tool_duration_ms),
                        Some(steps),
                        Some(&call),
                    );
                    finish_event.suppressed = suppressed;
                    finish_event.status = Some(
                        if suppressed {
                            "suppressed"
                        } else if outcome.error.is_none() {
                            "succeeded"
                        } else {
                            "failed"
                        }
                        .into(),
                    );
                    self.executions
                        .record_timing_event(execution.manifest_id, &finish_event)?;
                    let result = serde_json::to_string(&outcome)?;
                    let effect_link = self.effect_link_for_tool_result(&call, &result)?;
                    if !suppressed {
                        invoked_tools.push(tool_id);
                    }
                    tool_trace.push(format!("{} -> {}", call.name, outcome.summary));
                    let phase = if suppressed {
                        ToolLifecyclePhase::Suppressed
                    } else {
                        match outcome.kind {
                            ToolOutcomeKind::Succeeded => ToolLifecyclePhase::Succeeded,
                            ToolOutcomeKind::Failed => ToolLifecyclePhase::Failed,
                            ToolOutcomeKind::Cancelled => ToolLifecyclePhase::Cancelled,
                            ToolOutcomeKind::Ambiguous => ToolLifecyclePhase::Ambiguous,
                        }
                    };
                    let lifecycle = tool_lifecycle_event(
                        execution.manifest_id,
                        &call,
                        descriptor.id.clone(),
                        phase,
                        outcome.summary.clone(),
                        Some(tool_duration_ms),
                        Some(outcome.clone()),
                    );
                    self.executions
                        .record_tool_lifecycle_event(execution.manifest_id, &lifecycle)?;
                    sink(StreamEvent::Tool(Box::new(lifecycle)));
                    sink(StreamEvent::Timing(finish_event));
                    self.messages.push(Message {
                        role: Role::Tool,
                        content: result,
                        tool_call_id: Some(call.id),
                        name: Some(call.name),
                    });
                    self.sessions.save_with_effect_links(
                        self.session_id,
                        &self.session_title,
                        &pack_names(&self.packs),
                        &self.messages,
                        effect_link.as_slice(),
                    )?;
                }
                continue;
            }

            let text = resp
                .text
                .filter(|t| !t.trim().is_empty())
                .ok_or_else(|| KernelError::Model("empty assistant response".into()))?;
            self.messages.push(Message {
                role: Role::Assistant,
                content: text.clone(),
                tool_call_id: None,
                name: None,
            });
            // Final compress before persist if tool-heavy turn blew past threshold.
            if compress::compress_messages(&mut self.messages, &self.config.compression) {
                compressed = true;
            }
            self.save_session()?;
            return Ok(TurnResult {
                assistant_text: text,
                steps,
                tool_trace,
                invoked_tools,
                trace_context: execution.trace_context,
                schema_tokens_final: self.packs.schema_tokens(),
                loaded_packs: self
                    .packs
                    .loaded_packs()
                    .into_iter()
                    .map(|p| p.as_str().to_string())
                    .collect(),
                compressed,
                timings: TurnTimings::default(),
            });
        }
    }
}
