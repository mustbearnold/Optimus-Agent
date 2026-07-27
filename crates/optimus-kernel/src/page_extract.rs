//! Turning fetched HTML into the text and links a model is given.
//!
//! Split out of `browser.rs` under the module-size law. What separates this
//! from the effectors next door is that nothing here fetches anything: it is
//! the pure part, and the part with the judgement calls — which elements are
//! furniture, what a link is worth, where a page's content actually starts.

use super::BrowserLink;
use url::Url;

/// Links collected from a page before extraction stops.
///
/// Higher than what a result carries, so the budget in `browser` chooses from
/// a real sample rather than from whatever happened to appear first.
const MAX_EXTRACTED_LINKS: usize = 200;

pub(crate) fn extract_title(html: &str) -> String {
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

/// Byte ranges covered by [`CHROME_ELEMENTS`], for callers that walk the raw
/// HTML rather than its extracted text.
///
/// `to_ascii_lowercase` preserves byte length, so offsets found in the lowered
/// copy address the original exactly.
pub(crate) fn chrome_spans(lower: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut i = 0;
    while i < lower.len() {
        if lower.as_bytes()[i] != b'<' {
            i += 1;
            continue;
        }
        let mut end = (i + 18).min(lower.len());
        while end > i && !lower.is_char_boundary(end) {
            end -= 1;
        }
        if let Some((name, closing)) = element_name(&lower[i..end]) {
            if CHROME_ELEMENTS.contains(&name) {
                if closing {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        spans.push((start, i));
                    }
                } else {
                    if depth == 0 {
                        start = i;
                    }
                    depth += 1;
                }
            }
        }
        i += 1;
    }
    if depth > 0 {
        // Unclosed chrome runs to the end rather than releasing the rest of the
        // document into the result.
        spans.push((start, lower.len()));
    }
    spans
}

pub(crate) fn extract_links(html: &str, base: &str) -> Vec<BrowserLink> {
    let base = Url::parse(base).ok();
    let mut out = Vec::new();
    let bytes = html.as_bytes();
    let lower = html.to_ascii_lowercase();
    // Links are collected from the top and capped, so without this the cap is
    // spent entirely on the navigation menu. Observed: every one of the 40 links
    // returned for github.com/trending was site chrome, the repository URLs were
    // never reached, and the model — left to pair owners and names out of the
    // flattened text — reported `jackdorsey/bitchat` and `egoist/ego-lite` for
    // repositories that actually live under `permissionlesstech` and `citrolabs`.
    // The link table is the only place the pairing is unambiguous.
    let chrome = chrome_spans(&lower);
    let mut i = 0;
    while let Some(rel) = lower[i..].find("<a ") {
        let start = i + rel;
        if chrome
            .iter()
            .any(|(from, to)| start >= *from && start < *to)
        {
            i = start + 3;
            continue;
        }
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
            if out.len() >= MAX_EXTRACTED_LINKS {
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

pub(crate) fn attr_value(tag: &str, name: &str) -> Option<String> {
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

/// Elements whose text is furniture rather than anything the page is about.
///
/// Observed: `github.com/trending` renders its language filter as a menu holding
/// every language GitHub knows about. Extracted verbatim that menu was 13,241 of
/// the page's 16,136 characters, and the repository list the page exists for
/// began only after all of it. A model given the result reported that "only
/// GitHub's surrounding navigation loaded" and declined to answer — a correct
/// reading of what it was sent.
///
/// `header` is deliberately absent: article headers carry the headline, and
/// dropping it would lose content to save chrome.
pub(crate) const CHROME_ELEMENTS: &[&str] = &[
    "script",
    "style",
    "noscript",
    "svg",
    "nav",
    "footer",
    "select",
    "template",
    // Not standard HTML. GitHub's own element, and the one that actually holds
    // the language list above.
    "details-menu",
];

/// The element name in `<name …>` or `</name …>`, and whether it closes one.
///
/// Reads from a fixed lookahead window, so a name running past the end of the
/// window is taken as far as the window goes; the window is sized for the
/// longest name in [`CHROME_ELEMENTS`] and every name is compared exactly, so a
/// clipped name simply fails to match rather than matching the wrong element.
pub(crate) fn element_name(tail: &str) -> Option<(&str, bool)> {
    let rest = tail.strip_prefix('<')?;
    let (rest, closing) = match rest.strip_prefix('/') {
        Some(rest) => (rest, true),
        None => (rest, false),
    };
    let end = rest
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .unwrap_or(rest.len());
    let name = &rest[..end];
    (!name.is_empty()).then_some((name, closing))
}

pub(crate) fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    // Depth, not a flag: these elements nest, and one closing tag must not
    // re-open the text of the element still wrapping it.
    let mut chrome_depth = 0usize;
    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '<' {
            let tail: String = chars[i..]
                .iter()
                .take(18)
                .collect::<String>()
                .to_ascii_lowercase();
            if let Some((name, closing)) = element_name(&tail) {
                if CHROME_ELEMENTS.contains(&name) {
                    if closing {
                        chrome_depth = chrome_depth.saturating_sub(1);
                    } else {
                        chrome_depth += 1;
                    }
                }
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
        if !in_tag && chrome_depth == 0 {
            out.push(c);
        }
        i += 1;
    }
    let decoded = decode_entities(&out);
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

pub(crate) fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}
