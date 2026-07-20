//! Build-time assembly of the single desktop document.
//!
//! CSS and JavaScript stay in separate source files while both native WebView2
//! and the HTTP Playwright server receive the same self-contained HTML.

const HTML_SHELL: &str = include_str!("../ui/index.html");
const STYLE: &str = include_str!("../ui/style.css");
const APP_JS: &str = include_str!("../ui/app.js");
const CSS_MARKER: &str = "/*__OPTIMUS_CSS__*/";
const JS_MARKER: &str = "/*__OPTIMUS_JS__*/";

pub fn render_html() -> String {
    assert_eq!(
        HTML_SHELL.matches(CSS_MARKER).count(),
        1,
        "desktop HTML shell must contain exactly one CSS marker"
    );
    assert_eq!(
        HTML_SHELL.matches(JS_MARKER).count(),
        1,
        "desktop HTML shell must contain exactly one JS marker"
    );
    HTML_SHELL
        .replace(CSS_MARKER, STYLE)
        .replace(JS_MARKER, APP_JS)
}

#[cfg(test)]
mod tests {
    use super::render_html;

    #[test]
    fn assembled_document_has_no_asset_placeholders() {
        let html = render_html();
        assert!(!html.contains("/*__OPTIMUS_CSS__*/"));
        assert!(!html.contains("/*__OPTIMUS_JS__*/"));
        assert!(html.contains("document.addEventListener('DOMContentLoaded'"));
        assert!(html.contains("class=\"shell\""));
        assert!(html.contains("<style>"));
        assert!(html.contains("<script>"));
    }
}
