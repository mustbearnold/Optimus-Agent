//! Kernel tool dispatch: schema exposure, effect linkage, and the
//! model-requested tool call path.
//!
//! Split out of `lib.rs` under architectural law 21. This is a verbatim move —
//! the methods stay inherent to `Kernel` so no call site or visibility changed.
//! Private field access works because privacy in Rust extends to descendant
//! modules of the defining module.

use super::*;
use crate::browser_coord::record_agent_browser_coord;

impl Kernel {
    /// Add the path a user can actually open to a successful workspace write.
    ///
    /// Runtime receipts stay portable and content-addressed, so they retain the
    /// relative path. The model-facing tool result may also name the resolved
    /// host path: without it, an agent asked "where did you save that?" has to
    /// probe `pwd` or system mount metadata after the write already succeeded.
    pub(crate) fn enrich_workspace_tool_data(
        &self,
        invocation: ToolInvocation,
        arguments: &Value,
        data: &mut Value,
    ) {
        if invocation != ToolInvocation::WriteFile {
            return;
        }
        let Some(relative_path) = arguments.get("path").and_then(Value::as_str) else {
            return;
        };
        let Some(object) = data.as_object_mut() else {
            return;
        };
        object.insert("relative_path".into(), json!(relative_path));
        object.insert(
            "absolute_path".into(),
            json!(self.workspace().join(relative_path).display().to_string()),
        );
    }

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
                        sys.content = crate::system_prompt::system_prompt(
                            &self.packs,
                            &self.skills.list(false).unwrap_or_default(),
                            self.runtime.command_fs_envelope(),
                        );
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
                let window = read_window(&call.arguments, &body.content);
                Ok(json!({
                    "path": path,
                    "contents": window.text,
                    "truncated": body.truncated,
                    "start_line": window.start_line,
                    "total_lines": window.total_lines,
                })
                .to_string())
            }
            ToolInvocation::SearchContent => {
                let pattern = call
                    .arguments
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("search_content requires pattern".into()))?;
                let roots = FsRoots::new(self.project_roots.clone())
                    .map_err(|error| KernelError::Tool(error.to_string()))?;
                let request = fs_search::SearchRequest {
                    pattern,
                    path: call.arguments.get("path").and_then(|v| v.as_str()),
                    glob: call.arguments.get("glob").and_then(|v| v.as_str()),
                    case_sensitive: call
                        .arguments
                        .get("case_sensitive")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    max_results: call
                        .arguments
                        .get("max_results")
                        .and_then(|v| v.as_u64())
                        .map(|value| value as usize),
                };
                let hits = fs_search::search_content(&roots, &request)
                    .map_err(|error| KernelError::Tool(format!("search: {error}")))?;
                Ok(json!({ "matches": hits, "count": hits.len() }).to_string())
            }
            ToolInvocation::FindFiles => {
                let glob = call
                    .arguments
                    .get("glob")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| KernelError::Tool("find_files requires glob".into()))?;
                let roots = FsRoots::new(self.project_roots.clone())
                    .map_err(|error| KernelError::Tool(error.to_string()))?;
                let found = fs_search::find_files(
                    &roots,
                    glob,
                    call.arguments.get("path").and_then(|v| v.as_str()),
                    call.arguments
                        .get("max_results")
                        .and_then(|v| v.as_u64())
                        .map(|value| value as usize),
                )
                .map_err(|error| KernelError::Tool(format!("find: {error}")))?;
                Ok(json!({ "paths": found, "count": found.len() }).to_string())
            }
            ToolInvocation::ListDir => {
                // Absent path means the workspace root, which is what "list the
                // directory" means before the agent knows any path at all.
                let path = call
                    .arguments
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                let roots = FsRoots::new(self.project_roots.clone())
                    .map_err(|error| KernelError::Tool(error.to_string()))?;
                let entries = roots
                    .list_dir(path)
                    .map_err(|error| KernelError::Tool(format!("list {path}: {error}")))?;
                Ok(json!({ "path": path, "entries": entries }).to_string())
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
                let args = strip_argv0(program, args);
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
                // One effector per session, not per call. The HTTP effector
                // survives per-call reopening through browser_state.json; a
                // CDP effector does not — a fresh headless Chrome per call
                // meant snapshot-after-navigate captured about:blank on every
                // host with Chrome installed, and paid the launch tax each
                // time. The session effector is dropped on error so a dead
                // Chrome is rebuilt, and closed in Drop with the kernel.
                let home = self.home().to_path_buf();
                if self.browser.is_none() {
                    self.browser = Some(
                        best_effector(&self.workspace)
                            .map_err(|e| KernelError::Tool(e.to_string()))?,
                    );
                }
                let browser = self.browser.as_mut().expect("effector ensured above");
                let result = match invocation {
                    ToolInvocation::BrowserNavigate => {
                        call.arguments
                            .get("url")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                KernelError::Tool("browser_navigate requires url".into())
                            })
                            .and_then(|url| {
                                let out = browser
                                    .navigate(url)
                                    .map_err(|e| KernelError::Tool(e.to_string()))?;
                                // ADR-0040: record agent domain only — never UserPreview session.
                                record_agent_browser_coord(&home, &out, url);
                                Ok(out)
                            })
                    }
                    ToolInvocation::BrowserSnapshot => browser
                        .snapshot()
                        .map_err(|e| KernelError::Tool(e.to_string())),
                    ToolInvocation::BrowserClick => {
                        call.arguments
                            .get("index")
                            .and_then(|v| v.as_u64())
                            .ok_or_else(|| KernelError::Tool("browser_click requires index".into()))
                            .and_then(|idx| {
                                let out = browser
                                    .click(idx as usize)
                                    .map_err(|e| KernelError::Tool(e.to_string()))?;
                                // Clicks navigate the agent effector — same domain bus as navigate.
                                record_agent_browser_coord(&home, &out, "");
                                Ok(out)
                            })
                    }
                    _ => unreachable!("outer match restricts browser invocations"),
                };
                // Bound before the canonical clamp: a CDP result carries its
                // screenshot inline, and pixels never fit the outcome budget.
                let result =
                    result.map(|out| crate::browser_budget::bound_browser_tool_json(&home, out));
                if result.is_err() {
                    if let Some(mut dead) = self.browser.take() {
                        let _ = dead.close();
                    }
                }
                result
            }
            ToolInvocation::VisionAnalyze => {
                // The transport carries no image blocks (Message.content is a
                // plain String), so the effector makes its own bounded HTTP
                // sub-call and returns the provider's text analysis. Image
                // paths ride the same FsRoots confinement as read_file.
                let roots = FsRoots::new(self.project_roots.clone())
                    .map_err(|error| KernelError::Tool(format!("vision_analyze: {error}")))?;
                crate::vision::vision_analyze_json(self.home(), &roots, &call.arguments)
                    .map_err(|error| KernelError::Tool(error.to_string()))
            }
            ToolInvocation::Unavailable => Err(KernelError::Tool(format!(
                "tool is unavailable: {}",
                call.name
            ))),
        }?;
        Ok((tool_id, result))
    }
}

