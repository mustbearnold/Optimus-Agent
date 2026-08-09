use std::error::Error;
use std::path::Path;

use clap::Args;
use optimus_host::client::{self, ConnectOutcome};
use optimus_host::DEFAULT_HOST_PORT;
use serde_json::{json, Value};

/// `optimus chat` arguments (spec-015 B2). Flattened into the CLI's
/// subcommand enum so the client-mode surface lives with its
/// implementation instead of `main.rs` (module-size ratchet).
#[derive(Args, Debug)]
pub struct ChatArgs {
    /// User message
    pub message: String,
    /// Provider: auto (default) | offline | openai | codex
    #[arg(long, default_value = "auto")]
    pub provider: String,
    /// Override model; `auto` leaves selection to the provider
    #[arg(long)]
    pub model: Option<String>,
    /// Override base URL (openai provider only)
    #[arg(long)]
    pub base_url: Option<String>,
    /// Resume session uuid
    #[arg(long)]
    pub session: Option<String>,
    /// Thinking level: off|minimal|low|medium|high|xhigh|max|ultra
    #[arg(long)]
    pub thinking: Option<String>,
    /// Prefer lower-latency effort cap
    #[arg(long, default_value_t = false)]
    pub fast: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn run_chat(home: &Path, args: ChatArgs, embedded: bool) -> Result<(), Box<dyn Error>> {
    let params = chat_params(&args);
    if embedded {
        run_chat_embedded(home, params)
    } else {
        run_chat_client(home, params)
    }
}

/// Build one turn's wire params. `embedded` and client mode send the same
/// shape, so provider/model routing behaves identically on either path.
fn chat_params(args: &ChatArgs) -> Value {
    let model = model_override(args.model.clone());
    let mut params = json!({
        "message": args.message,
        "provider": args.provider,
        "fast": args.fast,
    });
    if let Some(model) = model {
        params["model"] = json!(model);
    }
    if let Some(base_url) = &args.base_url {
        params["base_url"] = json!(base_url);
    }
    if let Some(session) = &args.session {
        params["session"] = json!(session);
    }
    if let Some(thinking) = args.thinking.as_deref().filter(|value| *value != "off") {
        params["thinking_level"] = json!(thinking);
    }
    params
}

/// The in-process kernel path (`--embedded`): CI and headless use it, and it
/// is byte-for-byte the CLI's pre-B2 behaviour.
fn run_chat_embedded(home: &Path, params: Value) -> Result<(), Box<dyn Error>> {
    let result = optimus_host::chat_turn(&home.to_path_buf(), params, None)
        .map_err(std::io::Error::other)?;
    print_result(&result);
    Ok(())
}

/// Client mode (the B2 default): attach-or-spawn via the B1 host client and
/// speak the surface protocol exactly like the TUI does — hello as `cli`,
/// then one `chat_start` stream drained to its terminal event. A named
/// connect diagnostic is a terminal state (stderr + non-zero exit), the same
/// rule the TUI's `connect_host` enforces.
fn run_chat_client(home: &Path, params: Value) -> Result<(), Box<dyn Error>> {
    let port = std::env::var("OPTIMUS_SERVE_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_HOST_PORT);
    let mut client = match client::connect(home, port) {
        ConnectOutcome::Spawned(client) | ConnectOutcome::Attached(client) => client,
        ConnectOutcome::Diagnostic(diagnostic) => {
            eprintln!("{diagnostic}");
            return Err(diagnostic.to_string().into());
        }
    };
    // The carrier requires hello as its first frame; without it the serve
    // never starts the conversation and the first request hangs.
    client.hello_as("cli").map_err(std::io::Error::other)?;
    let stream_id = client.fresh_stream_id();
    let stream = client
        .start_turn(stream_id, params)
        .map_err(std::io::Error::other)?;
    let terminal = stream.wait_terminal().map_err(std::io::Error::other)?;
    match terminal.get("type").and_then(Value::as_str) {
        Some("done") => {
            let result = terminal.get("result").cloned().unwrap_or(Value::Null);
            // The turn settled: close the carrier (stdio EOF exits 0, R9).
            client.close();
            print_result(&result);
            Ok(())
        }
        Some("error") | Some("cancelled") => {
            let message = terminal
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("turn failed")
                .to_string();
            Err(std::io::Error::other(message).into())
        }
        _ => Err(std::io::Error::other("connection closed before the terminal event").into()),
    }
}

/// Print a settled turn result. The wire `done` event's `result` is the same
/// JSON `chat_turn` returns, so embedded and client mode share one printer.
fn print_result(result: &Value) {
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
        "[provider={reported_provider} session={session_id} steps={} packs={} schema_tokens={} compressed={}]",
        result["steps"],
        result["loaded_packs"],
        result["schema_tokens_final"],
        result["compressed"],
    );
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
