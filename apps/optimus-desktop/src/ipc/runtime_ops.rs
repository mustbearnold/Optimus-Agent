//! Approval, job, campaign, and bounded terminal IPC.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use optimus_graph::{Effect, JobSpec, NodeSpec};
use optimus_kernel::{ArtifactStore, BrowserEffector, ProjectAuthorityStore};
use optimus_runtime::{CampaignStepSpec, CampaignStore, StepKind};
use serde_json::json;

struct PreviewBrowserSession {
    home: PathBuf,
    effector: Box<dyn BrowserEffector>,
}

static PREVIEW_BROWSER: Mutex<Option<PreviewBrowserSession>> = Mutex::new(None);

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(
        method,
        "approvals_list"
            | "approvals_grant"
            | "jobs_list"
            | "campaign_list"
            | "campaign_create"
            | "campaign_run"
            | "campaign_status"
            | "term_run"
            | "browser_navigate"
            | "browser_click"
            | "browser_reload"
    )
}

pub(super) fn handle(
    home: &PathBuf,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "approvals_list" => {
            let rt = open_runtime(home)?;
            let rows: Vec<_> = rt
                .list_pending_approvals()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|p| {
                    json!({
                        "job_id": p.job_id.to_string(),
                        "job_label": p.job_label,
                        "job_status": format!("{:?}", p.job_status),
                        "node_label": p.node_label,
                        "node_index": p.node_index,
                        "has_grant": p.has_grant,
                        "effect_json": p.effect_json,
                    })
                })
                .collect();
            Ok(json!({ "pending": rows }))
        }
        "approvals_grant" => {
            let id = params
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "job_id required".to_string())?;
            let id = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
            let rt = open_runtime_for_job(home, optimus_runtime::job_id(id))?;
            let status = rt
                .grant_and_resume(optimus_runtime::job_id(id))
                .map_err(|e| e.to_string())?;
            let capture = rt
                .latest_command_capture(optimus_runtime::job_id(id))
                .map_err(|e| e.to_string())?;
            Ok(json!({
                "job_id": id.to_string(),
                "status": format!("{status:?}"),
                "stdout": capture.as_ref().map(|value| value.stdout.as_str()).unwrap_or(""),
                "stderr": capture.as_ref().map(|value| value.stderr.as_str()).unwrap_or(""),
                "exit_code": capture.as_ref().and_then(|value| value.exit_code),
                "timed_out": capture.as_ref().map(|value| value.timed_out).unwrap_or(false),
                "truncated_stdout": capture.as_ref().map(|value| value.truncated_stdout).unwrap_or(false),
                "truncated_stderr": capture.as_ref().map(|value| value.truncated_stderr).unwrap_or(false),
            }))
        }
        "jobs_list" => {
            let rt = open_runtime(home)?;
            let rows: Vec<_> = rt
                .list_jobs_summary()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|j| {
                    json!({
                        "job_id": j.job_id.to_string(),
                        "label": j.label,
                        "status": format!("{:?}", j.status),
                        "steps_executed": j.steps_executed,
                        "max_steps": j.max_steps,
                    })
                })
                .collect();
            Ok(json!({ "jobs": rows }))
        }
        "campaign_list" => {
            let store = CampaignStore::open(home).map_err(|e| e.to_string())?;
            let rows: Vec<_> = store
                .list()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|c| {
                    json!({
                        "id": c.id.to_string(),
                        "name": c.name,
                        "status": format!("{:?}", c.status),
                        "created_unix": c.created_unix,
                    })
                })
                .collect();
            Ok(json!({ "campaigns": rows }))
        }
        "campaign_create" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("campaign");
            let mut steps = Vec::new();
            if let Some(arr) = params.get("writes").and_then(|v| v.as_array()) {
                for w in arr {
                    let path = w.get("path").and_then(|v| v.as_str()).unwrap_or("out.txt");
                    let contents = w.get("contents").and_then(|v| v.as_str()).unwrap_or("");
                    steps.push(CampaignStepSpec {
                        label: path.to_string(),
                        kind: StepKind::WriteFile {
                            relative_path: path.into(),
                            contents: contents.into(),
                        },
                    });
                }
            }
            if steps.is_empty() {
                return Err("writes required".into());
            }
            let store = CampaignStore::open(home).map_err(|e| e.to_string())?;
            let view = store.create(name, steps).map_err(|e| e.to_string())?;
            Ok(json!({
                "id": view.campaign.id.to_string(),
                "steps": view.steps.len(),
                "status": format!("{:?}", view.campaign.status),
            }))
        }
        "campaign_run" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "id required".to_string())?;
            let id = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
            let store = CampaignStore::open(home).map_err(|e| e.to_string())?;
            let view = store.run(id).map_err(|e| e.to_string())?;
            Ok(json!({
                "id": view.campaign.id.to_string(),
                "status": format!("{:?}", view.campaign.status),
                "steps": view.steps.iter().map(|s| json!({
                    "idx": s.idx,
                    "label": s.label,
                    "status": format!("{:?}", s.status),
                    "detail": s.detail,
                })).collect::<Vec<_>>(),
            }))
        }
        "campaign_status" => {
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "id required".to_string())?;
            let id = uuid::Uuid::parse_str(id).map_err(|e| e.to_string())?;
            let store = CampaignStore::open(home).map_err(|e| e.to_string())?;
            let view = store
                .get(id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("campaign {id} not found"))?;
            Ok(json!({
                "id": view.campaign.id.to_string(),
                "name": view.campaign.name,
                "status": format!("{:?}", view.campaign.status),
                "steps": view.steps.iter().map(|s| json!({
                    "idx": s.idx,
                    "label": s.label,
                    "status": format!("{:?}", s.status),
                    "job_id": s.job_id.map(|j| j.to_string()),
                    "detail": s.detail,
                })).collect::<Vec<_>>(),
            }))
        }
        "term_run" => term_run(home, params),
        "browser_navigate" => browser_navigate(home, params),
        "browser_click" => browser_click(home, params),
        "browser_reload" => browser_reload(home, params),
        _ => Err(format!("unknown method: {method}")),
    }
}