impl Drop for Kernel {
    fn drop(&mut self) {
        // The session browser outlives individual calls by design; it must
        // not outlive the session. Close is best-effort — a Chrome that
        // already died has nothing left to leak.
        if let Some(mut effector) = self.browser.take() {
            let _ = effector.close();
        }
    }
}

/// The slice of a file a caller asked for, and where it sits in the whole.
struct ReadWindow {
    text: String,
    /// 1-based line the returned text begins at.
    start_line: u64,
    total_lines: u64,
}

/// Drop a leading argument that merely repeats the program name.
///
/// Two conventions collide here. `Command::new(program).args(args)` wants args
/// *without* argv[0]; POSIX `exec` and every `subprocess.run(["bash", "-lc",
/// …])` example a model has ever read put the program in argv[0]. Models emit
/// both, sometimes in consecutive calls of the same turn.
///
/// Passed through unchanged, the argv[0] form becomes `bash bash -lc "…"`,
/// which makes bash look for a *file* called `bash` and fail before the command
/// runs at all — a failure that reads as "the approved action failed" rather
/// than "the tool call was shaped wrong". Normalising here rather than at spawn
/// time means the approval card shows the command that will actually run.
///
/// Only an exact repeat is dropped: `bash bash` is normalised, `bash ./bash`
/// and `python python.py` are left alone.
fn strip_argv0(program: &str, args: Vec<String>) -> Vec<String> {
    match args.split_first() {
        Some((first, rest)) if first == program => rest.to_vec(),
        _ => args,
    }
}

