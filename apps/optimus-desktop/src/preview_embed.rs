//! Live embedded Preview Browser (Codex-style child WebView).
//!
//! The main Optimus UI remains one privileged WebView. The preview browser is a
//! second child WebView positioned over the right-panel viewport and controlled
//! from the UI chrome via IPC.
//!
//! On Linux/GTK the preview WebView MUST sit above the main UI WebView after
//! every bounds update; otherwise the hole stays black and clicks never reach
//! the live page. Annotation pins use both Wry IPC and a cancelled-navigation
//! fallback (`https://optimus.invalid/__annot?...`) because secondary WebKit
//! webviews sometimes fail to deliver `window.ipc` messages.

use serde_json::{json, Value};
use wry::{
    dpi::{LogicalPosition, LogicalSize},
    Rect, WebView,
};

/// Special origin used only as an in-page annotation callback. Navigation is
/// always denied; the payload is parsed from the query string.
pub const ANNOT_CALLBACK_PREFIX: &str = "https://optimus.invalid/__annot?";

#[derive(Debug, Clone, Copy, Default)]
pub struct EmbedBounds {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub visible: bool,
}

impl EmbedBounds {
    pub fn from_params(params: &Value) -> Self {
        Self {
            x: num(params, "x"),
            y: num(params, "y"),
            w: num(params, "w").max(0.0),
            h: num(params, "h").max(0.0),
            visible: params
                .get("visible")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }
    }

    pub fn is_usable(&self) -> bool {
        self.visible && self.w >= 32.0 && self.h >= 32.0
    }

    fn native_rect(&self) -> Option<NativeRect> {
        self.is_usable().then(|| NativeRect {
            x: self.x.round().max(0.0) as i32,
            y: self.y.round().max(0.0) as i32,
            w: self.w.round().max(32.0) as i32,
            h: self.h.round().max(32.0) as i32,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmbedUpdate {
    Noop,
    Hide,
    ShowAndRestack,
    MoveOnly,
}

fn classify_embed_update(previous: EmbedBounds, next: EmbedBounds) -> EmbedUpdate {
    match (previous.native_rect(), next.native_rect()) {
        (None, None) => EmbedUpdate::Noop,
        (Some(_), None) => EmbedUpdate::Hide,
        (None, Some(_)) => EmbedUpdate::ShowAndRestack,
        (Some(previous), Some(next)) if previous == next => EmbedUpdate::Noop,
        (Some(_), Some(_)) => EmbedUpdate::MoveOnly,
    }
}

fn num(params: &Value, key: &str) -> f64 {
    params
        .get(key)
        .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)))
        .unwrap_or(0.0)
}

pub struct PreviewEmbed {
    webview: WebView,
    bounds: EmbedBounds,
    last_url: String,
    annotate: bool,
}

impl PreviewEmbed {
    pub fn new(webview: WebView) -> Self {
        let _ = webview.set_visible(false);
        // Matches the builder warm-start URL in main.rs.
        let initial = "https://www.google.com/".to_string();
        Self {
            webview,
            bounds: EmbedBounds::default(),
            last_url: initial,
            annotate: false,
        }
    }

    pub fn apply_bounds(&mut self, bounds: EmbedBounds) -> Result<Value, String> {
        let update = classify_embed_update(self.bounds, bounds);
        let rect = bounds.native_rect();

        match update {
            EmbedUpdate::Noop => {}
            EmbedUpdate::Hide => self
                .webview
                .set_visible(false)
                .map_err(|error| error.to_string())?,
            EmbedUpdate::ShowAndRestack => {
                let rect = rect.expect("usable embed bounds");
                apply_preview_geometry(&self.webview, rect)?;
                self.webview
                    .set_visible(true)
                    .map_err(|error| error.to_string())?;
                restack_preview_webview(&self.webview, rect);
            }
            EmbedUpdate::MoveOnly => {
                apply_preview_geometry(&self.webview, rect.expect("usable embed bounds"))?;
            }
        }

        self.bounds = bounds;
        Ok(match rect {
            Some(rect) => json!({
                "ok": true,
                "visible": true,
                "x": rect.x,
                "y": rect.y,
                "w": rect.w,
                "h": rect.h,
                "effector": "embedded-webview",
            }),
            None => json!({
                "ok": true,
                "visible": false,
                "effector": "embedded-webview",
            }),
        })
    }

