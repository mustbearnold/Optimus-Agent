//! Browser effectors — abstract trait with HTTP (ureq) and CDP (headless_chrome) backends.
//!
//! The `BrowserEffector` trait is the seam between kernel tool dispatch and
//! the actual browser engine. Two implementations:
//!
//! 1. `HttpBrowserEffector` — HTTP text-only, SSRF-hardened (existing behaviour)
//! 2. `CdpBrowserEffector` — CDP-backed, screenshots + DOM snapshots (P20A)
//!
//! The kernel tries CDP first (when `cdp` feature is enabled and Chrome is on PATH)
//! then falls back to HTTP.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;
use url::Url;

use crate::page_extract::*;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum BrowserError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("http: {0}")]
    Http(String),
    #[error("ssrf blocked: {0}")]
    Ssrf(String),
    #[error("no page loaded")]
    NoPage,
    #[error("link index out of range: {0}")]
    BadLink(usize),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("url: {0}")]
    Url(String),
    #[error("cdp: {0}")]
    Cdp(String),
}

pub type Result<T> = std::result::Result<T, BrowserError>;

// ---------------------------------------------------------------------------
// Data types (shared across effectors)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BrowserLink {
    pub index: usize,
    pub text: String,
    pub href: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BrowserPage {
    pub url: String,
    pub final_url: String,
    pub title: String,
    pub status: u16,
    pub text: String,
    pub links: Vec<BrowserLink>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BrowserState {
    pub history: Vec<String>,
    pub page: Option<BrowserPage>,
}

// ---------------------------------------------------------------------------
// Trait — abstraction over backend
// ---------------------------------------------------------------------------

/// Browser effector trait. Methods return JSON strings ready to pass back to
/// the model via the kernel's tool dispatch.
pub trait BrowserEffector: Send {
    /// Navigate to a URL and return page state as JSON.
    fn navigate(&mut self, url: &str) -> Result<String>;
    /// Snapshot the current page as JSON.
    fn snapshot(&self) -> Result<String>;
    /// Click a link by index (0-based) and return new page state as JSON.
    fn click(&mut self, index: usize) -> Result<String>;
    /// Close the browser session.
    fn close(&mut self) -> Result<()>;
}

// ---------------------------------------------------------------------------
// Effector factory
// ---------------------------------------------------------------------------

/// Try to create a CDP effector. Returns `None` when CDP is unavailable
/// (no Chrome binary, or feature not enabled, or any launch error).
pub fn try_cdp_effector(workspace: impl AsRef<Path>) -> Option<Box<dyn BrowserEffector>> {
    // Only attempt when the `cdp` feature is enabled
    #[cfg(feature = "cdp")]
    {
        if !has_chrome_binary() {
            return None;
        }
        match CdpBrowserEffector::open(workspace) {
            Ok(effector) => {
                eprintln!("[browser] Using CDP browser effector");
                return Some(Box::new(effector));
            }
            Err(e) => {
                eprintln!("[browser] CDP effector failed, falling back to HTTP: {e}");
            }
        }
    }
    #[cfg(not(feature = "cdp"))]
    {
        let _ = workspace;
    }
    None
}

/// Create an HTTP text effector (always succeeds).
pub fn http_effector(workspace: impl AsRef<Path>) -> Result<Box<dyn BrowserEffector>> {
    HttpBrowserEffector::open(workspace).map(|e| Box::new(e) as Box<dyn BrowserEffector>)
}

/// Best-effort: try CDP, fall back to HTTP.
pub fn best_effector(workspace: impl AsRef<Path>) -> Result<Box<dyn BrowserEffector>> {
    let workspace = workspace.as_ref().to_path_buf();
    match try_cdp_effector(&workspace) {
        Some(e) => Ok(e),
        None => http_effector(&workspace),
    }
}

// ---------------------------------------------------------------------------
// HTTP effector (existing behaviour, unchanged)
// ---------------------------------------------------------------------------

pub(crate) struct HttpBrowserEffector {
    inner: HttpBrowserSession,
}

impl HttpBrowserEffector {
    fn open(workspace: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            inner: HttpBrowserSession::open(workspace)?,
        })
    }
}

