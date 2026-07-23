//! CDP (Chrome DevTools Protocol) browser effector for Optimus Agent.
//!
//! Wraps `headless_chrome` for tab lifecycle, navigation, screenshots,
//! DOM snapshots with bounding boxes, Set-of-Mark (SOM) numbered overlays,
//! and click-by-index interaction.

use std::fs;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use headless_chrome::{
    browser::tab::{point::Point, RequestPausedDecision},
    browser::transport::{SessionId, Transport},
    protocol::cdp::Fetch::events::RequestPausedEvent,
    protocol::cdp::{Fetch, Network, Page},
    Browser, LaunchOptions, LaunchOptionsBuilder, Tab,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum BrowserError {
    #[error("CDP browser: {0}")]
    Cdp(String),
    #[error("launch failed: {0}")]
    Launch(String),
    #[error("navigation failed: {0}")]
    Navigation(String),
    #[error("screenshot failed: {0}")]
    Screenshot(String),
    #[error("DOM snapshot failed: {0}")]
    DomSnapshot(String),
    #[error("click failed: {0}")]
    Click(String),
    #[error("unsafe browser URL: {0}")]
    Ssrf(String),
    #[error("unsafe browser state path: {0}")]
    UnsafeStatePath(String),
    #[error("tab not open")]
    TabNotOpen,
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, BrowserError>;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single interactable DOM element with its SOM index and bounding box.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomElement {
    pub index: usize,
    pub tag: String,
    pub text: String,
    pub bounds: Bounds,
    pub interactive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Output of a SOM capture: screenshot + element list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SomCapture {
    pub screenshot_b64: String,
    pub elements: Vec<DomElement>,
    pub element_count: usize,
}

/// Page state after navigation or click.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageState {
    pub url: String,
    pub title: String,
    pub screenshot_b64: String,
    pub elements: Vec<DomElement>,
    pub element_count: usize,
}

// ---------------------------------------------------------------------------
// Launch options
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BrowserOptions {
    pub headless: bool,
    pub chrome_path: Option<PathBuf>,
    pub port: u16,
    pub timeout_secs: u64,
    pub user_data_dir: Option<PathBuf>,
    pub window_size: (u32, u32),
}

impl Default for BrowserOptions {
    fn default() -> Self {
        Self {
            headless: true,
            chrome_path: None,
            port: 9222,
            timeout_secs: 15,
            user_data_dir: None,
            window_size: (1280, 720),
        }
    }
}

fn build_launch_options(opts: &BrowserOptions) -> Result<LaunchOptions<'static>> {
    let mut builder = LaunchOptionsBuilder::default();
    builder.headless(opts.headless);
    builder.window_size(Some(opts.window_size));
    builder.port(Some(opts.port));
    builder.idle_browser_timeout(Duration::from_secs(opts.timeout_secs));
    builder.sandbox(true);
    if let Some(ref path) = opts.chrome_path {
        builder.path(Some(path.clone()));
    }
    if let Some(ref path) = opts.user_data_dir {
        builder.user_data_dir(Some(path.clone()));
    }
    builder
        .build()
        .map_err(|error| BrowserError::Launch(error.to_string()))
}

fn prepare_state_paths(
    workspace: &Path,
    user_data_dir: Option<&Path>,
) -> Result<(PathBuf, Option<PathBuf>)> {
    fs::create_dir_all(workspace)?;
    let workspace = fs::canonicalize(workspace)?;
    let state_root = workspace.join(".optimus");
    if fs::symlink_metadata(&state_root).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(BrowserError::UnsafeStatePath(
            ".optimus must not be a symlink".into(),
        ));
    }
    fs::create_dir_all(&state_root)?;
    let state_root = fs::canonicalize(&state_root)?;
    if state_root.parent() != Some(workspace.as_path()) {
        return Err(BrowserError::UnsafeStatePath(
            ".optimus resolved outside the workspace".into(),
        ));
    }

    let profile = match user_data_dir {
        None => None,
        Some(requested) => {
            let requested = if requested.is_absolute() {
                requested.to_path_buf()
            } else {
                workspace.join(requested)
            };
            if requested
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
                || !requested.starts_with(&state_root)
            {
                return Err(BrowserError::UnsafeStatePath(
                    "profile must be inside the workspace .optimus directory".into(),
                ));
            }
            let relative = requested.strip_prefix(&state_root).map_err(|_| {
                BrowserError::UnsafeStatePath(
                    "profile must be inside the workspace .optimus directory".into(),
                )
            })?;
            let mut cursor = state_root.clone();
            for component in relative.components() {
                cursor.push(component);
                if fs::symlink_metadata(&cursor).is_ok_and(|meta| meta.file_type().is_symlink()) {
                    return Err(BrowserError::UnsafeStatePath(
                        "profile path must not contain symlinks".into(),
                    ));
                }
            }
            fs::create_dir_all(&requested)?;
            let canonical = fs::canonicalize(&requested)?;
            if !canonical.starts_with(&state_root) {
                return Err(BrowserError::UnsafeStatePath(
                    "profile resolved outside the workspace .optimus directory".into(),
                ));
            }
            Some(canonical)
        }
    };
    Ok((state_root.join("browser_state.json"), profile))
}

fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            a == 0
                || a == 10
                || a == 127
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && matches!(b, 0 | 168))
                || (a == 198 && matches!(b, 18 | 19 | 51))
                || (a == 203 && b == 0)
                || a >= 224
        }
        IpAddr::V6(ip) => {
            let octets = ip.octets();
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_forbidden_ip(IpAddr::V4(mapped));
            }
            ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || (octets[0] & 0xfe) == 0xfc
                || (octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80)
                || octets[0..4] == [0x20, 0x01, 0x0d, 0xb8]
        }
    }
}

fn validate_network_url(url: &Url) -> Result<()> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(BrowserError::Ssrf(format!(
            "scheme {} is not allowed",
            url.scheme()
        )));
    }
    let host = url
        .host_str()
        .ok_or_else(|| BrowserError::Ssrf("missing host".into()))?;
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    if normalized == "localhost"
        || normalized.ends_with(".localhost")
        || normalized.ends_with(".local")
        || normalized == "metadata.google.internal"
    {
        return Err(BrowserError::Ssrf("local host is not allowed".into()));
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| BrowserError::Ssrf("missing network port".into()))?;
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|_| BrowserError::Ssrf("host resolution failed".into()))?
        .collect::<Vec<_>>();
    if addresses.is_empty() || addresses.iter().any(|addr| is_forbidden_ip(addr.ip())) {
        return Err(BrowserError::Ssrf(
            "host resolves to a non-public address".into(),
        ));
    }
    Ok(())
}

fn validate_navigation_url(value: &str) -> Result<()> {
    let url = Url::parse(value).map_err(|error| BrowserError::Ssrf(error.to_string()))?;
    validate_network_url(&url)
}

