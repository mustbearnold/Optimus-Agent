//! Optimus desktop shell — WebView2 host + HTTP mode for Playwright.

mod bridge;
mod ipc;
mod native_workers;
mod server;
mod ui;

use std::borrow::Cow;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::Parser;
use serde_json::json;
use tao::{
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::WindowBuilder,
};
use wry::{
    http::{header::CONTENT_TYPE, Request, Response},
    WebViewBuilder,
};

use crate::bridge::{inject_bridge, BRIDGE_JS};
use crate::ipc::{pick_folder_dialog, IpcEnvelope, IpcReply};
use crate::native_workers::NativeWorkers;
use crate::server::{run_http_server, HttpSecurity};

#[derive(Parser, Debug)]
#[command(name = "optimus-desktop", version, about = "Optimus Agent desktop")]
struct Cli {
    /// Optimus home directory (default: %LOCALAPPDATA%/optimus)
    #[arg(long, default_value = "")]
    home: String,

    /// Serve UI + JSON API on 127.0.0.1:PORT for Playwright / browser testing (no native window).
    #[arg(long)]
    http: Option<u16>,

    /// Explicitly enable the test-only HTTP UI/API surface.
    #[arg(long, requires = "http")]
    development_http: bool,
}

#[derive(Debug, Clone)]
enum UserEvent {
    Ipc {
        id: u64,
        method: String,
        params: serde_json::Value,
    },
    IpcDone(IpcReply),
    /// Progressive stream event for a pending chat_stream request.
    Stream {
        id: u64,
        payload: serde_json::Value,
    },
    PushReady,
    /// Fast path for titlebar drag (no reply needed).
    DragWindow,
    SetOuterPosition {
        x: i32,
        y: i32,
    },
}

fn requires_window_thread(method: &str) -> bool {
    matches!(
        method,
        "window_minimize"
            | "window_maximize"
            | "window_close"
            | "window_drag"
            | "window_outer_position"
            | "window_set_outer_position"
            | "pick_folder"
    )
}