    pub fn resize_main_fill(main: &WebView, width: u32, height: u32) {
        let _ = main.set_bounds(Rect {
            position: LogicalPosition::new(0, 0).into(),
            size: LogicalSize::new(width, height).into(),
        });
        lower_main_webview(main, width, height);
    }

    pub fn reassert_bounds(&mut self) {
        if self.bounds.native_rect().is_some() {
            raise_preview_window(&self.webview);
        }
    }

    pub fn navigate(&mut self, url: &str) -> Result<Value, String> {
        let url = normalize_url(url)?;
        if self.bounds.native_rect().is_some() {
            raise_preview_window(&self.webview);
        }
        self.webview.load_url(&url).map_err(|e| e.to_string())?;
        self.last_url = url.clone();
        if self.annotate {
            let _ = self.inject_annotate_script();
        }
        Ok(json!({
            "ok": true,
            "url": url,
            "title": "",
            "effector": "embedded-webview",
            "live": true,
            "elements": [],
            "links": [],
        }))
    }

    pub fn reload(&mut self) -> Result<Value, String> {
        if self.last_url.is_empty() {
            if let Ok(url) = self.webview.url() {
                if !url.is_empty() && url != "about:blank" {
                    return self.navigate(&url);
                }
            }
            return Err("no page loaded".into());
        }
        let url = self.last_url.clone();
        self.navigate(&url)
    }

    pub fn back(&mut self) -> Result<Value, String> {
        self.webview
            .evaluate_script("window.history.back();")
            .map_err(|e| e.to_string())?;
        Ok(json!({"ok": true, "action": "back", "effector": "embedded-webview", "live": true}))
    }

    pub fn forward(&mut self) -> Result<Value, String> {
        self.webview
            .evaluate_script("window.history.forward();")
            .map_err(|e| e.to_string())?;
        Ok(json!({"ok": true, "action": "forward", "effector": "embedded-webview", "live": true}))
    }

    pub fn set_annotate(&mut self, enabled: bool) -> Result<Value, String> {
        self.annotate = enabled;
        if enabled {
            if self.bounds.native_rect().is_some() {
                raise_preview_window(&self.webview);
                // Focus live page so the next click hits the document, not the UI.
                #[cfg(target_os = "linux")]
                {
                    use gtk::prelude::*;
                    use wry::WebViewExtUnix;
                    let w = self.webview.webview();
                    w.grab_focus();
                }
            }
            self.inject_annotate_script()?;
        } else {
            self.webview
                .evaluate_script(ANNOTATE_TEARDOWN_JS)
                .map_err(|e| e.to_string())?;
        }
        Ok(json!({"ok": true, "annotate": enabled}))
    }

    pub fn on_navigated(&mut self, url: String) -> Value {
        if !url.is_empty() && url != "about:blank" && !url.starts_with(ANNOT_CALLBACK_PREFIX) {
            self.last_url = url.clone();
        }
        if self.bounds.native_rect().is_some() {
            raise_preview_window(&self.webview);
        }
        if self.annotate {
            // Page document is new; re-bind click capture after a short settle via
            // immediate inject (WebKit runs this after DOM is available for load).
            let _ = self.inject_annotate_script();
        }
        json!({
            "type": "browser_nav",
            "url": url,
            "effector": "embedded-webview",
            "live": true,
        })
    }

    pub fn note_url_if_empty(&mut self, url: &str) {
        if self.last_url.is_empty() && !url.is_empty() {
            self.last_url = url.to_string();
        }
    }

