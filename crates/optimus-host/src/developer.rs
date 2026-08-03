//! Developer Full Access activation and the stable development supervisor.
//!
//! The host process owns this module, so rebuilding an Optimus child never
//! replaces the control channel that is currently serving the conversation.
//! State is durable and user-only; the child is always a separate process with
//! a separate home, port, token, and log.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use optimus_kernel::{atomic_write_user_only, ProductSettings};
use optimus_policy::{
    ActionTarget, DeveloperAccessGrant, DeveloperScope, DEVELOPER_ACCESS_CONFIRMATION_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

const SUPERVISOR_DIR: &str = "developer-supervisor";
const STATE_FILE: &str = "state.json";
const LOG_FILE: &str = "instance.log";
const ACTION_LOG_FILE: &str = "actions.log";
const INSTANCE_HOME: &str = "instance-home";
const CONFIRMATION: &str = "I understand Developer Full Access risks";
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SupervisorSpec {
    binary: String,
    workspace: String,
    child_home: String,
    port: u16,
    token: String,
    pid: u32,
    started_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SupervisorState {
    status: String,
    #[serde(default)]
    current: Option<SupervisorSpec>,
    #[serde(default)]
    previous: Option<SupervisorSpec>,
    #[serde(default)]
    last_error: Option<String>,
    #[serde(default)]
    emergency_stopped: bool,
}

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(
        method,
        "developer_access_get"
            | "developer_access_enable"
            | "developer_access_revoke"
            | "developer_supervisor_status"
            | "developer_supervisor_launch"
            | "developer_supervisor_stop"
            | "developer_supervisor_restart"
            | "developer_supervisor_rollback"
            | "developer_supervisor_log"
            | "developer_emergency_stop"
    )
}

pub(super) fn handle(
    home: &PathBuf,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let action_id = uuid::Uuid::new_v4();
    let started_unix_ms = now_unix_ms();
    let started = Instant::now();
    let result = match method {
        "developer_access_get" => access_get(home),
        "developer_access_enable" => access_enable(home, params),
        "developer_access_revoke" => access_revoke(home),
        "developer_supervisor_status" => supervisor_status(home),
        "developer_supervisor_launch" => supervisor_launch(home, params),
        "developer_supervisor_stop" => supervisor_stop(home, false),
        "developer_emergency_stop" => supervisor_stop(home, true),
        "developer_supervisor_restart" => supervisor_restart(home),
        "developer_supervisor_rollback" => supervisor_rollback(home),
        "developer_supervisor_log" => supervisor_log(home, params),
        _ => Err(format!("unknown method: {method}")),
    };
    let outcome = if result.is_ok() { "ok" } else { "error" };
    let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let _ = append_action_log(
        home,
        action_id,
        method,
        outcome,
        started_unix_ms,
        now_unix_ms(),
        duration_ms,
    );
    result
}

fn access_get(home: &PathBuf) -> Result<serde_json::Value, String> {
    let settings = ProductSettings::load(home).map_err(|error| error.to_string())?;
    Ok(json!({
        "developer_access": settings.developer_access.public_json(),
        "supervisor": reconciled_status(home)?,
        "confirmation": CONFIRMATION,
        "confirmation_version": DEVELOPER_ACCESS_CONFIRMATION_VERSION,
    }))
}

fn access_enable(home: &PathBuf, params: serde_json::Value) -> Result<serde_json::Value, String> {
    if params.get("confirmation").and_then(|value| value.as_str()) != Some(CONFIRMATION) {
        return Err("Developer Full Access requires the one-time confirmation".into());
    }
    let raw_grant = params
        .get("grant")
        .cloned()
        .unwrap_or_else(|| params.clone());
    let mut grant: DeveloperAccessGrant = serde_json::from_value(raw_grant)
        .map_err(|error| format!("invalid Developer Full Access grant: {error}"))?;
    grant.enabled = true;
    grant.confirmation_version = DEVELOPER_ACCESS_CONFIRMATION_VERSION;
    grant.issued_unix = now_unix();
    grant.scope = canonical_scope(grant.scope)?;
    grant.validate()?;

    let mut settings = ProductSettings::load(home).map_err(|error| error.to_string())?;
    settings.developer_access = grant;
    settings.save(home).map_err(|error| error.to_string())?;
    access_get(home)
}

fn access_revoke(home: &PathBuf) -> Result<serde_json::Value, String> {
    let mut settings = ProductSettings::load(home).map_err(|error| error.to_string())?;
    settings.developer_access = DeveloperAccessGrant::default();
    settings.save(home).map_err(|error| error.to_string())?;
    let stop_error = stop_internal(home, true).err();
    let mut result = access_get(home)?;
    if let Some(error) = stop_error {
        result["supervisor_error"] = json!(error);
    }
    Ok(result)
}

fn supervisor_status(home: &Path) -> Result<serde_json::Value, String> {
    reconciled_status(home)
}

fn supervisor_launch(
    home: &PathBuf,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let settings = ProductSettings::load(home).map_err(|error| error.to_string())?;
    let grant = &settings.developer_access;
    if !grant.enabled {
        return Err("Developer Full Access is not enabled".into());
    }
    grant.validate()?;
    if !grant.capabilities.process_management || !grant.capabilities.terminal_execution {
        return Err(
            "Developer Full Access needs terminal execution and process management enabled".into(),
        );
    }

    let binary = required_absolute_path(&params, "binary", true)?;
    let workspace = required_absolute_path(&params, "workspace", false)?;
    let binary =
        fs::canonicalize(binary).map_err(|error| format!("binary is unavailable: {error}"))?;
    let workspace = fs::canonicalize(workspace)
        .map_err(|error| format!("workspace is unavailable: {error}"))?;
    if !binary.is_file() {
        return Err("development binary must be a regular file".into());
    }
    if !workspace.is_dir() {
        return Err("development workspace must be a directory".into());
    }
    assert_in_scope(grant, &binary, "development binary")?;
    assert_in_scope(grant, &workspace, "development workspace")?;

    let port = params
        .get("port")
        .and_then(|value| value.as_u64())
        .unwrap_or(17_866);
    let port = u16::try_from(port).map_err(|_| "port must be between 1024 and 65535")?;
    if !(1024..=65535).contains(&port) {
        return Err("port must be between 1024 and 65535".into());
    }

    let next = SupervisorSpec {
        binary: binary.display().to_string(),
        workspace: workspace.display().to_string(),
        child_home: supervisor_dir(home)
            .join(INSTANCE_HOME)
            .display()
            .to_string(),
        port,
        token: format!(
            "optimus-dev-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ),
        pid: 0,
        started_unix: now_unix(),
    };
    let old = load_state(home)?.current;
    if old.is_some() {
        stop_internal(home, false)?;
    }
    let previous = old;
    match launch_spec(home, &next) {
        Ok(launched) => {
            let mut state = load_state(home)?;
            state.status = "healthy".into();
            state.current = Some(launched);
            state.previous = previous;
            state.last_error = None;
            state.emergency_stopped = false;
            save_state(home, &state)?;
            reconciled_status(home)
        }
        Err(error) => {
            let _ = stop_pid(
                load_state(home)?
                    .current
                    .as_ref()
                    .and_then(|spec| (spec.pid != 0).then_some(spec.pid)),
            );
            let mut state = load_state(home)?;
            state.current = None;
            state.status = "failed".into();
            state.last_error = Some(error.clone());
            save_state(home, &state)?;
            if let Some(previous) = previous {
                if let Ok(launched) = launch_spec(home, &previous) {
                    let mut recovered = load_state(home)?;
                    recovered.status = "rolled_back".into();
                    recovered.current = Some(launched);
                    recovered.last_error = Some(error.clone());
                    save_state(home, &recovered)?;
                    return Err(format!(
                        "development instance failed health check; previous instance restored: {error}"
                    ));
                }
            }
            Err(error)
        }
    }
}

fn supervisor_stop(home: &Path, emergency: bool) -> Result<serde_json::Value, String> {
    stop_internal(home, emergency)?;
    reconciled_status(home)
}

fn supervisor_restart(home: &Path) -> Result<serde_json::Value, String> {
    let state = load_state(home)?;
    let current = state
        .current
        .ok_or_else(|| "no development instance is running".to_string())?;
    stop_internal(home, false)?;
    let launched = launch_spec(home, &current)?;
    let mut state = load_state(home)?;
    state.status = "healthy".into();
    state.current = Some(launched);
    state.last_error = None;
    save_state(home, &state)?;
    reconciled_status(home)
}

fn supervisor_rollback(home: &Path) -> Result<serde_json::Value, String> {
    let state = load_state(home)?;
    let previous = state
        .previous
        .ok_or_else(|| "no previous healthy development instance is available".to_string())?;
    let old_current = state.current;
    stop_internal(home, false)?;
    let launched = launch_spec(home, &previous)?;
    let mut state = load_state(home)?;
    state.status = "rolled_back".into();
    state.previous = old_current;
    state.current = Some(launched);
    state.last_error = None;
    save_state(home, &state)?;
    reconciled_status(home)
}

fn supervisor_log(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let requested = params
        .get("lines")
        .and_then(|value| value.as_u64())
        .unwrap_or(80)
        .clamp(1, 400) as usize;
    let path = supervisor_dir(home).join(LOG_FILE);
    let (lines, line_count) = read_log_tail(&path, requested)?;
    let action_path = supervisor_dir(home).join(ACTION_LOG_FILE);
    let (actions, action_count) = read_log_tail(&action_path, requested)?;
    Ok(json!({
        "path": path.display().to_string(),
        "lines": lines,
        "line_count": line_count,
        "action_path": action_path.display().to_string(),
        "actions": actions,
        "action_line_count": action_count,
    }))
}

fn read_log_tail(path: &Path, requested: usize) -> Result<(String, usize), String> {
    let body = if path.is_file() {
        fs::read_to_string(path).map_err(|error| error.to_string())?
    } else {
        String::new()
    };
    let lines: Vec<&str> = body.lines().collect();
    let start = lines.len().saturating_sub(requested);
    Ok((lines[start..].join("\n"), lines.len().saturating_sub(start)))
}

fn append_action_log(
    home: &Path,
    action_id: uuid::Uuid,
    action: &str,
    outcome: &str,
    started_unix_ms: u64,
    finished_unix_ms: u64,
    duration_ms: u64,
) -> Result<(), String> {
    let dir = supervisor_dir(home);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(ACTION_LOG_FILE);
    rotate_log_if_needed(&path)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    let record = json!({
        "action_id": action_id,
        "action": action,
        "outcome": outcome,
        "at": finished_unix_ms / 1_000,
        "started_unix_ms": started_unix_ms,
        "finished_unix_ms": finished_unix_ms,
        "duration_ms": duration_ms,
    });
    writeln!(file, "{record}").map_err(|error| error.to_string())
}

fn launch_spec(home: &Path, spec: &SupervisorSpec) -> Result<SupervisorSpec, String> {
    let dir = supervisor_dir(home);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    prepare_instance_home(spec)?;
    let log_path = dir.join(LOG_FILE);
    rotate_log_if_needed(&log_path)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| error.to_string())?;
    let stderr = log.try_clone().map_err(|error| error.to_string())?;
    let mut command = Command::new(&spec.binary);
    command
        .args([
            "--host-only",
            "--host-port",
            &spec.port.to_string(),
            "--home",
            &spec.child_home,
        ])
        .env("OPTIMUS_HTTP_TOKEN", &spec.token)
        .env("OPTIMUS_SUPPRESS_TOKEN_LOG", "1")
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr));
    let child = command
        .spawn()
        .map_err(|error| format!("could not launch development instance: {error}"))?;
    let actual = SupervisorSpec {
        pid: child.id(),
        started_unix: now_unix(),
        ..spec.clone()
    };
    let mut state = load_state(home)?;
    state.current = Some(actual.clone());
    state.status = "starting".into();
    save_state(home, &state)?;
    match wait_for_health(actual.port, &actual.token, actual.pid) {
        Ok(()) => Ok(actual),
        Err(error) => {
            let stop_error = stop_pid(Some(actual.pid)).err();
            let mut state = load_state(home)?;
            state.current = None;
            state.status = "failed".into();
            state.last_error = Some(match stop_error {
                Some(stop_error) => format!("{error}; cleanup failed: {stop_error}"),
                None => error.clone(),
            });
            save_state(home, &state)?;
            Err(error)
        }
    }
}

