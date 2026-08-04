//! Optimus desktop shell — native Wry webview, HTTP host (Playwright / Electron).

mod bridge;
mod host_runtime;
mod native_workers;
mod preview_embed;
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
    dpi::{LogicalPosition, LogicalSize},
    http::{header::CONTENT_TYPE, Request, Response},
    Rect, WebViewBuilder,
};

use crate::bridge::{inject_bridge, BRIDGE_JS};
use crate::native_workers::NativeWorkers;
use crate::preview_embed::{navigation_allowed as preview_nav_allowed, EmbedBounds, PreviewEmbed};
use crate::server::{run_http_server, HttpSecurity};
use optimus_host::{pick_folder_dialog, IpcEnvelope, IpcReply};

/// Default loopback port for `--host-only` (Electron / external shells).
const DEFAULT_HOST_PORT: u16 = 17865;

// Wry translates custom schemes to an HTTP `.localhost` origin only on
// WebView2 and Android. WebKitGTK, WebKit, and WKWebView use the scheme itself.
#[cfg(any(target_os = "windows", target_os = "android"))]
const NATIVE_WEBVIEW_URL: &str = "http://optimus.localhost/";
#[cfg(any(target_os = "windows", target_os = "android"))]
const NATIVE_WEBVIEW_ORIGIN: &str = "http://optimus.localhost";
#[cfg(not(any(target_os = "windows", target_os = "android")))]
const NATIVE_WEBVIEW_URL: &str = "optimus://localhost/";
#[cfg(not(any(target_os = "windows", target_os = "android")))]
const NATIVE_WEBVIEW_ORIGIN: &str = "optimus://localhost";

fn native_navigation_allowed(url: &str) -> bool {
    if url == "about:blank" {
        return true;
    }
    url.strip_prefix(NATIVE_WEBVIEW_ORIGIN).is_some_and(|rest| {
        rest.is_empty() || rest.starts_with('/') || rest.starts_with('?') || rest.starts_with('#')
    })
}

#[derive(Parser, Debug)]
#[command(name = "optimus-desktop", version, about = "Optimus Agent desktop")]
struct Cli {
    /// Optimus home directory (default: the platform-local data directory/optimus)
    #[arg(long, default_value = "")]
    home: String,

    /// Serve UI + JSON API on 127.0.0.1:PORT for Playwright / browser testing (no native window).
    #[arg(long)]
    http: Option<u16>,

    /// Headless Rust host for Electron (and other shells). No Wry window.
    /// Uses OPTIMUS_HTTP_TOKEN (or generates one) and binds loopback HTTP+IPC.
    #[arg(long, conflicts_with = "http")]
    host_only: bool,

    /// Port for `--host-only` (default 17865). Ignored unless `--host-only`.
    #[arg(long, default_value_t = DEFAULT_HOST_PORT)]
    host_port: u16,

    /// Explicitly enable the HTTP UI/API surface (required for `--http` tests;
    /// implied by `--host-only` for the Electron product path).
    #[arg(long)]
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
    /// Live preview browser navigated (update omnibox / status).
    BrowserNavigated {
        url: String,
    },
    /// Annotation pin emitted from the live preview page.
    BrowserAnnotation {
        params: serde_json::Value,
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
            | "browser_embed"
            | "browser_navigate"
            | "browser_reload"
            | "browser_back"
            | "browser_forward"
            | "browser_set_annotate"
    )
}

