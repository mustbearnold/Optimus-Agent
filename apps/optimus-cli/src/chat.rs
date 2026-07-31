use std::error::Error;
use std::path::Path;

use serde_json::json;

#[allow(clippy::too_many_arguments)]
pub fn run_chat(
    home: &Path,
    message: String,
    provider: String,
    model: Option<String>,
    base_url: Option<String>,
    session: Option<String>,
    thinking: Option<String>,
    fast: bool,
) -> Result<(), Box<dyn Error>> {
    let model = model_override(model);
    let mut params = json!({
        "message": message,
        "provider": provider,
        "fast": fast,
    });
    if let Some(model) = model {
        params["model"] = json!(model);
    }
    if let Some(base_url) = base_url {
        params["base_url"] = json!(base_url);
    }
    if let Some(session) = session {
        params["session"] = json!(session);
    }
    if let Some(thinking) = thinking.filter(|value| value != "off") {
        params["thinking_level"] = json!(thinking);
    }
    let result = optimus_host::chat_turn(&home.to_path_buf(), params, None)
        .map_err(std::io::Error::other)?;
    let session_id = result["session_id"].as_str().unwrap_or("unknown");
    let reported_provider = result["provider"].as_str().unwrap_or("unknown");
    println!("session {session_id}");
    if reported_provider == "codex" {
        println!("model {}", result["model"].as_str().unwrap_or("unknown"));
    }
    println!("{}", result["assistant_text"].as_str().unwrap_or_default());
    let tool_trace = result["tool_trace"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>();
    if !tool_trace.is_empty() {
        println!("[tools: {}]", tool_trace.join(" | "));
    }
    println!(
        "[provider={reported_provider} session={} steps={} packs={} schema_tokens={} compressed={}]",
        session_id,
        result["steps"],
        result["loaded_packs"],
        result["schema_tokens_final"],
        result["compressed"],
    );
    Ok(())
}

pub fn drain_gateway_once(
    home: &Path,
) -> Result<Option<optimus_kernel::DrainResult>, Box<dyn Error>> {
    optimus_host::drain_gateway_once(&home.to_path_buf())
        .map_err(|error| std::io::Error::other(error).into())
}

fn model_override(model: Option<String>) -> Option<String> {
    model.filter(|value| !value.trim().eq_ignore_ascii_case("auto"))
}

#[cfg(test)]
mod tests {
    use super::model_override;

    #[test]
    fn model_auto_is_no_override() {
        assert_eq!(model_override(None), None);
        assert_eq!(model_override(Some("auto".into())), None);
        assert_eq!(model_override(Some("Auto".into())), None);
    }

    #[test]
    fn an_explicit_model_remains_unchanged() {
        assert_eq!(
            model_override(Some("gpt-5.6-sol".into())).as_deref(),
            Some("gpt-5.6-sol")
        );
    }
}