fn main() -> wry::Result<()> {
    let cli = Cli::parse();
    let home = resolve_home(&cli.home);
    std::fs::create_dir_all(&home).ok();
    eprintln!("[optimus-desktop] home={}", home.display());

    if let Some(port) = cli.http {
        let token = std::env::var("OPTIMUS_HTTP_TOKEN").unwrap_or_default();
        let security = match HttpSecurity::new(port, cli.development_http, token) {
            Ok(security) => security,
            Err(error) => {
                eprintln!("[optimus-desktop] refusing HTTP mode: {error}");
                std::process::exit(2);
            }
        };
        let html = inject_bridge(&ui::render_html());
        // HTTP mode never returns Ok from wry — exit after server ends.
        if let Err(e) = run_http_server(home, port, html, security) {
            eprintln!("[optimus-desktop] http server error: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }

    run_webview(home)
}

fn run_webview(home: PathBuf) -> wry::Result<()> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let workers = NativeWorkers::start(home.clone(), proxy.clone())?;

    let mut window_builder = WindowBuilder::new()
        .with_title("Optimus Agent")
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 840.0))
        // Allow Windows snap layouts (1/4 ≈ 480×540 on 1080p; 1/2 width/height).
        .with_min_inner_size(tao::dpi::LogicalSize::new(420.0, 320.0))
        .with_resizable(true)
        // Seamless chrome: content owns the top bar (Windows app-bar integration).
        .with_decorations(false);

    #[cfg(target_os = "windows")]
    {
        use tao::platform::windows::WindowBuilderExtWindows;
        window_builder = window_builder.with_undecorated_shadow(true);
    }

    let window = window_builder.build(&event_loop).expect("window");

    let html = inject_bridge(&ui::render_html());
    let html_bytes: Cow<'static, [u8]> = Cow::Owned(html.into_bytes());

    let proxy_for_ipc = proxy.clone();
    let builder = WebViewBuilder::new()
        // Opaque dark paint so a failed/late first paint never shows a blank black HWND.
        .with_background_color((0x0a, 0x0a, 0x0c, 255))
        .with_custom_protocol("optimus".into(), move |_id, req: Request<Vec<u8>>| {
            let path = req.uri().path().to_string();
            eprintln!("[optimus-desktop] asset {path}");
            let (body, ctype) = if path == "/" || path == "/index.html" || path.is_empty() {
                (html_bytes.clone(), "text/html; charset=utf-8")
            } else {
                (
                    Cow::Borrowed(b"not found" as &[u8]),
                    "text/plain; charset=utf-8",
                )
            };
            Response::builder()
                .header(CONTENT_TYPE, ctype)
                .status(if ctype.starts_with("text/html") {
                    200
                } else {
                    404
                })
                .body(body)
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(500)
                        .body(Cow::Borrowed(b"err" as &[u8]))
                        .unwrap()
                })
        })
        .with_url("http://optimus.localhost/")
        .with_initialization_script(BRIDGE_JS)
        .with_ipc_handler(move |req: Request<String>| {
            let body = req.body().clone();
            // Fast-path drag / position without full Ipc envelope processing when possible.
            if body.contains("\"window_drag\"") && !body.contains("window_drag_") {
                let _ = proxy_for_ipc.send_event(UserEvent::DragWindow);
            }
            match serde_json::from_str::<IpcEnvelope>(&body) {
                Ok(env) => {
                    if env.method == "window_drag" {
                        // Already queued DragWindow; still enqueue Ipc for reply/tests.
                    }
                    if env.method == "window_set_outer_position" {
                        let x = env.params.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let y = env.params.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                        let _ = proxy_for_ipc.send_event(UserEvent::SetOuterPosition { x, y });
                    }
                    let _ = proxy_for_ipc.send_event(UserEvent::Ipc {
                        id: env.id,
                        method: env.method,
                        params: env.params,
                    });
                }
                Err(e) => {
                    eprintln!("[optimus-desktop] ipc parse error: {e} body={body}");
                }
            }
        })
        .with_accept_first_mouse(true);
    #[cfg(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    ))]
    let webview = builder.build(&window)?;
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    )))]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window.default_vbox().unwrap();
        builder.build_gtk(vbox)?
    };

    let proxy_bg = proxy.clone();
    let mut ready_at: Option<Instant> = None;
    let mut ready_pushed = false;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        if !ready_pushed {
            if let Some(t0) = ready_at {
                if t0.elapsed() >= Duration::from_millis(400) {
                    let _ = proxy_bg.send_event(UserEvent::PushReady);
                    ready_pushed = true;
                } else {
                    *control_flow =
                        ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(50));
                }
            }
        }

        match event {
            Event::NewEvents(StartCause::Init) => {
                ready_at = Some(Instant::now());
                *control_flow =
                    ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(50));
            }
            Event::UserEvent(UserEvent::PushReady) => {
                if let Err(error) = workers.enqueue_ready() {
                    eprintln!("[optimus-desktop] ready queue: {error}");
                    ready_pushed = false;
                    ready_at = Some(Instant::now());
                    *control_flow =
                        ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(50));
                }
            }
            Event::UserEvent(UserEvent::DragWindow) => {
                if let Err(e) = window.drag_window() {
                    eprintln!("[optimus-desktop] DragWindow failed: {e}");
                }
            }
            Event::UserEvent(UserEvent::SetOuterPosition { x, y }) => {
                window.set_outer_position(tao::dpi::PhysicalPosition::new(x, y));
            }
            Event::UserEvent(UserEvent::Ipc { id, method, params }) => {
                eprintln!("[optimus-desktop] handle {method} id={id}");
                // Window chrome controls (need live Window handle)
                if requires_window_thread(&method) {
                    let reply = match method.as_str() {
                        "window_minimize" => {
                            window.set_minimized(true);
                            IpcReply {
                                id,
                                ok: true,
                                result: Some(json!({"ok": true})),
                                error: None,
                            }
                        }
                        "window_maximize" => {
                            window.set_maximized(!window.is_maximized());
                            IpcReply {
                                id,
                                ok: true,
                                result: Some(json!({"maximized": window.is_maximized()})),
                                error: None,
                            }
                        }
                        "window_close" => {
                            *control_flow = ControlFlow::Exit;
                            IpcReply {
                                id,
                                ok: true,
                                result: Some(json!({"ok": true})),
                                error: None,
                            }
                        }
                        "window_drag" => {
                            // Prefer DragWindow fast-path; this reply path is for tests/HTTP.
                            // Avoid double ReleaseCapture if DragWindow already ran — still OK to try.
                            let _ = window.drag_window();
                            IpcReply {
                                id,
                                ok: true,
                                result: Some(json!({"ok": true})),
                                error: None,
                            }
                        }
                        "window_outer_position" => match window.outer_position() {
                            Ok(p) => IpcReply {
                                id,
                                ok: true,
                                result: Some(json!({"x": p.x, "y": p.y})),
                                error: None,
                            },
                            Err(e) => IpcReply {
                                id,
                                ok: false,
                                result: None,
                                error: Some(format!("outer_position: {e}")),
                            },
                        },
                        "window_set_outer_position" => {
                            // Also handled via SetOuterPosition fast-path; ack here.
                            let x = params.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                            let y = params.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                            window.set_outer_position(tao::dpi::PhysicalPosition::new(x, y));
                            IpcReply {
                                id,
                                ok: true,
                                result: Some(json!({"ok": true, "x": x, "y": y})),
                                error: None,
                            }
                        }
                        "pick_folder" => match pick_folder_dialog() {
                            Ok(v) => IpcReply {
                                id,
                                ok: true,
                                result: Some(v),
                                error: None,
                            },
                            Err(e) => IpcReply {
                                id,
                                ok: false,
                                result: None,
                                error: Some(e),
                            },
                        },
                        _ => unreachable!(),
                    };
                    let _ = eval_reply(&webview, &reply);
                    return;
                }
                if method == "chat_stream" {
                    if let Err(error) = workers.enqueue_stream(id, params) {
                        let _ = proxy_bg.send_event(UserEvent::Stream {
                            id,
                            payload: json!({"type":"error","error": format!("chat worker {error}")}),
                        });
                    }
                } else if let Err(error) = workers.enqueue_request(id, method, params) {
                    let _ = eval_reply(
                        &webview,
                        &IpcReply {
                            id,
                            ok: false,
                            result: None,
                            error: Some(format!("IPC worker {error}")),
                        },
                    );
                }
            }
            Event::UserEvent(UserEvent::Stream { id, payload }) => {
                let body = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
                let script = format!(
                    "try {{ window.__optimusStream({id}, {body}); }} catch (e) {{ console.error(e); }}"
                );
                let _ = webview.evaluate_script(&script);
            }
            Event::UserEvent(UserEvent::IpcDone(reply)) => {
                let _ = eval_reply(&webview, &reply);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            _ => {}
        }
    });
}

