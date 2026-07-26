//! Kernel tool dispatch: schema exposure, effect linkage, and the
//! model-requested tool call path.
//!
//! Split out of `lib.rs` under architectural law 21. This is a verbatim move —
//! the methods stay inherent to `Kernel` so no call site or visibility changed.
//! Private field access works because privacy in Rust extends to descendant
//! modules of the defining module.

use super::*;

impl Kernel {
    pub(crate) fn tool_schemas(&self) -> Vec<ToolSchema> {
        self.packs.loaded_tools().into_iter().cloned().collect()
    }

    pub(crate) fn effect_link_for_tool_result(
        &self,
        call: &ToolCall,
        result: &str,
    ) -> Result<Vec<SessionEffectLink>> {
        let Ok(value) = serde_json::from_str::<Value>(result) else {
            return Ok(Vec::new());
        };
        let Some(job_id) = value
            .pointer("/data/job")
            .or_else(|| value.get("job"))
            .and_then(Value::as_str)
        else {
            return Ok(Vec::new());
        };
        let job_uuid = Uuid::parse_str(job_id).map_err(|error| {
            KernelError::Tool(format!(
                "durable tool returned invalid job identity: {error}"
            ))
        })?;
        let outcome = self
            .runtime
            .latest_effect_outcome(optimus_runtime::job_id(job_uuid))?
            .ok_or_else(|| {
                KernelError::Tool(format!(
                    "durable tool returned job {job_uuid} without a terminal effect attempt"
                ))
            })?;
        Ok(vec![SessionEffectLink {
            tool_call_id: call.id.clone(),
            job_id: outcome.job_id.0,
            node_id: outcome.node_id,
            effect_attempt_id: outcome.attempt_id,
            effect_hash: outcome.effect_hash,
            outcome: outcome.status,
            receipt_hash: outcome.receipt_hash,
        }])
    }