    fn inject_annotate_script(&self) -> Result<(), String> {
        self.webview
            .evaluate_script(ANNOTATE_INJECT_JS)
            .map_err(|e| e.to_string())
    }
}

/// Parse annotation payload from the cancelled-navigation callback URL.
pub fn annotation_from_nav_url(url: &str) -> Option<Value> {
    let raw = url.strip_prefix(ANNOT_CALLBACK_PREFIX)?;
    let decoded = urlencoding_decode(raw);
    let v: Value = serde_json::from_str(&decoded).ok()?;
    if v.get("method").and_then(|m| m.as_str()) == Some("browser_annotation") {
        return v.get("params").cloned();
    }
    // bare params object
    if v.get("tag").is_some() || v.get("text").is_some() || v.get("href").is_some() {
        return Some(v);
    }
    None
}

pub fn is_annotation_callback_url(url: &str) -> bool {
    url.starts_with(ANNOT_CALLBACK_PREFIX)
}

const ANNOTATE_TEARDOWN_JS: &str = r#"(function(){
  try {
    const old = document.getElementById('__optimus_annotate_style');
    if (old) old.remove();
    const tip = document.getElementById('__optimus_annotate_tip');
    if (tip) tip.remove();
    document.documentElement.removeAttribute('data-optimus-annotate');
    if (window.__optimusAnnotateClick) {
      document.removeEventListener('click', window.__optimusAnnotateClick, true);
      window.__optimusAnnotateClick = null;
    }
  } catch (e) {}
})();"#;

const ANNOTATE_INJECT_JS: &str = r#"(function(){
  function postHost(body) {
    var via = null;
    try {
      if (window.ipc && typeof window.ipc.postMessage === 'function') {
        window.ipc.postMessage(body);
        via = 'ipc';
      }
    } catch (e) {}
    try {
      if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.ipc) {
        window.webkit.messageHandlers.ipc.postMessage(body);
        via = via || 'webkit';
      }
    } catch (e) {}
    try {
      if (window.chrome && window.chrome.webview && typeof window.chrome.webview.postMessage === 'function') {
        window.chrome.webview.postMessage(body);
        via = via || 'chrome';
      }
    } catch (e) {}
    // Reliable fallback: cancelled navigation intercepted by the host.
    try {
      var u = 'https://optimus.invalid/__annot?' + encodeURIComponent(body);
      // Prefer hidden iframe so we do not stomp the visible document if deny fails.
      var iframe = document.createElement('iframe');
      iframe.setAttribute('aria-hidden', 'true');
      iframe.style.cssText = 'position:fixed;width:0;height:0;border:0;left:-9999px;top:-9999px;';
      iframe.src = u;
      document.documentElement.appendChild(iframe);
      setTimeout(function(){ try { iframe.remove(); } catch (e) {} }, 50);
      via = via || 'nav';
    } catch (e) {}
    return via;
  }

  if (window.__optimusAnnotateClick) {
    try { document.removeEventListener('click', window.__optimusAnnotateClick, true); } catch (e) {}
  }

  document.documentElement.setAttribute('data-optimus-annotate','1');

  var style = document.getElementById('__optimus_annotate_style');
  if (!style) {
    style = document.createElement('style');
    style.id = '__optimus_annotate_style';
    style.textContent = `
      html[data-optimus-annotate="1"] *:hover {
        outline: 2px solid rgba(255,176,0,.9) !important;
        outline-offset: 1px !important;
        cursor: crosshair !important;
      }
      #__optimus_annotate_tip {
        position: fixed; z-index: 2147483647; left: 12px; bottom: 12px;
        background: rgba(20,20,20,.94); color: #fff;
        font: 12px/1.35 system-ui,sans-serif;
        padding: 8px 10px; border-radius: 8px;
        border: 1px solid rgba(255,255,255,.14);
        pointer-events: none; max-width: min(460px, 86vw);
        box-shadow: 0 8px 24px rgba(0,0,0,.35);
      }
    `;
    document.documentElement.appendChild(style);
  }

  var tip = document.getElementById('__optimus_annotate_tip');
  if (!tip) {
    tip = document.createElement('div');
    tip.id = '__optimus_annotate_tip';
    document.documentElement.appendChild(tip);
  }
  tip.textContent = 'Annotation mode: click an element to pin it';

  window.__optimusAnnotateClick = function(ev) {
    try {
      var el = ev.target;
      if (!el || el.id === '__optimus_annotate_tip') return;
      ev.preventDefault();
      ev.stopPropagation();
      if (typeof ev.stopImmediatePropagation === 'function') ev.stopImmediatePropagation();

      var tag = (el.tagName || '').toLowerCase();
      var text = '';
      try {
        text = ((el.innerText || el.value || el.getAttribute('aria-label') || el.getAttribute('title') || el.alt || '') + '')
          .replace(/\s+/g, ' ').trim().slice(0, 160);
      } catch (e) {}
      var href = el.href || el.getAttribute('href') || '';
      var r = el.getBoundingClientRect();
      var payload = {
        id: Math.floor(Math.random() * 1e9),
        method: 'browser_annotation',
        params: {
          tag: tag,
          text: text,
          href: href,
          url: String(location.href || ''),
          bounds: { x: r.x, y: r.y, width: r.width, height: r.height }
        }
      };
      var body = JSON.stringify(payload);
      var via = postHost(body);
      tip.textContent = via
        ? ('Pinned <' + tag + '> ' + (text || href || '') + ' · via ' + via)
        : ('Pinned <' + tag + '> but host bridge missing');
    } catch (e) {
      try { tip.textContent = 'Annotation click failed: ' + (e && e.message ? e.message : e); } catch (_) {}
    }
    return false;
  };
  document.addEventListener('click', window.__optimusAnnotateClick, true);
})();"#;