fn resolve_home(home: &str) -> PathBuf {
    if !home.is_empty() {
        let p = PathBuf::from(home);
        if p.is_absolute() {
            return p;
        }
        return std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(p);
    }
    if let Some(data) = dirs::data_local_dir() {
        return data.join("optimus");
    }
    PathBuf::from(".optimus")
}

fn eval_reply(webview: &wry::WebView, reply: &IpcReply) -> Result<(), String> {
    let payload = serde_json::to_string(reply).map_err(|e| e.to_string())?;
    let script =
        format!("try {{ window.__optimusIpcReply({payload}); }} catch (e) {{ console.error(e); }}");
    webview.evaluate_script(&script).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{requires_window_thread, IpcReply, UserEvent};

    fn assert_send_static<T: Send + 'static>() {}

    #[test]
    fn worker_payloads_remain_send_and_static() {
        assert_send_static::<UserEvent>();
        assert_send_static::<IpcReply>();
    }

    #[test]
    fn only_live_window_and_dialog_methods_require_the_event_loop() {
        for method in [
            "ping",
            "doctor",
            "window_minimize",
            "window_maximize",
            "window_close",
            "window_drag",
            "window_outer_position",
            "window_set_outer_position",
            "pick_folder",
            "auth_status",
            "auth_import_hermes",
            "auth_import_cli",
            "sessions",
            "delete_session",
            "rename_session",
            "open_path",
            "new_session",
            "get_session",
            "cron_list",
            "cron_add",
            "cron_tick",
            "approvals_list",
            "approvals_grant",
            "jobs_list",
            "campaign_list",
            "campaign_create",
            "campaign_run",
            "campaign_status",
            "fs_roots",
            "fs_list",
            "fs_read",
            "term_run",
            "chat",
            "chat_offline",
            "chat_stream",
        ] {
            let expected = matches!(
                method,
                "window_minimize"
                    | "window_maximize"
                    | "window_close"
                    | "window_drag"
                    | "window_outer_position"
                    | "window_set_outer_position"
                    | "pick_folder"
            );
            assert_eq!(requires_window_thread(method), expected, "{method}");
        }
    }
}
