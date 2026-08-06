use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use optimus_host::DEFAULT_HOST_PORT;
use optimus_kernel::CancellationToken;
use serde_json::{json, Value};
use tauri::ipc::Channel;
use tauri::{Manager, State, WebviewWindow};

mod serve_lifecycle;
mod stage_relay;

use serve_lifecycle::ServeLifecycle;

#[derive(Clone)]
struct AppState {
    home: PathBuf,
    cancellations: Arc<Mutex<HashMap<u64, CancellationToken>>>,
    ready_file: Option<PathBuf>,
    initial_session_id: Option<String>,
    /// The serve lifecycle: attach-or-spawn-or-diagnose (spec-015 A4/R8).
    serve: ServeLifecycle,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct LaunchOptions {
    home: Option<PathBuf>,
    ready_file: Option<PathBuf>,
    initial_session_id: Option<String>,
}

fn launch_options() -> LaunchOptions {
    parse_launch_options(std::env::args().skip(1))
}

fn parse_launch_options<I>(args: I) -> LaunchOptions
where
    I: IntoIterator<Item = String>,
{
    let mut options = LaunchOptions::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--home" => {
                options.home = args.next().map(PathBuf::from);
            }
            "--supervised-ready" => {
                options.ready_file = args.next().map(PathBuf::from);
            }
            "--session" => {
                options.initial_session_id = args.next();
            }
            _ => {}
        }
    }
    options
}

fn write_ready_file(path: &PathBuf) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let body = serde_json::json!({
        "pid": std::process::id(),
        "ready": true,
    });
    std::fs::write(path, body.to_string()).map_err(|error| error.to_string())
}

#[tauri::command]
async fn host_invoke(
    method: String,
    params: Value,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    if method == "startup_context" {
        return Ok(json!({
            "session_id": state.initial_session_id,
            "handoff": state.initial_session_id.is_some(),
        }));
    }
    let home = state.home.clone();
    tauri::async_runtime::spawn_blocking(move || optimus_host::handle_ipc(&home, &method, params))
        .await
        .map_err(|error| format!("IPC worker failed: {error}"))?
}

#[tauri::command]
fn chat_start(
    stream_id: u64,
    request: Value,
    events: Channel<Value>,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let cancellation = CancellationToken::new();
    state
        .cancellations
        .lock()
        .map_err(|_| "chat cancellation registry poisoned".to_string())?
        .insert(stream_id, cancellation.clone());

    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut on_event = |event| {
            let payload = optimus_host::stream_event_to_json(&event);
            optimus_host::stream_delivery_control(events.send(payload).is_ok())
        };
        let outcome = optimus_host::chat_turn_cancellable(
            &app.home,
            request,
            Some(&mut on_event),
            &cancellation,
        );
        let terminal = match outcome {
            Ok(result) => json!({ "type": "done", "result": result }),
            Err(error) if cancellation.is_cancelled() => {
                json!({ "type": "cancelled", "error": error })
            }
            Err(error) => json!({ "type": "error", "error": error }),
        };
        let _ = events.send(terminal);
        if let Ok(mut active) = app.cancellations.lock() {
            active.remove(&stream_id);
        }
    });
    Ok(json!({ "streamId": stream_id }))
}

/// Resolve a parked chat approval as a streaming turn (ADR-0046).
///
/// Settling an approval resumes the paused turn, so the decision is not a
/// request/response: the continuation's events must reach the surface as they
/// arrive, and the user must be able to cancel a long continuation. This
/// mirrors [`chat_start`] — same registry, same terminal-event contract — but
/// drives `chat_approval_resolve_cancellable`, which settles the exact bound
/// effect first and then streams the resumed turn. Without this path the
/// workbench's "Approving…" button stayed stuck for the whole continuation:
/// `host_invoke` ran the resolve as a blocking call with no events and no
/// cancellation handle.
#[tauri::command]
fn chat_approval_resolve_start(
    stream_id: u64,
    params: Value,
    events: Channel<Value>,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let cancellation = CancellationToken::new();
    state
        .cancellations
        .lock()
        .map_err(|_| "chat cancellation registry poisoned".to_string())?
        .insert(stream_id, cancellation.clone());

    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut on_event = |event| {
            let payload = optimus_host::stream_event_to_json(&event);
            optimus_host::stream_delivery_control(events.send(payload).is_ok())
        };
        let outcome = optimus_host::chat_approval_resolve_cancellable(
            &app.home,
            params,
            Some(&mut on_event),
            &cancellation,
        );
        // A cancelled resolve is a cancelled resolve even when the settlement
        // itself succeeded: the user stopped the continuation, and the UI must
        // not read the terminal event as a completed turn.
        let terminal = if cancellation.is_cancelled() {
            let error = outcome
                .as_ref()
                .err()
                .cloned()
                .unwrap_or_else(|| "approval continuation cancelled".into());
            json!({ "type": "cancelled", "error": error })
        } else {
            match outcome {
                Ok(result) => json!({ "type": "done", "result": result }),
                Err(error) => json!({ "type": "error", "error": error }),
            }
        };
        let _ = events.send(terminal);
        if let Ok(mut active) = app.cancellations.lock() {
            active.remove(&stream_id);
        }
    });
    Ok(json!({ "streamId": stream_id }))
}

#[tauri::command]
fn chat_cancel(stream_id: u64, state: State<'_, AppState>) -> Result<Value, String> {
    let active = state
        .cancellations
        .lock()
        .map_err(|_| "chat cancellation registry poisoned".to_string())?;
    let requested = active.get(&stream_id).is_some_and(|token| {
        token.cancel();
        true
    });
    Ok(json!({ "requested": requested }))
}