/// Apply `offset`/`limit` to already-read text.
///
/// Reading a whole 4000-line file to quote one function spends the context
/// window on 3980 lines nobody asked about. The window is applied after the
/// sandboxed read rather than during it so that confinement, the secret filter
/// and the byte cap all stay in exactly one place.
///
/// `start_line` and `total_lines` ride along because a slice without its
/// position is unciteable: an agent given lines it cannot number cannot report
/// where anything is.
fn read_window(arguments: &Value, content: &str) -> ReadWindow {
    let number = |key: &str| arguments.get(key).and_then(Value::as_u64);
    let (offset, limit) = (number("offset"), number("limit"));
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len() as u64;
    if offset.is_none() && limit.is_none() {
        // Nothing asked for, nothing changed. `lines()` would swallow a
        // trailing newline, and a read tool that quietly reshapes a file is one
        // you cannot patch against.
        return ReadWindow {
            text: content.to_string(),
            start_line: 1,
            total_lines,
        };
    }
    // 1-based for the caller, because that is what an editor and a stack trace
    // both say; 0 is taken as the start rather than rejected.
    let start = offset.unwrap_or(1).saturating_sub(1).min(total_lines) as usize;
    let take = limit.map_or(usize::MAX, |limit| {
        usize::try_from(limit).unwrap_or(usize::MAX)
    });
    ReadWindow {
        text: lines
            .iter()
            .skip(start)
            .take(take)
            .copied()
            .collect::<Vec<_>>()
            .join("\n"),
        start_line: start as u64 + 1,
        total_lines,
    }
}

#[cfg(test)]
mod tests {
    use super::{read_window, strip_argv0};
    use serde_json::json;

    const FILE: &str = "one\ntwo\nthree\nfour\nfive\n";

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn a_repeated_program_name_is_not_passed_on_as_a_script_to_run() {
        // The exact shape that made every approved `bash -lc` fail: bash would
        // look for a file called `bash` instead of reading the -c string.
        assert_eq!(
            strip_argv0("bash", args(&["bash", "-lc", "echo hi"])),
            args(&["-lc", "echo hi"])
        );
    }

    #[test]
    fn args_that_already_omit_the_program_are_left_exactly_alone() {
        assert_eq!(
            strip_argv0("bash", args(&["-lc", "echo hi"])),
            args(&["-lc", "echo hi"])
        );
        assert_eq!(strip_argv0("ls", Vec::new()), Vec::<String>::new());
    }

    #[test]
    fn only_an_exact_repeat_is_dropped_not_a_lookalike_first_argument() {
        // `python python.py` and `bash ./bash` are real commands with a real
        // first argument. Guessing harder here would eat it.
        assert_eq!(
            strip_argv0("python", args(&["python.py"])),
            args(&["python.py"])
        );
        assert_eq!(strip_argv0("bash", args(&["./bash"])), args(&["./bash"]));
    }

    #[test]
    fn a_second_repeat_survives_because_only_argv0_is_the_convention() {
        // `bash bash bash` means run the script `bash` with the argument
        // `bash`. One layer of convention, not a de-duplication pass.
        assert_eq!(
            strip_argv0("bash", args(&["bash", "bash"])),
            args(&["bash"])
        );
    }

    #[test]
    fn no_window_returns_the_file_exactly_as_it_is_on_disk() {
        let whole = read_window(&json!({}), FILE);
        assert_eq!(whole.text, FILE, "the trailing newline must survive");
        assert_eq!(whole.start_line, 1);
        assert_eq!(whole.total_lines, 5);
    }

    #[test]
    fn an_offset_counts_from_one_the_way_a_stack_trace_does() {
        let window = read_window(&json!({"offset": 3, "limit": 2}), FILE);
        assert_eq!(window.text, "three\nfour");
        assert_eq!(window.start_line, 3, "off by one here misquotes every hit");
    }

    #[test]
    fn the_total_travels_with_the_slice_so_the_caller_knows_what_it_missed() {
        let window = read_window(&json!({"limit": 2}), FILE);
        assert_eq!(window.text, "one\ntwo");
        assert_eq!(window.total_lines, 5);
    }

    #[test]
    fn reading_past_the_end_is_empty_rather_than_a_panic() {
        // Models guess offsets. A guess past the end must answer "nothing
        // there", not take the process down.
        let window = read_window(&json!({"offset": 900, "limit": 10}), FILE);
        assert!(window.text.is_empty());
        assert_eq!(window.start_line, 6);
    }

    #[test]
    fn offset_zero_means_the_start_not_an_error() {
        let window = read_window(&json!({"offset": 0, "limit": 1}), FILE);
        assert_eq!(window.text, "one");
        assert_eq!(window.start_line, 1);
    }
}
