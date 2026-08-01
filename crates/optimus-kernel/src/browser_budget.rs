//! Bounding a browser effector result to the canonical outcome budget.
//!
//! The CDP effector's raw result inlines a full-page screenshot and an
//! unbudgeted element list. A 1440x1200 capture alone is several hundred KiB
//! of base64, so every CDP navigate cleared Chrome's launch tax and then
//! failed canonical outcome validation (observed at 70,083 ms in the
//! 2026-07-31 round-2 TUI evidence). The model cannot read inline base64
//! anyway; the pixels belong in the artifact store, and the outcome carries
//! their reference. HTTP results pass through unchanged — their budget is
//! `page_to_tool_json` in `browser`.

use std::path::Path;

use optimus_artifacts::ArtifactStore;
use serde_json::json;

/// Chars of serialized `elements` carried inline in a CDP tool result.
///
/// Sized like the HTTP page budget: enough for every element a model can
/// usefully click, comfortably under the canonical data ceiling once the
/// screenshot no longer rides along.
const MAX_ELEMENTS_CHARS: usize = 18_000;

/// Bound one effector result. Anything unparseable passes through untouched:
/// this function makes results smaller, never invents failure.
pub(crate) fn bound_browser_tool_json(home: &Path, raw: String) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return raw;
    };
    if !value.is_object() {
        return raw;
    }
    publish_screenshot(home, &mut value);
    budget_elements(&mut value);
    value.to_string()
}