fn prepare_instance_home(spec: &SupervisorSpec) -> Result<(), String> {
    let home = Path::new(&spec.child_home);
    fs::create_dir_all(home).map_err(|error| error.to_string())?;
    let workspace_link = home.join("workspace");
    if workspace_link.exists() {
        let actual = fs::canonicalize(&workspace_link).map_err(|error| error.to_string())?;
        if actual != Path::new(&spec.workspace) {
            return Err("development instance home is bound to a different workspace".into());
        }
        return Ok(());
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&spec.workspace, &workspace_link)
        .map_err(|error| format!("could not bind development workspace: {error}"))?;
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&spec.workspace, &workspace_link)
        .map_err(|error| format!("could not bind development workspace: {error}"))?;
    Ok(())
}

fn wait_for_health(port: u16, token: &str, pid: u32) -> Result<(), String> {
    for _ in 0..50 {
        if !pid_alive(pid) {
            return Err("development instance exited before health check".into());
        }
        if health_ok(port, token) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("development instance did not pass the health check within 5 seconds".into())
}

fn health_ok(port: u16, token: &str) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(150)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
    let request = format!(
        "GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut response = String::new();
    stream.read_to_string(&mut response).is_ok() && response.starts_with("HTTP/1.1 200")
}

fn stop_internal(home: &Path, emergency: bool) -> Result<(), String> {
    let mut state = load_state(home)?;
    if let Some(spec) = state.current.as_ref() {
        stop_pid((spec.pid != 0).then_some(spec.pid))?;
    }
    state.current = None;
    state.status = if emergency {
        "emergency_stopped"
    } else {
        "stopped"
    }
    .into();
    state.emergency_stopped = emergency;
    save_state(home, &state)
}

fn reconciled_status(home: &Path) -> Result<serde_json::Value, String> {
    let mut state = load_state(home)?;
    let (healthy, pid) = if let Some(spec) = state.current.as_ref() {
        let pid = (spec.pid != 0).then_some(spec.pid);
        let alive = pid.is_some_and(pid_alive);
        (alive && health_ok(spec.port, &spec.token), pid)
    } else {
        (false, None)
    };
    if state.current.is_some() && !healthy {
        state.status = if pid.is_some() {
            "unhealthy"
        } else {
            "stopped"
        }
        .into();
    } else if healthy {
        state.status = "healthy".into();
    }
    save_state(home, &state)?;
    let current = state.current.as_ref();
    Ok(json!({
        "status": state.status,
        "healthy": healthy,
        "pid": pid,
        "port": current.map(|spec| spec.port),
        "binary": current.map(|spec| spec.binary.clone()),
        "workspace": current.map(|spec| spec.workspace.clone()),
        "child_home": current.map(|spec| spec.child_home.clone()),
        "log_path": supervisor_dir(home).join(LOG_FILE).display().to_string(),
        "started_unix": current.map(|spec| spec.started_unix),
        "last_error": state.last_error,
        "emergency_stopped": state.emergency_stopped,
        "previous_available": state.previous.is_some(),
    }))
}

fn assert_in_scope(grant: &DeveloperAccessGrant, path: &Path, label: &str) -> Result<(), String> {
    let target = ActionTarget {
        summary: label.into(),
        project_root_hash: None,
        relative_path: None,
        absolute_path: Some(path.display().to_string()),
        owned_localhost: None,
    };
    if grant.scope.allows_target(&target) {
        Ok(())
    } else {
        Err(format!(
            "{label} is outside the active Developer Full Access scope"
        ))
    }
}

fn canonical_scope(scope: DeveloperScope) -> Result<DeveloperScope, String> {
    match scope {
        DeveloperScope::SelectedRepository { root, root_hash } => {
            Ok(DeveloperScope::SelectedRepository {
                root: canonical_root(&root)?,
                root_hash,
            })
        }
        DeveloperScope::SelectedDirectories { roots } => Ok(DeveloperScope::SelectedDirectories {
            roots: roots
                .iter()
                .map(|root| canonical_root(root))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        DeveloperScope::EntireLocalMachine => Ok(DeveloperScope::EntireLocalMachine),
    }
}

fn canonical_root(raw: &str) -> Result<String, String> {
    fs::canonicalize(raw)
        .map(|path| path.display().to_string())
        .map_err(|error| format!("scope root {raw:?} is unavailable: {error}"))
}

fn required_absolute_path(
    params: &serde_json::Value,
    field: &str,
    file: bool,
) -> Result<PathBuf, String> {
    let raw = params
        .get(field)
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("{field} required"))?;
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(format!("{field} must be an absolute path"));
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(format!("{field} must not contain parent traversal"));
    }
    if file && !path.is_file() {
        return Err(format!("{field} must point to an existing file"));
    }
    Ok(path)
}

fn supervisor_dir(home: &Path) -> PathBuf {
    home.join(SUPERVISOR_DIR)
}

fn state_path(home: &Path) -> PathBuf {
    supervisor_dir(home).join(STATE_FILE)
}

fn load_state(home: &Path) -> Result<SupervisorState, String> {
    let path = state_path(home);
    if !path.is_file() {
        return Ok(SupervisorState {
            status: "idle".into(),
            ..Default::default()
        });
    }
    let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&raw).map_err(|error| format!("supervisor state is invalid: {error}"))
}