/// Phase A terminal: one-shot host shell via Work Graph capture (not interactive PTY).
#[cfg(windows)]
fn term_effect(line: &str) -> Effect {
    Effect::RunCommand {
        program: "cmd".into(),
        args: vec!["/C".into(), line.into()],
    }
}

#[cfg(unix)]
fn term_effect(line: &str) -> Effect {
    Effect::RunCommand {
        program: "sh".into(),
        args: vec!["-c".into(), line.into()],
    }
}

fn term_line_denied(line: &str) -> Option<&'static str> {
    let l = line.to_ascii_lowercase();
    const BAD: &[&str] = &[
        "format ",
        "format\t",
        "del /s",
        "rd /s",
        "rmdir /s",
        "rm -rf",
        "rm -r ",
        "sudo ",
        "mkfs",
        "dd if=",
        "chmod -r",
        "chmod --recursive",
        "chown -r",
        "chown --recursive",
        "pkill ",
        "killall ",
        "systemctl poweroff",
        "systemctl reboot",
        " poweroff",
        " reboot",
        "shutdown",
        "reg delete",
        "powershell -enc",
        "msiexec",
        "start /b",
        "curl ",
        "wget ",
        "invoke-webrequest",
        "invoke-restmethod",
        "certutil",
        "bitsadmin",
        "net user",
        "net localgroup",
    ];
    for b in BAD {
        if l.contains(b) {
            return Some("command blocked by term allowlist policy");
        }
    }
    if line.len() > 2_000 {
        return Some("command too long");
    }
    None
}