/// Move an inline screenshot into the content-addressed artifact store.
///
/// The inline field is removed even when the store is unavailable — an
/// outcome that states `screenshot_dropped` is honest, while one that fails
/// canonical validation wastes the whole navigate. Mirrors the host preview
/// path's labeling so both routes address the same blob identically.
fn publish_screenshot(home: &Path, value: &mut serde_json::Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    let Some(shot) = obj.remove("screenshot_b64") else {
        return;
    };
    let Some(b64) = shot.as_str().filter(|s| !s.is_empty()) else {
        return;
    };
    let title = obj
        .get("page_title")
        .or_else(|| obj.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("page");
    let url = obj
        .get("final_url")
        .or_else(|| obj.get("url"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let label = if url.is_empty() {
        format!("screenshot · {title}")
    } else {
        format!("screenshot · {title} · {url}")
    };
    let published = ArtifactStore::open(home)
        .and_then(|store| store.put_base64(b64, "image/png", "browser.screenshot", &label, None));
    match published {
        Ok(record) => {
            obj.insert("artifact_sha256".into(), json!(record.sha256));
            obj.insert("artifact_size_bytes".into(), json!(record.size_bytes));
        }
        Err(error) => {
            obj.insert(
                "screenshot_dropped".into(),
                json!(format!("artifact store unavailable: {error}")),
            );
        }
    }
}

/// Keep elements while they fit, and say so when they do not.
fn budget_elements(value: &mut serde_json::Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    let Some(serde_json::Value::Array(elements)) = obj.get("elements") else {
        return;
    };
    let total = elements.len();
    let mut remaining = MAX_ELEMENTS_CHARS;
    let mut kept = Vec::new();
    for element in elements {
        let cost = element.to_string().chars().count();
        if cost > remaining {
            break;
        }
        remaining -= cost;
        kept.push(element.clone());
    }
    if kept.len() < total {
        obj.insert(
            "elements_truncated".into(),
            json!(format!("{} of {total} elements", kept.len())),
        );
        obj.insert("elements".into(), serde_json::Value::Array(kept));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use optimus_packs::MAX_TOOL_OUTCOME_DATA_BYTES;
    use serde_json::Value;
    use tempfile::tempdir;

    /// Valid base64 well past the canonical ceiling once inlined: 400k chars
    /// decoding to 300k bytes, the shape of a real full-page capture.
    fn oversized_screenshot() -> String {
        "QUFB".repeat(100_000)
    }

    fn cdp_result(screenshot: &str, elements: usize) -> String {
        json!({
            "ok": true,
            "effector": "cdp-browser",
            "final_url": "https://example.com/page",
            "page_title": "Example",
            "text": "Example — https://example.com/page",
            "element_count": elements,
            "elements": (0..elements)
                .map(|i| json!({"index": i, "role": "link", "text": format!("element {i}")}))
                .collect::<Vec<_>>(),
            "screenshot_b64": screenshot,
        })
        .to_string()
    }

    #[test]
    fn a_screenshot_moves_to_the_artifact_store_instead_of_the_outcome() {
        let home = tempdir().expect("home");
        let raw = cdp_result(&oversized_screenshot(), 4);
        assert!(
            raw.len() > MAX_TOOL_OUTCOME_DATA_BYTES,
            "fixture must exceed the ceiling"
        );

        let bounded = bound_browser_tool_json(home.path(), raw);
        assert!(bounded.len() < MAX_TOOL_OUTCOME_DATA_BYTES);

        let value: Value = serde_json::from_str(&bounded).expect("json");
        assert!(
            value.get("screenshot_b64").is_none(),
            "pixels must not ride the outcome"
        );
        let sha = value["artifact_sha256"]
            .as_str()
            .expect("artifact reference");
        assert_eq!(sha.len(), 64);
        assert!(value["artifact_size_bytes"].as_u64().expect("size") > 0);
        assert!(
            home.path().join("artifacts").join("blobs").is_dir(),
            "the blob store holds the actual capture"
        );
    }

    #[test]
    fn a_dead_artifact_store_still_keeps_the_outcome_under_budget() {
        let dir = tempdir().expect("dir");
        let file_as_home = dir.path().join("not-a-directory");
        std::fs::write(&file_as_home, b"occupied").expect("file");

        let bounded =
            bound_browser_tool_json(&file_as_home, cdp_result(&oversized_screenshot(), 4));
        assert!(bounded.len() < MAX_TOOL_OUTCOME_DATA_BYTES);

        let value: Value = serde_json::from_str(&bounded).expect("json");
        assert!(value.get("screenshot_b64").is_none());
        assert!(value.get("artifact_sha256").is_none());
        let dropped = value["screenshot_dropped"].as_str().expect("stated drop");
        assert!(dropped.contains("artifact store unavailable"));
    }

    #[test]
    fn element_floods_are_truncated_and_stated() {
        let home = tempdir().expect("home");
        let bounded = bound_browser_tool_json(home.path(), cdp_result("", 5_000));
        assert!(bounded.len() < MAX_TOOL_OUTCOME_DATA_BYTES);

        let value: Value = serde_json::from_str(&bounded).expect("json");
        let kept = value["elements"].as_array().expect("elements").len();
        assert!(kept > 0, "the budget keeps a real sample");
        assert!(kept < 5_000);
        assert_eq!(
            value["elements_truncated"],
            json!(format!("{kept} of 5000 elements")),
        );
        assert_eq!(
            value["element_count"], 5_000,
            "the true count stays visible"
        );
    }

    #[test]
    fn http_results_pass_through_semantically_unchanged() {
        let home = tempdir().expect("home");
        let raw = json!({
            "ok": true,
            "effector": "http-browser",
            "url": "https://example.com",
            "final_url": "https://example.com",
            "status": 200,
            "title": "Example",
            "text": "hello",
            "links": [{"index": 0, "text": "next", "href": "https://example.com/2"}],
            "fetched_at": "2026-08-01T00:00:00Z",
        })
        .to_string();
        let bounded = bound_browser_tool_json(home.path(), raw.clone());
        let before: Value = serde_json::from_str(&raw).expect("raw json");
        let after: Value = serde_json::from_str(&bounded).expect("bounded json");
        assert_eq!(before, after);
    }

    #[test]
    fn unparseable_results_are_left_for_the_caller_to_report() {
        let home = tempdir().expect("home");
        let raw = "chrome exploded mid-sentence".to_string();
        assert_eq!(bound_browser_tool_json(home.path(), raw.clone()), raw);
    }
}