/// Persist the child's geometry without changing visibility or sibling order.
/// This is the resize hot path: no remove/put, show/hide, draw queue, or restack.
fn apply_preview_geometry(webview: &WebView, rect: NativeRect) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::*;
        use wry::WebViewExtUnix;

        let widget = webview.webview();
        if let Some(parent) = widget.parent() {
            if let Ok(fixed) = parent.downcast::<gtk::Fixed>() {
                fixed.move_(&widget, rect.x, rect.y);
            }
        }
        // Keep GtkFixed's persistent requisition current. Wry's set_bounds below
        // applies the allocation immediately for same-turn visual convergence.
        widget.set_size_request(rect.w, rect.h);
    }

    webview
        .set_bounds(Rect {
            position: LogicalPosition::new(rect.x as f64, rect.y as f64).into(),
            size: LogicalSize::new(rect.w as f64, rect.h as f64).into(),
        })
        .map_err(|error| error.to_string())
}

/// Re-append once when crossing hidden → visible. Never call this per resize frame.
#[cfg(target_os = "linux")]
fn restack_preview_webview(webview: &WebView, rect: NativeRect) {
    use gtk::prelude::*;
    use wry::WebViewExtUnix;

    let widget = webview.webview();
    if let Some(parent) = widget.parent() {
        if let Ok(fixed) = parent.downcast::<gtk::Fixed>() {
            fixed.remove(&widget);
            fixed.put(&widget, rect.x, rect.y);
            fixed.move_(&widget, rect.x, rect.y);
        }
    }

    widget.set_size_request(rect.w, rect.h);
    widget.size_allocate(&gtk::Allocation::new(rect.x, rect.y, rect.w, rect.h));
    widget.set_hexpand(false);
    widget.set_vexpand(false);
    widget.set_can_focus(true);
    widget.show_all();
    if let Some(gdk_win) = widget.window() {
        gdk_win.raise();
        gdk_win.show();
    }
}

#[cfg(not(target_os = "linux"))]
fn restack_preview_webview(_webview: &WebView, _rect: NativeRect) {}

/// Reassert z-order without reparenting or remapping the persistent child.
#[cfg(target_os = "linux")]
fn raise_preview_window(webview: &WebView) {
    use gtk::prelude::*;
    use wry::WebViewExtUnix;
    if let Some(gdk_win) = webview.webview().window() {
        gdk_win.raise();
    }
}

#[cfg(not(target_os = "linux"))]
fn raise_preview_window(_webview: &WebView) {}

#[cfg(target_os = "linux")]
fn lower_main_webview(main: &WebView, width: u32, height: u32) {
    use gtk::prelude::*;
    use wry::WebViewExtUnix;
    let widget = main.webview();
    let wi = width as i32;
    let hi = height as i32;
    if let Some(parent) = widget.parent() {
        if let Ok(fixed) = parent.downcast::<gtk::Fixed>() {
            // Keep main at origin under later siblings.
            fixed.move_(&widget, 0, 0);
        }
    }
    widget.set_size_request(wi, hi);
    widget.size_allocate(&gtk::Allocation::new(0, 0, wi, hi));
    if let Some(gdk_win) = widget.window() {
        gdk_win.lower();
    }
}

#[cfg(not(target_os = "linux"))]
fn lower_main_webview(_main: &WebView, _width: u32, _height: u32) {}