fn term_run(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let line = params
        .get("line")
        .or_else(|| params.get("cmd"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "line required".to_string())?;
    if let Some(reason) = term_line_denied(line) {
        return Err(reason.into());
    }
    let rt = open_runtime(home)?;
    let job = rt
        .create_job(JobSpec {
            label: format!("term:{}", line.chars().take(48).collect::<String>()),
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: "term_run".into(),
                effect: term_effect(line),
            }],
        })
        .map_err(|e| e.to_string())?;
    let status = match rt.run_all(job) {
        Ok(s) => s,
        Err(optimus_runtime::RuntimeError::CommandFailed { capture, .. }) => {
            return Ok(json!({
                "job_id": job.0.to_string(),
                "status": "Failed",
                "stdout": capture.stdout,
                "stderr": capture.stderr,
                "exit_code": capture.exit_code,
                "timed_out": capture.timed_out,
                "truncated_stdout": capture.truncated_stdout,
                "truncated_stderr": capture.truncated_stderr,
                "line": line,
                "pty": false,
                "mode": "job-stream",
            }));
        }
        Err(e) => return Err(e.to_string()),
    };
    let cap = rt
        .latest_command_capture(job)
        .map_err(|e| e.to_string())?
        .unwrap_or(optimus_runtime::CommandCapture {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            truncated_stdout: false,
            truncated_stderr: false,
            timed_out: false,
        });
    Ok(json!({
        "job_id": job.0.to_string(),
        "status": format!("{status:?}"),
        "stdout": cap.stdout,
        "stderr": cap.stderr,
        "exit_code": cap.exit_code,
        "timed_out": cap.timed_out,
        "truncated_stdout": cap.truncated_stdout,
        "truncated_stderr": cap.truncated_stderr,
        "line": line,
        "pty": false,
        "mode": "job-stream",
    }))
}

pub(super) fn open_runtime(home: &std::path::Path) -> Result<optimus_runtime::Runtime, String> {
    let db = home.join("optimus.db");
    let ws = home.join("workspace");
    std::fs::create_dir_all(&ws).map_err(|e| e.to_string())?;
    optimus_runtime::Runtime::open(&db, &ws).map_err(|e| e.to_string())
}

fn open_runtime_for_job(
    home: &Path,
    job_id: optimus_graph::JobId,
) -> Result<optimus_runtime::Runtime, String> {
    let shared = open_runtime(home)?;
    let pending = shared
        .list_pending_approvals()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|approval| approval.job_id == job_id)
        .ok_or_else(|| format!("job {job_id} has no pending exact action"))?;
    let effect: Effect =
        serde_json::from_str(&pending.effect_json).map_err(|error| error.to_string())?;
    let expected = match effect {
        Effect::ProjectWriteFile {
            workspace_sha256, ..
        }
        | Effect::ProjectRunCommand {
            workspace_sha256, ..
        } => workspace_sha256,
        _ => return Ok(shared),
    };
    let store = ProjectAuthorityStore::open(home).map_err(|error| error.to_string())?;
    for scope in store.list_scopes().map_err(|error| error.to_string())? {
        let Ok(actual) = optimus_runtime::Runtime::canonical_workspace_sha256(&scope.primary_root)
        else {
            continue;
        };
        if actual == expected {
            return optimus_runtime::Runtime::open(&home.join("optimus.db"), &scope.primary_root)
                .map_err(|error| error.to_string());
        }
    }
    Err(format!(
        "project root for pending job {job_id} is no longer authorized"
    ))
}

/// IPC handler: navigate the browser to a URL and return page state.
fn browser_navigate(
    home: &PathBuf,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let url = params
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "browser_navigate requires url".to_string())?;
    with_preview_browser(home, |effector| {
        effector.navigate(url).map_err(|e| e.to_string())
    })
}

/// IPC handler: click an element by SOM index.
fn browser_click(home: &PathBuf, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let index = params
        .get("index")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "browser_click requires index".to_string())? as usize;
    with_preview_browser(home, |effector| {
        effector.click(index).map_err(|e| e.to_string())
    })
}

/// IPC handler: refresh the current page snapshot.
fn browser_reload(home: &PathBuf, _params: serde_json::Value) -> Result<serde_json::Value, String> {
    with_preview_browser(home, |effector| {
        effector.snapshot().map_err(|e| e.to_string())
    })
}

fn with_preview_browser<F>(home: &Path, op: F) -> Result<serde_json::Value, String>
where
    F: FnOnce(&mut dyn BrowserEffector) -> Result<String, String>,
{
    let workspace = home.join("workspace");
    std::fs::create_dir_all(&workspace).map_err(|e| e.to_string())?;

    let mut guard = PREVIEW_BROWSER
        .lock()
        .map_err(|_| "preview browser lock poisoned".to_string())?;

    let needs_new = match guard.as_ref() {
        Some(session) => session.home != home,
        None => true,
    };
    if needs_new {
        if let Some(mut old) = guard.take() {
            let _ = old.effector.close();
        }
        let effector = optimus_kernel::best_effector(&workspace).map_err(|e| e.to_string())?;
        *guard = Some(PreviewBrowserSession {
            home: home.to_path_buf(),
            effector,
        });
    }

    let session = guard
        .as_mut()
        .ok_or_else(|| "preview browser session missing".to_string())?;

    match op(session.effector.as_mut()) {
        Ok(result_json) => {
            let mut value: serde_json::Value = serde_json::from_str(&result_json)
                .unwrap_or(json!({ "ok": false, "error": result_json }));
            maybe_publish_browser_screenshot(home, &mut value);
            Ok(value)
        }
        Err(err) => {
            if let Some(mut old) = guard.take() {
                let _ = old.effector.close();
            }
            Err(err)
        }
    }
}