fn save_state(home: &Path, state: &SupervisorState) -> Result<(), String> {
    let dir = supervisor_dir(home);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let body = serde_json::to_vec_pretty(state).map_err(|error| error.to_string())?;
    atomic_write_user_only(&state_path(home), &body).map_err(|error| error.to_string())
}

fn rotate_log_if_needed(path: &Path) -> Result<(), String> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(());
    };
    if metadata.len() <= MAX_LOG_BYTES {
        return Ok(());
    }
    fs::rename(path, path.with_extension("log.1")).map_err(|error| error.to_string())
}

fn stop_pid(pid: Option<u32>) -> Result<(), String> {
    let Some(pid) = pid else {
        return Ok(());
    };
    #[cfg(unix)]
    {
        let pid = pid as libc::pid_t;
        unsafe {
            if libc::kill(pid, libc::SIGTERM) != 0 && *libc::__errno_location() != libc::ESRCH {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        for _ in 0..20 {
            if reap_child(pid)? {
                return Ok(());
            }
            if !pid_alive(pid as u32) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        unsafe {
            if libc::kill(pid, libc::SIGKILL) != 0 && *libc::__errno_location() != libc::ESRCH {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        for _ in 0..20 {
            if reap_child(pid)? || !pid_alive(pid as u32) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err("development instance did not exit after SIGKILL".into())
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Err("process stop is not implemented on this platform".into())
    }
}

#[cfg(unix)]
fn reap_child(pid: libc::pid_t) -> Result<bool, String> {
    let mut status = 0;
    let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    if result == pid {
        return Ok(true);
    }
    if result == 0 {
        return Ok(false);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ECHILD) {
        return Ok(false);
    }
    Err(format!("could not inspect development instance: {error}"))
}

fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        if reap_child(pid as libc::pid_t).unwrap_or(false) {
            return false;
        }
        unsafe {
            libc::kill(pid as libc::pid_t, 0) == 0 || *libc::__errno_location() == libc::EPERM
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn developer_methods_are_registered_as_one_surface() {
        assert!(owns("developer_access_enable"));
        assert!(owns("developer_emergency_stop"));
        assert!(!owns("developer_full_access"));
    }

    #[test]
    fn activation_requires_confirmation_and_canonicalizes_scope() {
        let home = tempdir().unwrap();
        let root = tempdir().unwrap();
        let error = access_enable(
            &home.path().to_path_buf(),
            json!({
                "confirmation": "no",
                "grant": {
                    "scope": { "kind": "selected_repository", "root": root.path() },
                    "capabilities": DeveloperAccessGrant::default().capabilities
                }
            }),
        )
        .unwrap_err();
        assert!(error.contains("one-time confirmation"));

        let result = access_enable(
            &home.path().to_path_buf(),
            json!({
                "confirmation": CONFIRMATION,
                "grant": {
                    "scope": { "kind": "selected_repository", "root": root.path() },
                    "capabilities": DeveloperAccessGrant::default().capabilities
                }
            }),
        )
        .unwrap();
        assert_eq!(result["developer_access"]["enabled"], true);
        assert_eq!(
            result["developer_access"]["scope"]["root"],
            root.path().canonicalize().unwrap().display().to_string()
        );
    }

    #[test]
    fn revoked_access_stops_the_supervisor_state() {
        let home = tempdir().unwrap();
        let result = access_revoke(&home.path().to_path_buf()).unwrap();
        assert_eq!(result["developer_access"]["enabled"], false);
        assert_eq!(result["supervisor"]["status"], "emergency_stopped");
    }

    #[test]
    fn handle_records_every_developer_action_with_monotonic_duration() {
        let home = tempdir().unwrap();
        let _ = handle(
            &home.path().to_path_buf(),
            "developer_access_get",
            json!({}),
        );
        let lines =
            fs::read_to_string(home.path().join(SUPERVISOR_DIR).join(ACTION_LOG_FILE)).unwrap();
        let record: serde_json::Value =
            serde_json::from_str(lines.lines().next().unwrap()).unwrap();
        assert!(record["action_id"].as_str().is_some());
        assert_eq!(record["action"], "developer_access_get");
        assert!(record["started_unix_ms"].as_u64().unwrap() > 0);
        assert!(
            record["finished_unix_ms"].as_u64().unwrap()
                >= record["started_unix_ms"].as_u64().unwrap()
        );
        assert!(record["duration_ms"].as_u64().is_some());
    }

    #[cfg(unix)]
    #[test]
    fn failed_launch_cleans_up_child_and_state() {
        let home = tempdir().unwrap();
        let workspace = tempdir().unwrap();
        let spec = SupervisorSpec {
            binary: "/bin/false".into(),
            workspace: workspace.path().display().to_string(),
            child_home: home.path().join(INSTANCE_HOME).display().to_string(),
            port: 18_867,
            token: "failed-launch-test".into(),
            pid: 0,
            started_unix: now_unix(),
        };

        let error = launch_spec(home.path(), &spec).unwrap_err();
        assert!(error.contains("exited before health check"));
        let state = load_state(home.path()).unwrap();
        assert!(state.current.is_none());
        assert_eq!(state.status, "failed");
    }
}
