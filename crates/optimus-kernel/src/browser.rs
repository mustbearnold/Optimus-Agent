//! HTTP browser effector — durable page state in workspace (exceeds pack stubs).
//!
//! Not a full CDP browser: navigate + text snapshot + link click by index.
//! Network is SSRF-hardened (no link-local/metadata hosts).

use std::fs;
use std::io::Read;
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;
use url::Url;

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
}

pub type Result<T> = std::result::Result<T, BrowserError>;

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

pub struct BrowserSession {
    path: PathBuf,
    state: BrowserState,
    timeout: Duration,
    max_bytes: usize,
    max_text_chars: usize,
}

impl BrowserSession {
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

    pub fn state(&self) -> &BrowserState {
        &self.state
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
    if let Ok(fu) = Url::parse(&final_url) {
        assert_url_safe(&fu)?;
    }
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
    // Keep dependency-free ISO-ish UTC-ish local stamp
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

fn assert_url_safe(url: &Url) -> Result<()> {
    match url.scheme() {
        "http" | "https" => {}
        other => return Err(BrowserError::Ssrf(format!("scheme {other}"))),
    }
    let host = url
        .host_str()
        .ok_or_else(|| BrowserError::Ssrf("missing host".into()))?
        .to_ascii_lowercase();
    if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host == "metadata.google.internal"
    {
        return Err(BrowserError::Ssrf(format!("host {host}")));
    }
    // Block obvious IPs
    if let Ok(addr) = host.parse::<std::net::IpAddr>() {
        if ip_blocked(addr) {
            return Err(BrowserError::Ssrf(format!("ip {addr}")));
        }
    } else {
        // Resolve and check (best-effort)
        let port = url.port_or_known_default().unwrap_or(80);
        let key = format!("{host}:{port}");
        if let Ok(iter) = key.to_socket_addrs() {
            for sa in iter.take(8) {
                if ip_blocked(sa.ip()) {
                    return Err(BrowserError::Ssrf(format!("resolved {}", sa.ip())));
                }
            }
        }
    }
    Ok(())
}

fn ip_blocked(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.octets()[0] == 169 && v4.octets()[1] == 254
                || v4.octets()[0] == 0
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback() || v6.is_unique_local() || v6.is_unicast_link_local()
        }
    }
}

fn extract_title(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    if let Some(s) = lower.find("<title") {
        if let Some(gt) = html[s..].find('>') {
            let start = s + gt + 1;
            if let Some(end_rel) = lower[start..].find("</title>") {
                return decode_entities(html[start..start + end_rel].trim());
            }
        }
    }
    String::new()
}

fn extract_links(html: &str, base: &str) -> Vec<BrowserLink> {
    let base = Url::parse(base).ok();
    let mut out = Vec::new();
    let bytes = html.as_bytes();
    let lower = html.to_ascii_lowercase();
    let mut i = 0;
    while let Some(rel) = lower[i..].find("<a ") {
        let start = i + rel;
        let rest = &html[start..];
        let rest_l = &lower[start..];
        let end_tag = rest_l.find('>').unwrap_or(0);
        let tag = &rest[..=end_tag.min(rest.len().saturating_sub(1))];
        let href = attr_value(tag, "href");
        let after = start + end_tag + 1;
        let text_end = lower[after..]
            .find("</a>")
            .map(|p| after + p)
            .unwrap_or(after);
        let text = decode_entities(html_to_text(&html[after..text_end]).trim());
        if let Some(h) = href {
            if h.starts_with('#') || h.starts_with("javascript:") || h.starts_with("mailto:") {
                i = text_end + 4;
                continue;
            }
            let abs = if let Some(b) = &base {
                b.join(&h)
                    .map(|u| u.to_string())
                    .unwrap_or_else(|_| h.clone())
            } else {
                h
            };
            let idx = out.len();
            out.push(BrowserLink {
                index: idx,
                text: text.chars().take(120).collect(),
                href: abs,
            });
            if out.len() >= 40 {
                break;
            }
        }
        i = text_end + 4;
        if i >= bytes.len() {
            break;
        }
    }
    out
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let key = format!("{name}=");
    let pos = lower.find(&key)?;
    let after = &tag[pos + key.len()..];
    let after = after.trim_start();
    let quote = after.chars().next()?;
    if quote == '"' || quote == '\'' {
        let rest = &after[1..];
        let end = rest.find(quote)?;
        Some(rest[..end].to_string())
    } else {
        let end = after
            .find(|c: char| c.is_whitespace() || c == '>')
            .unwrap_or(after.len());
        Some(after[..end].to_string())
    }
}

fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut in_script = false;
    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '<' {
            // detect script/style
            let tail: String = chars[i..]
                .iter()
                .take(10)
                .collect::<String>()
                .to_ascii_lowercase();
            if tail.starts_with("<script") || tail.starts_with("<style") {
                in_script = true;
            }
            if tail.starts_with("</script") || tail.starts_with("</style") {
                in_script = false;
            }
            if tail.starts_with("<br")
                || tail.starts_with("<p")
                || tail.starts_with("<div")
                || tail.starts_with("<li")
                || tail.starts_with("<tr")
                || tail.starts_with("<h")
            {
                out.push('\n');
            }
            in_tag = true;
            i += 1;
            continue;
        }
        if c == '>' {
            in_tag = false;
            i += 1;
            continue;
        }
        if !in_tag && !in_script {
            out.push(c);
        }
        i += 1;
    }
    let decoded = decode_entities(&out);
    // collapse whitespace
    let mut cleaned = String::new();
    let mut prev_space = false;
    for line in decoded.lines() {
        let t = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if t.is_empty() {
            if !prev_space {
                cleaned.push('\n');
                prev_space = true;
            }
        } else {
            cleaned.push_str(&t);
            cleaned.push('\n');
            prev_space = false;
        }
    }
    cleaned.trim().to_string()
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

pub fn page_to_tool_json(page: &BrowserPage) -> serde_json::Value {
    json!({
        "ok": true,
        "url": page.url,
        "final_url": page.final_url,
        "status": page.status,
        "title": page.title,
        "text": page.text,
        "links": page.links,
        "fetched_at": page.fetched_at,
        "effector": "http-browser",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