/// Best-effort: content-address browser screenshots into the artifact store.
fn maybe_publish_browser_screenshot(home: &Path, value: &mut serde_json::Value) {
    let Some(b64) = value.get("screenshot_b64").and_then(|v| v.as_str()) else {
        return;
    };
    if b64.is_empty() {
        return;
    }
    let title = value
        .get("page_title")
        .or_else(|| value.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("page");
    let url = value
        .get("final_url")
        .or_else(|| value.get("url"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let label = if url.is_empty() {
        format!("screenshot · {title}")
    } else {
        format!("screenshot · {title} · {url}")
    };
    let Ok(store) = ArtifactStore::open(home) else {
        return;
    };
    if let Ok(record) = store.put_base64(b64, "image/png", "browser.screenshot", &label, None) {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("artifact_sha256".into(), json!(record.sha256));
            obj.insert("artifact_size_bytes".into(), json!(record.size_bytes));
        }
    }
}

#[cfg(test)]
mod tests {
    use optimus_graph::{Effect, JobSpec, JobStatus, NodeSpec};
    use optimus_kernel::ProjectAuthorityStore;
    use tempfile::tempdir;

    use super::{handle, term_effect, term_line_denied};

    #[test]
    fn terminal_effect_uses_the_host_shell() {
        let Effect::RunCommand { program, args } = term_effect("printf optimus") else {
            panic!("terminal must remain a RunCommand effect");
        };

        #[cfg(windows)]
        assert_eq!((program.as_str(), args[0].as_str()), ("cmd", "/C"));
        #[cfg(unix)]
        assert_eq!((program.as_str(), args[0].as_str()), ("sh", "-c"));
        assert_eq!(args[1], "printf optimus");
    }

    #[test]
    fn terminal_policy_blocks_linux_elevation_and_disk_destruction() {
        for line in [
            "sudo apt purge optimus",
            "mkfs.ext4 /dev/sda",
            "dd if=/dev/zero of=/dev/sda",
            "chmod -R 000 /",
            "chown -R root:root /home",
        ] {
            assert!(term_line_denied(line).is_some(), "allowed: {line}");
        }
    }

    #[test]
    fn approval_grant_reopens_the_exact_authorized_project_root() {
        let home = tempdir().unwrap();
        let project = tempdir().unwrap();
        let authority = ProjectAuthorityStore::open(home.path()).unwrap();
        let selection = authority.stage_native_selection(project.path()).unwrap();
        authority
            .authorize_project(
                "project-a",
                std::slice::from_ref(&selection.path),
                Some(&selection.path),
                std::slice::from_ref(&selection.grant_token),
            )
            .unwrap();
        let runtime =
            optimus_runtime::Runtime::open(&home.path().join("optimus.db"), project.path())
                .unwrap();
        let job = runtime
            .create_job(JobSpec {
                label: "project write".into(),
                budget: Default::default(),
                nodes: vec![NodeSpec {
                    label: "write".into(),
                    effect: Effect::ProjectWriteFile {
                        workspace_sha256: runtime.workspace_sha256(),
                        relative_path: "approved.txt".into(),
                        contents: "exact".into(),
                    },
                }],
            })
            .unwrap();
        assert_eq!(runtime.run_all(job).unwrap(), JobStatus::AwaitingApproval);

        let response = handle(
            &home.path().to_path_buf(),
            "approvals_grant",
            serde_json::json!({"job_id": job.to_string()}),
        )
        .unwrap();

        assert_eq!(response["status"], "Succeeded");
        assert_eq!(
            std::fs::read_to_string(project.path().join("approved.txt")).unwrap(),
            "exact"
        );
        assert!(!home.path().join("workspace/approved.txt").exists());
    }
}