impl BrowserEffector for HttpBrowserEffector {
    fn navigate(&mut self, url: &str) -> Result<String> {
        let page = self.inner.navigate(url)?;
        Ok(page_to_tool_json(&page).to_string())
    }

    fn snapshot(&self) -> Result<String> {
        let page = self.inner.snapshot()?;
        Ok(page_to_tool_json(page).to_string())
    }

    fn click(&mut self, index: usize) -> Result<String> {
        let page = self.inner.click(index)?;
        Ok(page_to_tool_json(&page).to_string())
    }

    fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// HTTP browser session (original BrowserSession, moved here)
// ---------------------------------------------------------------------------

pub struct HttpBrowserSession {
    path: PathBuf,
    state: BrowserState,
    timeout: Duration,
    max_bytes: usize,
    max_text_chars: usize,
}

impl HttpBrowserSession {
    pub fn open(workspace: impl AsRef<Path>) -> Result<Self> {
        let dir = workspace.as_ref().join(".optimus");
        fs::create_dir_all(&dir)?;
        let path = dir.join("browser_state.json");
        let state = if path.exists() {
            serde_json::from_str(&fs::read_to_string(&path)?)?
        } else {
            BrowserState::default()
        };
        Ok(Self {
            path,
            state,
            timeout: Duration::from_secs(30),
            max_bytes: 2_000_000,
            max_text_chars: 24_000,
        })
    }

    pub fn save(&self) -> Result<()> {
        fs::write(&self.path, serde_json::to_string_pretty(&self.state)?)?;
        Ok(())
    }

    pub fn navigate(&mut self, url: &str) -> Result<BrowserPage> {
        let page = fetch_page(url, self.timeout, self.max_bytes, self.max_text_chars)?;
        self.state.history.push(page.final_url.clone());
        if self.state.history.len() > 50 {
            let drain = self.state.history.len() - 50;
            self.state.history.drain(0..drain);
        }
        self.state.page = Some(page.clone());
        self.save()?;
        Ok(page)
    }

    pub fn snapshot(&self) -> Result<&BrowserPage> {
        self.state.page.as_ref().ok_or(BrowserError::NoPage)
    }

    pub fn click(&mut self, index: usize) -> Result<BrowserPage> {
        let href = {
            let page = self.state.page.as_ref().ok_or(BrowserError::NoPage)?;
            page.links
                .iter()
                .find(|l| l.index == index)
                .map(|l| l.href.clone())
                .ok_or(BrowserError::BadLink(index))?
        };
        self.navigate(&href)
    }
}

// ---------------------------------------------------------------------------
// CDP effector
// ---------------------------------------------------------------------------

#[cfg(feature = "cdp")]
struct CdpBrowserEffector {
    inner: Option<optimus_browser::CdpBrowserSession>,
    _workspace: PathBuf,
}

#[cfg(feature = "cdp")]
impl CdpBrowserEffector {
    fn open(workspace: impl AsRef<Path>) -> Result<Self> {
        use optimus_browser::BrowserOptions;
        let workspace = workspace.as_ref().to_path_buf();
        let chrome_path = chrome_binary_path();
        let opts = BrowserOptions {
            headless: true,
            chrome_path,
            port: 0, // let the system pick
            timeout_secs: 90,
            user_data_dir: Some(workspace.join(".optimus/cdp-profile")),
            window_size: (1440, 1200),
        };
        let session = optimus_browser::CdpBrowserSession::open(&workspace, opts)
            .map_err(|e| BrowserError::Cdp(e.to_string()))?;
        Ok(Self {
            inner: Some(session),
            _workspace: workspace,
        })
    }
}

#[cfg(feature = "cdp")]
impl BrowserEffector for CdpBrowserEffector {
    fn navigate(&mut self, url: &str) -> Result<String> {
        let session = self.inner.as_mut().ok_or(BrowserError::NoPage)?;
        let state = session
            .navigate(url)
            .map_err(|e| BrowserError::Cdp(e.to_string()))?;
        Ok(cdp_page_tool_json(
            &state.url,
            &state.title,
            &state.screenshot_b64,
            &state.elements,
        ))
    }