    pub(crate) fn dispatch_tool(&mut self, call: &ToolCall) -> Result<(ToolId, String)> {
        let descriptor = self.packs.resolve_loaded_tool(&call.name)?;
        descriptor.validate_arguments(&call.arguments)?;
        let tool_id = descriptor.id.clone();
        let invocation = descriptor.invocation;
        let result = match invocation {
            ToolInvocation::ActivatePack => {
                let name = call
                    .arguments
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("activate_pack requires name".into()))?;
                self.packs.activate_str(name)?;
                // Update system prompt content for subsequent steps in-turn.
                if let Some(sys) = self.messages.first_mut() {
                    if sys.role == Role::System {
                        sys.content = system_prompt(&self.packs);
                    }
                }
                Ok(self.packs.activation_snapshot().to_string())
            }
            ToolInvocation::MemoryRecall => {
                let subject = call
                    .arguments
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let predicate = call
                    .arguments
                    .get("predicate")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let packet = self.memory.recall(
                    &self.config.memory_ctx,
                    RecallQuery {
                        purpose: RecallPurpose::Inform,
                        subject,
                        predicate,
                        as_of_valid: None,
                        as_of_tx: None,
                        limit: 5,
                    },
                )?;
                Ok(serde_json::to_string(&packet)?)
            }
            ToolInvocation::WebSearch => {
                let query = call
                    .arguments
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("web_search requires query".into()))?;
                let limit = call
                    .arguments
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(5) as usize;
                web_search_json(query, limit).map_err(|e| KernelError::Tool(e.to_string()))
            }
            ToolInvocation::SkillResolve => {
                let name = call
                    .arguments
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("skill_resolve requires name".into()))?;
                match self.skills.resolve(name)? {
                    Some(s) => Ok(json!({
                        "found": true,
                        "id": s.id,
                        "name": s.name,
                        "version": s.version,
                        "status": format!("{:?}", s.status),
                        "body": s.body,
                        "permissions": s.permissions,
                        "success_rate": s.success_rate,
                    })
                    .to_string()),
                    None => Ok(json!({ "found": false, "name": name }).to_string()),
                }
            }
            ToolInvocation::WriteFile => {
                let path = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("write_file requires path".into()))?;
                let contents = call
                    .arguments
                    .get("contents")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                {
                    let result = self.run_project_file_job(
                        format!("write:{path}"),
                        "write",
                        Effect::ProjectWriteFile {
                            workspace_sha256: self.runtime.workspace_sha256(),
                            relative_path: path.into(),
                            contents: contents.into(),
                        },
                    )?;
                    Ok(result)
                }
            }
            ToolInvocation::Mkdir => {
                let path = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("mkdir requires path".into()))?;
                {
                    let result = self.run_project_file_job(
                        format!("mkdir:{path}"),
                        "mkdir",
                        Effect::ProjectMkdir {
                            workspace_sha256: self.runtime.workspace_sha256(),
                            relative_path: path.into(),
                        },
                    )?;
                    Ok(result)
                }
            }
            ToolInvocation::DeletePath => {
                let path = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("delete_path requires path".into()))?;
                {
                    let result = self.run_project_file_job(
                        format!("delete:{path}"),
                        "delete",
                        Effect::ProjectDeletePath {
                            workspace_sha256: self.runtime.workspace_sha256(),
                            relative_path: path.into(),
                        },
                    )?;
                    Ok(result)
                }
            }
            ToolInvocation::RenamePath => {
                let from = call
                    .arguments
                    .get("from")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("rename_path requires from".into()))?;
                let to = call
                    .arguments
                    .get("to")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("rename_path requires to".into()))?;
                {
                    let result = self.run_project_file_job(
                        format!("rename:{from}->{to}"),
                        "rename",
                        Effect::ProjectRenamePath {
                            workspace_sha256: self.runtime.workspace_sha256(),
                            from_relative_path: from.into(),
                            to_relative_path: to.into(),
                        },
                    )?;
                    Ok(result)
                }
            }
            ToolInvocation::PatchFile => {
                let path = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("patch_file requires path".into()))?;
                let old_string = call
                    .arguments
                    .get("old_string")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("patch_file requires old_string".into()))?;
                let new_string = call
                    .arguments
                    .get("new_string")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("patch_file requires new_string".into()))?;
                {
                    let result = self.run_project_file_job(
                        format!("patch:{path}"),
                        "patch",
                        Effect::ProjectPatchFile {
                            workspace_sha256: self.runtime.workspace_sha256(),
                            relative_path: path.into(),
                            old_string: old_string.into(),
                            new_string: new_string.into(),
                        },
                    )?;
                    Ok(result)
                }
            }
            // Read-only helpers that may appear in core pack list
            ToolInvocation::ReadFile => {
                let path = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("read_file requires path".into()))?;
                let roots = FsRoots::new(self.project_roots.clone())
                    .map_err(|error| KernelError::Tool(format!("read {path}: {error}")))?;
                let body = roots
                    .read_text(path, 1024 * 1024, false)
                    .map_err(|error| KernelError::Tool(format!("read {path}: {error}")))?;
                Ok(json!({
                    "path": path,
                    "contents": body.content,
                    "truncated": body.truncated,
                })
                .to_string())
            }
            ToolInvocation::Terminal => {
                let program = call
                    .arguments
                    .get("program")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("terminal requires program".into()))?;
                let args: Vec<String> = call
                    .arguments
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let job = self.runtime.create_job(JobSpec {
                    label: format!("terminal:{program}"),
                    budget: Default::default(),
                    nodes: vec![NodeSpec {
                        label: "run".into(),
                        effect: Effect::ProjectRunCommand {
                            workspace_sha256: self.runtime.workspace_sha256(),
                            program: program.into(),
                            args,
                        },
                    }],
                })?;
                let status = self.runtime.run_all(job)?;
                if status == optimus_runtime::JobStatus::AwaitingApproval {
                    let node_index = self
                        .runtime
                        .list_pending_approvals()?
                        .into_iter()
                        .find(|pending| pending.job_id == job)
                        .and_then(|pending| pending.node_index)
                        .unwrap_or(0);
                    return Err(optimus_runtime::RuntimeError::NeedsApproval {
                        job_id: job,
                        node_index,
                    }
                    .into());
                }
                let capture = self.runtime.latest_command_capture(job)?;
                Ok(json!({
                    "ok": status == optimus_runtime::JobStatus::Succeeded,
                    "job": job.to_string(),
                    "status": format!("{status:?}"),
                    "stdout": capture.as_ref().map(|c| c.stdout.as_str()).unwrap_or(""),
                    "stderr": capture.as_ref().map(|c| c.stderr.as_str()).unwrap_or(""),
                    "exit_code": capture.as_ref().and_then(|c| c.exit_code),
                    "truncated_stdout": capture.as_ref().map(|c| c.truncated_stdout).unwrap_or(false),
                    "truncated_stderr": capture.as_ref().map(|c| c.truncated_stderr).unwrap_or(false),
                    "timed_out": capture.as_ref().map(|c| c.timed_out).unwrap_or(false),
                })
                .to_string())
            }
            ToolInvocation::BrowserNavigate
            | ToolInvocation::BrowserSnapshot
            | ToolInvocation::BrowserClick => {
                let mut browser =
                    best_effector(&self.workspace).map_err(|e| KernelError::Tool(e.to_string()))?;
                let result = match invocation {
                    ToolInvocation::BrowserNavigate => {
                        let url = call
                            .arguments
                            .get("url")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                KernelError::Tool("browser_navigate requires url".into())
                            })?;
                        let out = browser
                            .navigate(url)
                            .map_err(|e| KernelError::Tool(e.to_string()))?;
                        // ADR-0040: record agent domain only — never UserPreview session.
                        record_agent_browser_coord(self.home(), &out, url);
                        out
                    }
                    ToolInvocation::BrowserSnapshot => browser
                        .snapshot()
                        .map_err(|e| KernelError::Tool(e.to_string()))?,
                    ToolInvocation::BrowserClick => {
                        let idx = call
                            .arguments
                            .get("index")
                            .and_then(|v| v.as_u64())
                            .ok_or_else(|| {
                                KernelError::Tool("browser_click requires index".into())
                            })? as usize;
                        let out = browser
                            .click(idx)
                            .map_err(|e| KernelError::Tool(e.to_string()))?;
                        // Clicks navigate the agent effector — same domain bus as navigate.
                        record_agent_browser_coord(self.home(), &out, "");
                        out
                    }
                    _ => unreachable!("outer match restricts browser invocations"),
                };
                let _ = browser.close();
                Ok(result)
            }
            ToolInvocation::Unavailable => Err(KernelError::Tool(format!(
                "tool is unavailable: {}",
                call.name
            ))),
        }?;
        Ok((tool_id, result))
    }
}