pub fn normalize_url(raw: &str) -> Result<String, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err("url required".into());
    }
    if t.starts_with(ANNOT_CALLBACK_PREFIX) {
        return Err("blocked annotation callback url".into());
    }
    // Reject explicit non-web schemes before any rewriting.
    if let Ok(early) = url::Url::parse(t) {
        if !matches!(early.scheme(), "http" | "https") {
            return Err(format!("blocked scheme: {}", early.scheme()));
        }
    } else if let Some(scheme) = t.split(':').next() {
        let s = scheme.to_ascii_lowercase();
        if matches!(
            s.as_str(),
            "file" | "javascript" | "data" | "about" | "blob" | "chrome" | "devtools"
        ) {
            return Err(format!("blocked scheme: {s}"));
        }
    }
    let candidate = if t.starts_with("http://") || t.starts_with("https://") {
        t.to_string()
    } else if t.contains(' ') || !t.contains('.') {
        format!("https://www.google.com/search?q={}", urlencoding_lite(t))
    } else {
        format!("https://{t}")
    };
    let parsed = url::Url::parse(&candidate).map_err(|e| e.to_string())?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed.to_string()),
        other => Err(format!("blocked scheme: {other}")),
    }
}

pub fn navigation_allowed(url: &str) -> bool {
    if url == "about:blank" {
        return true;
    }
    if is_annotation_callback_url(url) {
        return false;
    }
    url::Url::parse(url)
        .map(|u| matches!(u.scheme(), "http" | "https"))
        .unwrap_or(false)
}

fn urlencoding_lite(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn urlencoding_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h = |c: u8| -> Option<u8> {
                    match c {
                        b'0'..=b'9' => Some(c - b'0'),
                        b'a'..=b'f' => Some(c - b'a' + 10),
                        b'A'..=b'F' => Some(c - b'A' + 10),
                        _ => None,
                    }
                };
                if let (Some(a), Some(b)) = (h(bytes[i + 1]), h(bytes[i + 2])) {
                    out.push((a << 4) | b);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_blocks_schemes() {
        assert!(normalize_url("example.com")
            .unwrap()
            .starts_with("https://example.com"));
        assert!(normalize_url("https://a.test/x").is_ok());
        assert!(normalize_url("file:///etc/passwd").is_err());
        assert!(normalize_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn search_query_without_dot_goes_to_google() {
        let u = normalize_url("rust wry bounds").unwrap();
        assert!(u.contains("google.com/search"));
    }

    #[test]
    fn embed_bounds_require_minimum_size() {
        let b =
            EmbedBounds::from_params(&json!({"visible": true, "x": 10, "y": 10, "w": 10, "h": 10}));
        assert!(!b.is_usable());
        let b =
            EmbedBounds::from_params(&json!({"visible": true, "x": 10, "y": 10, "w": 40, "h": 40}));
        assert!(b.is_usable());
    }

    #[test]
    fn stable_native_embed_updates_move_without_remounting() {
        let hidden = EmbedBounds::default();
        let visible = EmbedBounds {
            x: 700.0,
            y: 140.0,
            w: 520.0,
            h: 680.0,
            visible: true,
        };
        let moved = EmbedBounds {
            x: 699.0,
            w: 521.0,
            ..visible
        };

        assert_eq!(
            classify_embed_update(hidden, visible),
            EmbedUpdate::ShowAndRestack
        );
        assert_eq!(classify_embed_update(visible, visible), EmbedUpdate::Noop);
        assert_eq!(classify_embed_update(visible, moved), EmbedUpdate::MoveOnly);
        assert_eq!(classify_embed_update(visible, hidden), EmbedUpdate::Hide);
        assert_eq!(classify_embed_update(hidden, hidden), EmbedUpdate::Noop);
    }

    #[test]
    fn annotation_callback_url_parses_envelope() {
        let params = json!({"tag":"h1","text":"Hello","url":"https://example.com/"});
        let env = json!({"id":1,"method":"browser_annotation","params": params});
        let body = serde_json::to_string(&env).unwrap();
        let url = format!("{ANNOT_CALLBACK_PREFIX}{}", urlencoding_lite(&body));
        // urlencoding_lite uses %XX uppercase; our decoder accepts it.
        let parsed = annotation_from_nav_url(&url).expect("parse");
        assert_eq!(parsed["tag"], "h1");
        assert_eq!(parsed["text"], "Hello");
        assert!(is_annotation_callback_url(&url));
        assert!(!navigation_allowed(&url));
    }
}