fn is_safe_request_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    match url.scheme() {
        "about" | "blob" | "data" => true,
        "http" | "https" => validate_network_url(&url).is_ok(),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// CDP Browser Session
// ---------------------------------------------------------------------------

pub struct CdpBrowserSession {
    _path: PathBuf,
    browser: Option<Arc<Browser>>,
    tab: Option<Arc<Tab>>,
}

impl CdpBrowserSession {
    /// Open a new CDP browser session, launching Chromium if needed.
    pub fn open(workspace: impl AsRef<Path>, opts: BrowserOptions) -> Result<Self> {
        let (path, user_data_dir) =
            prepare_state_paths(workspace.as_ref(), opts.user_data_dir.as_deref())?;
        let mut safe_opts = opts;
        safe_opts.user_data_dir = user_data_dir;
        let launch_opts = build_launch_options(&safe_opts)?;

        let browser = Browser::new(launch_opts).map_err(|e| BrowserError::Launch(e.to_string()))?;

        let browser_arc = Arc::new(browser);

        // Open first tab
        let tab = browser_arc
            .new_tab()
            .map_err(|e| BrowserError::Launch(e.to_string()))?;
        let patterns = [Fetch::RequestPattern {
            url_pattern: None,
            resource_Type: None,
            request_stage: Some(Fetch::RequestStage::Request),
        }];
        tab.enable_fetch(Some(&patterns), None)
            .map_err(|error| BrowserError::Launch(error.to_string()))?;
        tab.enable_request_interception(Arc::new(
            |_: Arc<Transport>, _: SessionId, intercepted: RequestPausedEvent| {
                if is_safe_request_url(&intercepted.params.request.url) {
                    RequestPausedDecision::Continue(None)
                } else {
                    RequestPausedDecision::Fail(Fetch::FailRequest {
                        request_id: intercepted.params.request_id,
                        error_reason: Network::ErrorReason::BlockedByClient,
                    })
                }
            },
        ))
        .map_err(|error| BrowserError::Launch(error.to_string()))?;

        Ok(Self {
            _path: path,
            browser: Some(browser_arc),
            tab: Some(tab),
        })
    }

    /// Navigate to a URL, wait for load, return page state.
    pub fn navigate(&mut self, url: &str) -> Result<PageState> {
        validate_navigation_url(url)?;
        let tab = self.tab_ref()?;

        tab.navigate_to(url)
            .map_err(|e| BrowserError::Navigation(e.to_string()))?;

        tab.wait_until_navigated()
            .map_err(|e| BrowserError::Navigation(e.to_string()))?;

        validate_navigation_url(&tab.get_url())?;

        Self::capture_page_state(tab)
    }

    /// Viewport screenshot as base64 PNG.
    pub fn screenshot(&self) -> Result<String> {
        let tab = self.tab_ref()?;

        let png = tab
            .capture_screenshot(Page::CaptureScreenshotFormatOption::Png, None, None, true)
            .map_err(|e| BrowserError::Screenshot(e.to_string()))?;

        Ok(base64_encode_png(&png))
    }

    /// DOM snapshot — interactable elements with bounding boxes.
    pub fn dom_snapshot(&self) -> Result<Vec<DomElement>> {
        let tab = self.tab_ref()?;
        let js = include_str!("dom_snapshot.js");

        let remote_obj = tab
            .evaluate(js, false)
            .map_err(|e| BrowserError::DomSnapshot(e.to_string()))?;

        let elements: Vec<DomElement> = remote_obj
            .value
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        Ok(elements)
    }

    /// Full SOM capture: screenshot + DOM elements.
    pub fn som_capture(&self) -> Result<SomCapture> {
        let elements = self.dom_snapshot()?;
        let screenshot_b64 = self.screenshot()?;
        let count = elements.len();

        Ok(SomCapture {
            screenshot_b64,
            elements,
            element_count: count,
        })
    }

    /// Click element by SOM index (1-based).
    pub fn click(&mut self, index: usize) -> Result<PageState> {
        let elements = self.dom_snapshot()?;
        let elem = elements
            .iter()
            .find(|e| e.index == index)
            .ok_or_else(|| BrowserError::Click(format!("no element with SOM index {index}")))?;

        let x = elem.bounds.x + elem.bounds.width / 2.0;
        let y = elem.bounds.y + elem.bounds.height / 2.0;

        let tab = self.tab_ref()?;

        tab.click_point(Point { x, y })
            .map_err(|e| BrowserError::Click(e.to_string()))?;

        std::thread::sleep(Duration::from_millis(500));
        let _ = tab.wait_until_navigated();

        Self::capture_page_state(tab)
    }

    /// Close the browser session.
    pub fn close(&mut self) -> Result<()> {
        if let Some(tab) = self.tab.take() {
            let _ = tab.close(false);
        }
        if let Some(browser) = self.browser.take() {
            drop(browser);
        }
        Ok(())
    }

    /// Current page title.
    pub fn title(&self) -> Result<String> {
        let tab = self.tab_ref()?;
        tab.get_title()
            .map_err(|e| BrowserError::Cdp(e.to_string()))
    }

    /// Current URL.
    pub fn current_url(&self) -> String {
        self.tab_ref().map(|t| t.get_url()).unwrap_or_default()
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    fn tab_ref(&self) -> Result<&Tab> {
        self.tab.as_deref().ok_or_else(|| BrowserError::TabNotOpen)
    }

    fn capture_page_state(tab: &Tab) -> Result<PageState> {
        let url = tab.get_url();
        let title = tab.get_title().unwrap_or_default();

        let png = tab
            .capture_screenshot(Page::CaptureScreenshotFormatOption::Png, None, None, true)
            .map_err(|e| BrowserError::Screenshot(e.to_string()))?;

        let screenshot_b64 = base64_encode_png(&png);

        let js = include_str!("dom_snapshot.js");
        let elements: Vec<DomElement> = tab
            .evaluate(js, false)
            .ok()
            .and_then(|r| r.value)
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        Ok(PageState {
            url,
            title,
            screenshot_b64,
            element_count: elements.len(),
            elements,
        })
    }
}

impl Drop for CdpBrowserSession {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

fn base64_encode_png(png_bytes: &[u8]) -> String {
    use base64::engine::Engine;
    base64::engine::general_purpose::STANDARD.encode(png_bytes)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_policy_rejects_local_and_non_http_urls() {
        for url in [
            "http://127.0.0.1/",
            "http://[::1]/",
            "http://169.254.169.254/latest/meta-data/",
            "http://metadata.google.internal/",
            "file:///etc/passwd",
        ] {
            assert!(
                matches!(validate_navigation_url(url), Err(BrowserError::Ssrf(_))),
                "expected navigation to reject {url}"
            );
        }
        assert!(validate_navigation_url("https://93.184.216.34/path").is_ok());
    }

    #[test]
    fn request_policy_blocks_network_pivots_but_allows_inline_resources() {
        assert!(!is_safe_request_url("http://10.0.0.1/private"));
        assert!(!is_safe_request_url("http://localhost/admin"));
        assert!(!is_safe_request_url("file:///etc/passwd"));
        assert!(is_safe_request_url("https://93.184.216.34/app.js"));
        assert!(is_safe_request_url("data:text/plain,hello"));
        assert!(is_safe_request_url("blob:https://example.com/id"));
    }

    #[test]
    fn launch_options_keep_the_chromium_sandbox_enabled() {
        let options = build_launch_options(&BrowserOptions::default()).expect("launch options");
        assert!(options.sandbox, "Chromium sandbox must remain enabled");
    }

    #[cfg(unix)]
    #[test]
    fn state_root_rejects_symlinked_optimus_directory() {
        use std::os::unix::fs::symlink;

        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), workspace.path().join(".optimus")).unwrap();

        let error = prepare_state_paths(workspace.path(), None).unwrap_err();
        assert!(matches!(error, BrowserError::UnsafeStatePath(_)));
    }

    #[test]
    fn dom_element_roundtrip() {
        let el = DomElement {
            index: 1,
            tag: "button".into(),
            text: "Submit".into(),
            bounds: Bounds {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 40.0,
            },
            interactive: true,
        };
        let json = serde_json::to_string_pretty(&el).unwrap();
        let back: DomElement = serde_json::from_str(&json).unwrap();
        assert_eq!(back.index, 1);
    }

    #[test]
    fn som_capture_roundtrip() {
        let cap = SomCapture {
            screenshot_b64: "iVBOR".into(),
            elements: vec![],
            element_count: 0,
        };
        let json = serde_json::to_string_pretty(&cap).unwrap();
        let back: SomCapture = serde_json::from_str(&json).unwrap();
        assert_eq!(back.element_count, 0);
    }

    #[test]
    #[cfg_attr(not(feature = "cdp_live_tests"), ignore)]
    fn live_navigate_and_screenshot() {
        let dir = tempfile::tempdir().unwrap();
        let opts = BrowserOptions {
            headless: true,
            port: 0,
            ..Default::default()
        };
        let mut session = CdpBrowserSession::open(dir.path(), opts).unwrap();
        let state = session.navigate("https://example.com").unwrap();
        assert!(state.url.contains("example.com"));
        assert!(!state.screenshot_b64.is_empty());
        session.close().unwrap();
    }
}