#[tauri::command]
fn window_action(action: String, window: WebviewWindow) -> Result<Value, String> {
    match action.as_str() {
        "minimize" => window.minimize().map_err(|error| error.to_string())?,
        "maximize" => {
            if window.is_maximized().map_err(|error| error.to_string())? {
                window.unmaximize().map_err(|error| error.to_string())?;
            } else {
                window.maximize().map_err(|error| error.to_string())?;
            }
        }
        "close" => window.close().map_err(|error| error.to_string())?,
        _ => return Err(format!("unsupported window action: {action}")),
    }
    Ok(json!({ "ok": true }))
}

#[tauri::command]
fn pick_folder(state: State<'_, AppState>) -> Result<Value, String> {
    // The shell stages the native selection over its own shell-kind
    // wire connection when serve is up (spec-015 A4); the in-process
    // path remains the pre-wire fallback.
    let picked = rfd::FileDialog::new()
        .set_title("Authorize project folder")
        .pick_folder();
    let Some(path) = picked else {
        return Ok(json!({ "ok": false, "cancelled": true }));
    };
    match state.serve.stage_native(&path.to_string_lossy()) {
        Ok(selection) => {
            // Same envelope as the dialog path (the renderer's
            // pickFolder contract): the selection carries the grant
            // token the authorize call consumes.
            let mut envelope = selection;
            if let Some(object) = envelope.as_object_mut() {
                object.insert("ok".into(), json!(true));
                object.insert("cancelled".into(), json!(false));
            }
            Ok(envelope)
        }
        Err(_) => {
            // Pre-wire fallback: no serve to relay with — stage
            // in-process (the shell still shares the home).
            optimus_host::pick_folder_dialog(&state.home)
        }
    }
}

/// The broker answer (spec-015 A3): the healthy v2/ws record — port +
/// dial ticket — or null. The renderer connects to `optimus serve` over
/// WS with it; null is a CONFIRMED absence (the terminal affordance).
#[tauri::command]
fn broker_ticket(state: State<'_, AppState>) -> Option<Value> {
    state
        .serve
        .broker_ticket()
        .map(|(port, ticket)| json!({ "port": port, "ticket": ticket }))
}

fn main() {
    if std::env::args().any(|arg| arg == "--version" || arg == "-V") {
        println!("optimus-agent {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let options = launch_options();
    #[cfg(target_os = "linux")]
    configure_linux_webview();
    let home = optimus_host::resolve_home(options.home.as_deref().and_then(|path| path.to_str()));
    if let Err(error) = std::fs::create_dir_all(&home) {
        eprintln!(
            "[optimus-tauri] could not create home {}: {error}",
            home.display()
        );
    }
    // The broker lifecycle runs before the window: the renderer's
    // bootstrap awaits the broker ticket (spec-015 A3), so the record must
    // exist (or the diagnosis be terminal) by the time the webview loads.
    let serve = ServeLifecycle::start(home.clone(), DEFAULT_HOST_PORT);
    // SIGTERM/SIGINT quit termination (R8): the tauri event loop does not
    // fire RunEvent::Exit for signals.
    serve_lifecycle::install_termination_handler(serve.clone());
    let state = AppState {
        home,
        cancellations: Arc::new(Mutex::new(HashMap::new())),
        ready_file: options.ready_file,
        initial_session_id: options.initial_session_id,
        serve,
    };
    tauri::Builder::default()
        .manage(state)
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                window.set_background_color(Some(tauri::window::Color(10, 10, 12, 255)))?;
            }
            if let Some(path) = app.state::<AppState>().ready_file.as_ref() {
                if let Err(error) = write_ready_file(path) {
                    eprintln!("[optimus-tauri] supervised readiness failed: {error}");
                }
            }
            eprintln!(
                "[optimus-tauri] ready ui=react mode={} debug={}",
                if tauri::is_dev() {
                    "dev-server"
                } else {
                    "embedded"
                },
                cfg!(debug_assertions)
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            host_invoke,
            chat_start,
            chat_approval_resolve_start,
            chat_cancel,
            window_action,
            pick_folder,
            broker_ticket
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Optimus Tauri shell")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // Quit termination (R8): kill the spawned serve.
                app.state::<AppState>().serve.terminate();
            }
        });
}

#[cfg(target_os = "linux")]
fn configure_linux_webview() {
    // WebKitGTK hardware compositing produces blank frames on the CachyOS
    // Wayland/GBM stack used by the supported Linux installer. XWayland plus
    // software compositing is deterministic and avoids the startup protocol
    // error while retaining smooth CSS transforms and scrolling.
    let gdk_backend = std::env::var("GDK_BACKEND").ok();
    if std::env::var_os("DISPLAY").is_some()
        && gdk_backend
            .as_deref()
            .is_none_or(|backend| backend == "wayland")
    {
        std::env::set_var("GDK_BACKEND", "x11");
        std::env::set_var("WINIT_UNIX_BACKEND", "x11");
    }
    if std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_none() {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervised_launch_options_preserve_home_and_readiness_marker() {
        let options = parse_launch_options([
            "--unknown".into(),
            "--home".into(),
            "/tmp/optimus-child-home".into(),
            "--supervised-ready".into(),
            "/tmp/optimus-ready.json".into(),
            "--session".into(),
            "00000000-0000-4000-8000-000000000001".into(),
        ]);
        assert_eq!(options.home, Some(PathBuf::from("/tmp/optimus-child-home")));
        assert_eq!(
            options.ready_file,
            Some(PathBuf::from("/tmp/optimus-ready.json"))
        );
        assert_eq!(
            options.initial_session_id,
            Some("00000000-0000-4000-8000-000000000001".into())
        );
    }

    #[test]
    fn absent_supervision_options_leave_default_state() {
        assert_eq!(
            parse_launch_options(Vec::<String>::new()),
            LaunchOptions::default()
        );
    }
}