fn main() -> wry::Result<()> {
    let cli = Cli::parse();
    let home = optimus_host::resolve_home(Some(cli.home.as_str()));
    std::fs::create_dir_all(&home).ok();
    eprintln!("[optimus-desktop] home={}", home.display());

    if cli.host_only || cli.http.is_some() {
        let port = if cli.host_only {
            cli.host_port
        } else {
            cli.http.expect("http port")
        };
        let development = cli.development_http || cli.host_only;
        let token = std::env::var("OPTIMUS_HTTP_TOKEN").unwrap_or_else(|_| {
            if cli.host_only {
                // Product host path: mint a process-local token if unset.
                format!(
                    "optimus-host-{}-{}",
                    std::process::id(),
                    uuid::Uuid::new_v4().simple()
                )
            } else {
                String::new()
            }
        });
        if cli.host_only {
            // One core per home (C3): a healthy host already serving this
            // home means the caller should have attached, not spawned.
            if let Some(served_port) = host_runtime::healthy_serving_port(&home) {
                eprintln!(
                    "[optimus-desktop] refusing second core: home {} is already served by a \
                     healthy host on 127.0.0.1:{served_port} — probe and attach (C3)",
                    home.display()
                );
                std::process::exit(3);
            }
            eprintln!("[optimus-desktop] host-only mode on 127.0.0.1:{port}");
            // Electron parent reads this line to pair Authorization.
            if std::env::var_os("OPTIMUS_SUPPRESS_TOKEN_LOG").is_none() {
                eprintln!("[optimus-desktop] OPTIMUS_HTTP_TOKEN={token}");
            }
        }
        let security = match HttpSecurity::new(port, development, token) {
            Ok(security) => security,
            Err(error) => {
                eprintln!("[optimus-desktop] refusing HTTP/host mode: {error}");
                std::process::exit(2);
            }
        };
        let html = inject_bridge(&ui::render_html());
        if let Err(e) = run_http_server(home, port, html, security, cli.host_only) {
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

    let window_builder = WindowBuilder::new()
        .with_title("Optimus Agent")
        .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 840.0))
        // Remains usable in compact tiling/snap layouts.
        .with_min_inner_size(tao::dpi::LogicalSize::new(420.0, 320.0))
        .with_resizable(true)
        // Seamless chrome: content owns the cross-platform top bar.
        .with_decorations(false);

    #[cfg(target_os = "windows")]
    let window_builder = {
        use tao::platform::windows::WindowBuilderExtWindows;
        window_builder.with_undecorated_shadow(true)
    };

    let window = window_builder.build(&event_loop).expect("window");

    let html = inject_bridge(&ui::render_html());
    let html_bytes: Cow<'static, [u8]> = Cow::Owned(html.into_bytes());

    let initial = window.inner_size().to_logical::<u32>(window.scale_factor());

    let proxy_for_ipc = proxy.clone();
    let ui_builder = WebViewBuilder::new()
        // Opaque dark paint prevents a bright/blank native surface before first paint.
        .with_background_color((0x0a, 0x0a, 0x0c, 255))
        .with_bounds(Rect {
            position: LogicalPosition::new(0, 0).into(),
            size: LogicalSize::new(initial.width, initial.height).into(),
        })
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
                .header(
                    "Content-Security-Policy",
                    "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-src 'none'",
                )
                .header("X-Content-Type-Options", "nosniff")
                .header("Referrer-Policy", "no-referrer")
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
        .with_url(NATIVE_WEBVIEW_URL)
        .with_navigation_handler(|url| native_navigation_allowed(&url))
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_initialization_script(BRIDGE_JS)
        .with_ipc_handler(move |req: Request<String>| {
            let body = req.body().clone();
            // Fast-path drag / position without full Ipc envelope processing when possible.
            if body.contains("\"window_drag\"") && !body.contains("window_drag_") {
                let _ = proxy_for_ipc.send_event(UserEvent::DragWindow);
            }
            match serde_json::from_str::<IpcEnvelope>(&body) {
                Ok(env) => {
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

    let proxy_for_browser = proxy.clone();
    let browser_builder = WebViewBuilder::new()
        .with_background_color((0xff, 0xff, 0xff, 255))
        .with_bounds(Rect {
            position: LogicalPosition::new(0, 0).into(),
            size: LogicalSize::new(1, 1).into(),
        })
        .with_visible(false)
        // Warm Google as the default homepage so the first Browser tab open is
        // already painted (no blank flash / cold navigation).
        .with_url("https://www.google.com/")
        .with_navigation_handler(move |url| {
            // Annotation pins can arrive as a cancelled navigation when child
            // webview IPC is unavailable.
            if let Some(params) = preview_embed::annotation_from_nav_url(&url) {
                eprintln!("[optimus-desktop] browser_annotation via nav callback");
                let _ = proxy_for_browser.send_event(UserEvent::BrowserAnnotation { params });
                return false;
            }
            let allowed = preview_nav_allowed(&url);
            if allowed {
                let _ = proxy_for_browser.send_event(UserEvent::BrowserNavigated { url });
            }
            allowed
        })
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_ipc_handler({
            let proxy = proxy.clone();
            move |req: Request<String>| {
                let body = req.body().clone();
                eprintln!(
                    "[optimus-desktop] browser ipc raw len={} head={}",
                    body.len(),
                    body.chars().take(120).collect::<String>()
                );
                // Wry/WebKit may deliver a JSON object string, or a JSON-encoded string.
                let envelope = match serde_json::from_str::<IpcEnvelope>(&body) {
                    Ok(env) => Some(env),
                    Err(_) => serde_json::from_str::<String>(&body)
                        .ok()
                        .and_then(|inner| serde_json::from_str::<IpcEnvelope>(&inner).ok()),
                };
                match envelope {
                    Some(env) if env.method == "browser_annotation" => {
                        eprintln!("[optimus-desktop] browser_annotation received");
                        let _ =
                            proxy.send_event(UserEvent::BrowserAnnotation { params: env.params });
                    }
                    Some(env) => {
                        eprintln!(
                            "[optimus-desktop] browser ipc ignored method={}",
                            env.method
                        );
                    }
                    None => {
                        eprintln!("[optimus-desktop] browser ipc parse error body={body}");
                    }
                }
            }
        })
        .with_accept_first_mouse(true);

    // Shared Fixed container on Linux so both webviews can be absolutely positioned.
    #[cfg(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    ))]
    let (webview, browser_wv) = {
        let webview = ui_builder.build_as_child(&window)?;
        let browser_wv = browser_builder.build_as_child(&window)?;
        (webview, browser_wv)
    };
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    )))]
    let (webview, browser_wv) = {
        use gtk::prelude::*;
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;

        let fixed = gtk::Fixed::new();
        let vbox = window.default_vbox().unwrap();
        vbox.pack_start(&fixed, true, true, 0);
        fixed.show_all();
        let webview = ui_builder.build_gtk(&fixed)?;
        let browser_wv = browser_builder.build_gtk(&fixed)?;
        (webview, browser_wv)
    };

    let mut preview = PreviewEmbed::new(browser_wv);

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
            Event::UserEvent(UserEvent::BrowserNavigated { url }) => {
                preview.note_url_if_empty(&url);
                let payload = preview.on_navigated(url);
                let body = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
                let script = format!(
                    "try {{ window.__optimusBrowserPush && window.__optimusBrowserPush({body}); }} catch (e) {{ console.error(e); }}"
                );
                let _ = webview.evaluate_script(&script);
            }
            Event::UserEvent(UserEvent::BrowserAnnotation { params }) => {
                let body = serde_json::to_string(&params).unwrap_or_else(|_| "{}".into());
                let script = format!(
                    "try {{ window.__optimusBrowserAnnotation && window.__optimusBrowserAnnotation({body}); }} catch (e) {{ console.error(e); }}"
                );
                let _ = webview.evaluate_script(&script);
            }
            Event::UserEvent(UserEvent::Ipc { id, method, params }) => {
                eprintln!("[optimus-desktop] handle {method} id={id}");
                // Window chrome controls + live browser embed (need live WebView handles)
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
                        "pick_folder" => match pick_folder_dialog(&home) {
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
                        "browser_embed" => {
                            match preview.apply_bounds(EmbedBounds::from_params(&params)) {
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
                            }
                        }
                        "browser_navigate" => {
                            let url = params
                                .get("url")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default();
                            match preview.navigate(url) {
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
                            }
                        }
                        "browser_reload" => match preview.reload() {
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
                        "browser_back" => match preview.back() {
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
                        "browser_forward" => match preview.forward() {
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
                        "browser_set_annotate" => {
                            let enabled = params
                                .get("enabled")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            match preview.set_annotate(enabled) {
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
                            }
                        }
                        _ => unreachable!(),
                    };
                    let _ = eval_reply(&webview, &reply);
                    return;
                }
                if method == "chat_cancel" {
                    let stream_id = params.get("stream_id").and_then(|value| value.as_u64());
                    let reply = match stream_id {
                        Some(stream_id) => IpcReply {
                            id,
                            ok: true,
                            result: Some(json!({
                                "requested": workers.cancel_stream(stream_id),
                                "stream_id": stream_id,
                            })),
                            error: None,
                        },
                        None => IpcReply {
                            id,
                            ok: false,
                            result: None,
                            error: Some("stream_id required".into()),
                        },
                    };
                    let _ = eval_reply(&webview, &reply);
                } else if method == "chat_stream" {
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
                event: WindowEvent::Resized(size),
                ..
            } => {
                let logical = size.to_logical::<u32>(window.scale_factor());
                PreviewEmbed::resize_main_fill(&webview, logical.width, logical.height);
                // Main resize can disturb child z-order; raise without remapping.
                preview.reassert_bounds();
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *control_flow = ControlFlow::Exit,
            _ => {}
        }
    });
}

fn eval_reply(webview: &wry::WebView, reply: &IpcReply) -> Result<(), String> {
    let payload = serde_json::to_string(reply).map_err(|e| e.to_string())?;
    let script =
        format!("try {{ window.__optimusIpcReply({payload}); }} catch (e) {{ console.error(e); }}");
    webview.evaluate_script(&script).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        native_navigation_allowed, requires_window_thread, IpcReply, UserEvent, DEFAULT_HOST_PORT,
    };

    fn assert_send_static<T: Send + 'static>() {}

    #[test]
    fn host_only_default_port_is_stable() {
        assert_eq!(DEFAULT_HOST_PORT, 17865);
    }

    #[test]
    fn worker_payloads_remain_send_and_static() {
        assert_send_static::<UserEvent>();
        assert_send_static::<IpcReply>();
    }

    #[test]
    fn only_live_window_dialog_and_browser_embed_methods_require_the_event_loop() {
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
            "browser_embed",
            "browser_navigate",
            "browser_reload",
            "browser_back",
            "browser_forward",
            "browser_set_annotate",
            "auth_status",
            "chat",
            "chat_stream",
            "term_run",
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
                    | "browser_embed"
                    | "browser_navigate"
                    | "browser_reload"
                    | "browser_back"
                    | "browser_forward"
                    | "browser_set_annotate"
            );
            assert_eq!(requires_window_thread(method), expected, "{method}");
        }
    }

    #[test]
    fn privileged_webview_navigation_is_confined_to_the_packaged_origin() {
        assert!(native_navigation_allowed(super::NATIVE_WEBVIEW_URL));
        assert!(native_navigation_allowed("about:blank"));
        assert!(!native_navigation_allowed("https://evil.example/"));
        assert!(!native_navigation_allowed("file:///etc/passwd"));
    }
}