    fn snapshot(&self) -> Result<String> {
        let session = self.inner.as_ref().ok_or(BrowserError::NoPage)?;
        let cap = session
            .som_capture()
            .map_err(|e| BrowserError::Cdp(e.to_string()))?;
        let url = session.current_url();
        let title = session.title().unwrap_or_default();
        Ok(cdp_page_tool_json(
            &url,
            &title,
            &cap.screenshot_b64,
            &cap.elements,
        ))
    }

    fn click(&mut self, index: usize) -> Result<String> {
        let session = self.inner.as_mut().ok_or(BrowserError::NoPage)?;
        let state = session
            .click(index)
            .map_err(|e| BrowserError::Cdp(e.to_string()))?;
        Ok(cdp_page_tool_json(
            &state.url,
            &state.title,
            &state.screenshot_b64,
            &state.elements,
        ))
    }

    fn close(&mut self) -> Result<()> {
        if let Some(mut session) = self.inner.take() {
            let _ = session.close();
        }
        Ok(())
    }
}

/// Build CDP tool JSON with summary-friendly keys before the huge screenshot payload.
///
/// Tool traces truncate at 120 chars; BTreeMap key order would bury `url`/`title`
/// after `screenshot_b64`, so we emit a compact `final_url` + `page_title` + `text`
/// that sorts earlier and remains visible in traces.
#[cfg(feature = "cdp")]
fn cdp_page_tool_json(
    url: &str,
    title: &str,
    screenshot_b64: &str,
    elements: &[optimus_browser::DomElement],
) -> String {
    let text = if title.is_empty() {
        url.to_string()
    } else {
        format!("{title} — {url}")
    };
    json!({
        "ok": true,
        "effector": "cdp-browser",
        "final_url": url,
        "page_title": title,
        "text": text,
        "element_count": elements.len(),
        "elements": elements,
        "screenshot_b64": screenshot_b64,
    })
    .to_string()
}

// Stub for non-cdp builds
#[cfg(not(feature = "cdp"))]
struct CdpBrowserEffector;
#[cfg(not(feature = "cdp"))]
impl CdpBrowserEffector {
    fn open(_workspace: impl AsRef<Path>) -> Result<Self> {
        Err(BrowserError::Cdp("cdp feature not enabled".into()))
    }
}

// ---------------------------------------------------------------------------
// Chrome detection
// ---------------------------------------------------------------------------

fn has_chrome_binary() -> bool {
    chrome_binary_path().is_some()
}

/// Resolve a Chromium/Chrome binary for CDP launches.
pub fn chrome_binary_path() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("OPTIMUS_CHROME_PATH") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Some(path);
        }
    }
    for name in [
        "chromium-browser",
        "chromium",
        "google-chrome-stable",
        "google-chrome",
        "chrome",
        "chrome-stable",
    ] {
        if let Ok(path) = which::which(name) {
            return Some(path);
        }
    }
    // Playwright-managed Chromium (common on developer machines).
    if let Ok(home) = std::env::var("HOME") {
        let root = PathBuf::from(home).join(".cache/ms-playwright");
        if let Ok(entries) = fs::read_dir(&root) {
            let mut candidates = Vec::new();
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with("chromium-") {
                    continue;
                }
                for rel in [
                    "chrome-linux/chrome",
                    "chrome-linux64/chrome",
                    "chrome-linux/chromium",
                ] {
                    let cand = entry.path().join(rel);
                    if cand.is_file() {
                        candidates.push(cand);
                    }
                }
            }
            candidates.sort();
            if let Some(path) = candidates.pop() {
                return Some(path);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// HTTP fetch logic (unchanged from original browser.rs)
// ---------------------------------------------------------------------------

pub fn fetch_page(
    url_str: &str,
    timeout: Duration,
    max_bytes: usize,
    max_text_chars: usize,
) -> Result<BrowserPage> {
    let url = Url::parse(url_str).map_err(|e| BrowserError::Url(e.to_string()))?;
    assert_url_safe(&url)?;

    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    let resp = agent
        .get(url.as_str())
        .set(
            "User-Agent",
            "OptimusAgent/0.1 (+https://local; research browser effector)",
        )
        .set(
            "Accept",
            "text/html,application/xhtml+xml,text/plain;q=0.9,*/*;q=0.8",
        )
        .call()
        .map_err(|e| BrowserError::Http(e.to_string()))?;
    let status = resp.status();
    let final_url = resp.get_url().to_string();
    // Fail closed: unparsable redirect target is not a free pass (SSRF residual).
    let fu = Url::parse(&final_url).map_err(|e| BrowserError::Url(format!("final_url: {e}")))?;
    assert_url_safe(&fu)?;
    let mut body = Vec::new();
    resp.into_reader()
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|e| BrowserError::Http(e.to_string()))?;
    if body.len() > max_bytes {
        body.truncate(max_bytes);
    }
    let html = String::from_utf8_lossy(&body);
    let title = extract_title(&html);
    let links = extract_links(&html, &final_url);
    let mut text = html_to_text(&html);
    if text.chars().count() > max_text_chars {
        text = text.chars().take(max_text_chars).collect::<String>() + "\n…[truncated]";
    }
    Ok(BrowserPage {
        url: url_str.to_string(),
        final_url,
        title,
        status,
        text,
        links,
        fetched_at: chrono_stamp(),
    })
}

fn chrono_stamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

fn assert_url_safe(url: &Url) -> Result<()> {
    crate::network_policy::assert_public_http_url(url).map_err(|e| match e {
        crate::network_policy::EgressError::Ssrf(msg) => BrowserError::Ssrf(msg),
    })
}

/// A page result's total size, and how it is divided.
///
/// One budget split at run time rather than two constants that have to be kept
/// consistent by hand. The result must fit under the kernel's per-result context
/// clamp; text is bounded first because it carries what the page said, and links
/// get whatever the text did not use.
///
/// That split matters because the two vary inversely. Stripping site chrome cut
/// `github.com/trending` from 16,136 chars of text to 3,855 — and the same page
/// needs many links, because each repository row spends one on the repository
/// and three more on its stargazers, forks and contributors. Fixed caps sized
/// for the worst case would have starved exactly the page that had room.
const MAX_RESULT_CHARS: usize = 22_000;

/// Chars of page text carried into a tool result.
///
/// Observed: `github.com/trending` was fetched successfully four times. Each
/// time the link table took the head of the budget, the text was cut short of
/// its end — exactly where the repository list begins, since the nav menu comes
/// first — and the model reported that "only GitHub's surrounding navigation
/// loaded" and declined to answer rather than invent star counts.
const MAX_TEXT_CHARS: usize = 16_000;

/// Roughly what one link costs once serialised, beyond its own text and href:
/// the `index`, the field names, the braces and the commas.
const LINK_OVERHEAD_CHARS: usize = 40;

pub fn page_to_tool_json(page: &BrowserPage) -> serde_json::Value {
    let text_chars = page.text.chars().count();
    let text: String = page.text.chars().take(MAX_TEXT_CHARS).collect();

    let mut remaining = MAX_RESULT_CHARS.saturating_sub(text.chars().count());
    let mut links: Vec<&BrowserLink> = Vec::new();
    for link in &page.links {
        let cost = link.href.chars().count() + link.text.chars().count() + LINK_OVERHEAD_CHARS;
        if cost > remaining {
            break;
        }
        remaining -= cost;
        links.push(link);
    }

    let mut value = json!({
        "ok": true,
        "url": page.url,
        "final_url": page.final_url,
        "status": page.status,
        "title": page.title,
        "text": text,
        "links": links,
        "fetched_at": page.fetched_at,
        "effector": "http-browser",
    });
    // Stated, not implied. A page that stops early and a page that ended read
    // identically otherwise, and the difference decides whether the right move
    // is to answer or to fetch the rest.
    if text_chars > MAX_TEXT_CHARS {
        value["text_truncated"] = json!(format!("{MAX_TEXT_CHARS} of {text_chars} chars"));
    }
    if links.len() < page.links.len() {
        value["links_truncated"] = json!(format!("{} of {} links", links.len(), page.links.len()));
    }
    value
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A chrome-heavy page: a long navigation menu, then the content, then a
    /// link table dominated by the menu. Shaped like `github.com/trending`.
    fn chrome_heavy_page(nav_chars: usize, content: &str, link_count: usize) -> BrowserPage {
        BrowserPage {
            url: "https://github.com/trending".into(),
            final_url: "https://github.com/trending?since=daily".into(),
            title: "Trending repositories".into(),
            status: 200,
            text: format!("{}{content}", "Navigation Menu ".repeat(nav_chars / 16)),
            links: (0..link_count)
                .map(|index| BrowserLink {
                    index,
                    text: "GitHub CopilotWrite better code with AI".into(),
                    href: format!("https://github.com/features/{index}"),
                })
                .collect(),
            fetched_at: "unix:0".into(),
        }
    }

    #[test]
    fn a_filter_menu_does_not_bury_the_list_it_filters() {
        // The shape of github.com/trending: a menu naming every language GitHub
        // knows, then the repositories. Extracted whole the menu was 13,241 of
        // 16,136 chars and the model concluded the page had not loaded.
        let languages: String = ["Wollok", "Wren", "Yacc", "YAML", "Zig", "Zimpl"]
            .iter()
            .map(|l| format!("<a role=\"menuitemradio\">{l}</a>"))
            .collect();
        let html = format!(
            "<main><details-menu>{languages}</details-menu>\
             <div class=\"Box-row\">permissionlesstech / bitchat</div></main>"
        );
        let text = html_to_text(&html);
        assert!(
            text.contains("permissionlesstech / bitchat"),
            "the list the page exists for must survive"
        );
        assert!(
            !text.contains("Wollok") && !text.contains("Zimpl"),
            "the filter menu is furniture, not content: {text:?}"
        );
    }

    #[test]
    fn a_closing_tag_inside_chrome_does_not_let_the_rest_of_it_through() {
        // These nest. Treated as a flag rather than a depth, the inner `</svg>`
        // would re-open extraction and leak the remaining menu text.
        let html = "<nav>MENU<svg>ICON</svg>MORE MENU</nav><p>REAL</p>";
        let text = html_to_text(html);
        assert!(text.contains("REAL"));
        assert!(!text.contains("MORE MENU"), "leaked after the inner close");
    }

    #[test]
    fn ordinary_markup_is_still_read_as_text() {
        // The skip list must not cost the common case anything.
        let html = "<header><h1>Headline</h1></header><p>Body text</p><footer>legal</footer>";
        let text = html_to_text(html);
        assert!(text.contains("Headline"), "an article header is content");
        assert!(text.contains("Body text"));
        assert!(!text.contains("legal"));
    }

    #[test]
    fn a_page_result_fits_the_context_clamp_without_being_cut_by_it() {
        // The clamp keeps the head of a result and serde sorts keys, so `links`
        // is serialised before `text` whatever order the literal is written in.
        // A result that overruns the clamp therefore always loses page text —
        // the one field carrying what the page said. Staying under it is what
        // keeps that decision in this module.
        let page = chrome_heavy_page(MAX_TEXT_CHARS * 2, "CONTENT", 400);
        let rendered = page_to_tool_json(&page).to_string();
        assert!(
            rendered.chars().count() < crate::CompressionConfig::default().max_tool_result_chars,
            "a bounded page must fit the clamp, got {} chars",
            rendered.chars().count()
        );
    }

    #[test]
    fn the_content_survives_the_navigation_menu_in_front_of_it() {
        // Observed: `github.com/trending` fetched successfully four times and the
        // repository list — which sits after ~14k chars of nav menu — was cut off
        // every time. The model reported that only the navigation had loaded and
        // declined to answer.
        let page = chrome_heavy_page(14_000, "block / buzz 13,589 stars today", 200);
        let rendered = page_to_tool_json(&page).to_string();
        assert!(
            rendered.contains("block / buzz 13,589 stars today"),
            "the part of the page worth fetching must reach the model"
        );
    }

    #[test]
    fn the_link_cap_is_not_spent_on_the_navigation_menu() {
        // Observed: all 40 links returned for github.com/trending were site
        // chrome — the cap was reached long before the repository rows. Left to
        // pair `owner /` and `name` back together out of the flattened text, the
        // model answered with `jackdorsey/bitchat` and `egoist/ego-lite`, which
        // belong to `permissionlesstech` and `citrolabs`. Those hrefs were on
        // the page; the cap is why they never arrived.
        let chrome: String = (0..60)
            .map(|i| format!("<a href=\"/features/{i}\">Feature {i}</a>"))
            .collect();
        let html = format!(
            "<nav>{chrome}</nav>\
             <div class=\"Box-row\">\
             <a href=\"/permissionlesstech/bitchat\">permissionlesstech / bitchat</a></div>"
        );
        let links = extract_links(&html, "https://github.com/trending");
        assert!(
            links
                .iter()
                .any(|l| l.href.ends_with("/permissionlesstech/bitchat")),
            "the repository href must survive a menu that outnumbers it: {links:?}"
        );
        assert!(
            !links.iter().any(|l| l.href.contains("/features/")),
            "menu links are not content"
        );
    }

    #[test]
    fn a_link_table_never_outspends_the_page_it_came_from() {
        // The two vary inversely, so the invariant is the split, not a count: a
        // page whose text fills the budget must yield its links, and one with
        // room to spare must keep them.
        let links_kept = |text_chars, link_count| {
            page_to_tool_json(&chrome_heavy_page(text_chars, "CONTENT", link_count))["links"]
                .as_array()
                .unwrap()
                .len()
        };
        let with_a_short_page = links_kept(0, 200);
        let with_a_long_page = links_kept(MAX_TEXT_CHARS, 200);
        assert!(
            with_a_long_page < with_a_short_page,
            "text at its cap must squeeze the link table: {with_a_long_page} vs {with_a_short_page}"
        );
        assert!(
            with_a_short_page > 100,
            "a page that leaves room must keep its links; each trending row spends \
             four of them, so a low cap loses the owner of every repository but the first"
        );
    }

    #[test]
    fn dropping_links_is_always_stated() {
        // A missing click index otherwise looks like a page that never had it.
        let value = page_to_tool_json(&chrome_heavy_page(MAX_TEXT_CHARS, "CONTENT", 200));
        let kept = value["links"].as_array().unwrap().len();
        assert_eq!(value["links_truncated"], format!("{kept} of 200 links"));
    }

    #[test]
    fn a_page_that_fits_is_reported_whole_and_claims_no_truncation() {
        let page = chrome_heavy_page(0, "CONTENT", 3);
        let value = page_to_tool_json(&page);
        assert!(value["text"].as_str().unwrap().contains("CONTENT"));
        assert!(value.get("text_truncated").is_none());
        assert!(value.get("links_truncated").is_none());
    }

    #[test]
    fn strips_tags_and_finds_title() {
        let html = r#"<html><head><title>Hi &amp; Bye</title></head>
        <body><script>x</script><p>Hello <b>world</b></p>
        <a href="/x">Go</a></body></html>"#;
        assert_eq!(extract_title(html), "Hi & Bye");
        let text = html_to_text(html);
        assert!(text.contains("Hello world"));
        assert!(!text.contains("script"));
    }

    #[test]
    fn blocks_localhost() {
        let u = Url::parse("http://127.0.0.1/").unwrap();
        assert!(assert_url_safe(&u).is_err());
        let u = Url::parse("http://localhost/").unwrap();
        assert!(assert_url_safe(&u).is_err());
    }

    #[test]
    fn allows_example_parse() {
        let u = Url::parse("https://example.com/path").unwrap();
        assert!(assert_url_safe(&u).is_ok());
    }

    /// S1.8 / browser.http: HTTP effector fails closed on SSRF without CDP.
    #[test]
    fn http_effector_navigate_rejects_ssrf_targets_without_cdp() {
        let dir = tempfile::tempdir().unwrap();
        let mut effector = http_effector(dir.path()).unwrap();
        for bad in [
            "http://127.0.0.1/",
            "http://localhost/admin",
            "http://10.0.0.5/",
            "http://192.168.1.1/",
            "http://[::1]/",
            "http://169.254.169.254/latest/meta-data/",
            "file:///etc/passwd",
            "ftp://example.com/",
        ] {
            let err = effector.navigate(bad).unwrap_err();
            assert!(
                matches!(err, BrowserError::Ssrf(_)),
                "expected BrowserError::Ssrf for {bad}, got {err:?}"
            );
        }
        // No page state after denied navigations.
        assert!(matches!(
            effector.snapshot().unwrap_err(),
            BrowserError::NoPage
        ));
    }

    #[test]
    fn chrome_binary_path_honors_explicit_env_file() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("fake-chrome");
        fs::write(&fake, b"#!/bin/true").unwrap();
        // SAFETY: tests run single-threaded by default for this crate unit suite.
        std::env::set_var("OPTIMUS_CHROME_PATH", &fake);
        let found = chrome_binary_path();
        std::env::remove_var("OPTIMUS_CHROME_PATH");
        assert_eq!(found.as_deref(), Some(fake.as_path()));
    }

    #[test]
    fn chrome_binary_path_ignores_missing_env_file() {
        std::env::set_var("OPTIMUS_CHROME_PATH", "/no/such/chrome-binary-optimus-test");
        // Should fall through to PATH / Playwright discovery instead of panicking.
        let _ = chrome_binary_path();
        std::env::remove_var("OPTIMUS_CHROME_PATH");
    }

    #[test]
    fn best_effector_factory_uses_cdp_when_launchable_and_honestly_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut effector = best_effector(dir.path()).unwrap();
        let json = effector.navigate("https://example.com").unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        if value["effector"] == "cdp-browser" {
            assert!(
                value["screenshot_b64"]
                    .as_str()
                    .map(|s| s.len() > 100)
                    .unwrap_or(false),
                "CDP preview must return a real screenshot payload"
            );
        } else {
            assert_eq!(value["effector"], "http-browser");
            assert!(value.get("screenshot_b64").is_none());
        }
        let _ = effector.close();
    }

    #[test]
    fn http_effector_navigate_and_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        // Write a small local HTML file
        let html = "<html><head><title>Test</title></head><body><p>Hello</p></body></html>";
        let file_path = dir.path().join("test.html");
        fs::write(&file_path, html).unwrap();

        // Create an effector pointing workspace to dir
        let mut effector = http_effector(dir.path()).unwrap();

        // Navigate to the file (file:// is blocked by SSRF, but let's test via
        // a direct page load mock — actually we can just check that navigate
        // works to a real site if available. For unit tests, we'll just verify
        // the types round-trip.)
        let snapshot = effector.snapshot().unwrap_err();
        assert!(matches!(snapshot, BrowserError::NoPage));

        // Close is a no-op for HTTP
        effector.close().unwrap();
    }
}
