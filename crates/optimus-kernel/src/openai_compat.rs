//! OpenAI-compatible Chat Completions provider.

use serde_json::{json, Value};

use crate::{
    completion_usage_from_value, CompletionRequest, CompletionResponse, CompletionUsage,
    KernelError, Message, ModelProvider, Result, Role, ToolCall, ToolSchema,
};

#[derive(Debug, Clone)]
pub struct OpenAiCompatConfig {
    /// e.g. `https://api.openai.com/v1` or `http://127.0.0.1:1234/v1`
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub organization: Option<String>,
    /// Request timeout seconds.
    pub timeout_secs: u64,
}

impl OpenAiCompatConfig {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPTIMUS_API_KEY").map_err(|_| {
            KernelError::Model("OPTIMUS_API_KEY not set (required for live chat)".into())
        })?;
        let base_url = std::env::var("OPTIMUS_API_BASE")
            .unwrap_or_else(|_| "https://api.openai.com/v1".into());
        let model = std::env::var("OPTIMUS_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".into());
        let organization = std::env::var("OPTIMUS_API_ORG").ok();
        Ok(Self {
            base_url,
            api_key,
            model,
            organization,
            timeout_secs: 120,
        })
    }

    pub fn from_deepseek_env() -> Result<Self> {
        let api_key = std::env::var("DEEPSEEK_API_KEY").map_err(|_| {
            KernelError::Model("DEEPSEEK_API_KEY not set (required for live chat)".into())
        })?;
        Ok(Self::deepseek_with_key(api_key, None))
    }

    /// DeepSeek configuration for a desktop session: the key saved in Settings
    /// first, then `DEEPSEEK_API_KEY`. `from_deepseek_env` stays for callers
    /// that have no Optimus home (CLI one-shots, tests).
    pub fn from_deepseek_home(home: impl AsRef<std::path::Path>) -> Result<Self> {
        let store = crate::ProviderKeyStore::open(home)?;
        match store.resolve(crate::DEEPSEEK_PROVIDER, "DEEPSEEK_API_KEY")? {
            Some((api_key, base_url, _source)) => {
                Ok(Self::deepseek_with_key(api_key, base_url.as_deref()))
            }
            None => Err(KernelError::Model(
                "No DeepSeek API key. Add one in Settings > Authentication, \
                 or set DEEPSEEK_API_KEY before launching."
                    .into(),
            )),
        }
    }

    fn deepseek_with_key(api_key: String, stored_base_url: Option<&str>) -> Self {
        let base_url = stored_base_url
            .map(str::to_string)
            .or_else(|| std::env::var("DEEPSEEK_API_BASE").ok())
            .unwrap_or_else(|| "https://api.deepseek.com".into());
        let model = std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| "deepseek-v4-flash".into());
        Self {
            base_url,
            api_key,
            model,
            organization: None,
            timeout_secs: 180,
        }
    }
}

pub struct OpenAiCompatModel {
    pub config: OpenAiCompatConfig,
    /// Optional override for tests: full chat completions URL.
    pub completions_url_override: Option<String>,
    pub last_usage: Option<CompletionUsage>,
}

impl OpenAiCompatModel {
    pub fn new(config: OpenAiCompatConfig) -> Self {
        Self {
            config,
            completions_url_override: None,
            last_usage: None,
        }
    }

    fn completions_url(&self) -> String {
        if let Some(u) = &self.completions_url_override {
            return u.clone();
        }
        let base = self.config.base_url.trim_end_matches('/');
        if base.ends_with("/chat/completions") {
            base.to_string()
        } else {
            format!("{base}/chat/completions")
        }
    }
}

impl ModelProvider for OpenAiCompatModel {
    fn identity(&self) -> (String, String) {
        ("openai-compat".into(), self.config.model.clone())
    }

    fn last_usage(&self) -> Option<CompletionUsage> {
        self.last_usage.clone()
    }

    fn complete(&mut self, request: CompletionRequest) -> Result<CompletionResponse> {
        self.last_usage = None;
        let body = to_openai_request(&request, &self.config.model);
        let url = self.completions_url();
        let (response, usage) = complete_http(
            &self.config,
            &url,
            body,
            self.config.organization.as_deref(),
        )?;
        self.last_usage = usage;
        Ok(response)
    }
}

/// DeepSeek V4 Chat Completions adapter. It intentionally has its own type so
/// provider-specific thinking and tool replay rules cannot silently change the
/// existing OpenAI-compatible provider.
pub struct DeepseekModel {
    pub config: OpenAiCompatConfig,
    /// Optional override for tests: full chat completions URL.
    pub completions_url_override: Option<String>,
    pub last_usage: Option<CompletionUsage>,
}

impl DeepseekModel {
    pub fn new(config: OpenAiCompatConfig) -> Self {
        Self {
            config,
            completions_url_override: None,
            last_usage: None,
        }
    }

    fn completions_url(&self) -> String {
        completion_url(&self.config, self.completions_url_override.as_ref())
    }
}

impl ModelProvider for DeepseekModel {
    fn identity(&self) -> (String, String) {
        ("deepseek".into(), self.config.model.clone())
    }

    fn last_usage(&self) -> Option<CompletionUsage> {
        self.last_usage.clone()
    }

    fn complete(&mut self, request: CompletionRequest) -> Result<CompletionResponse> {
        self.last_usage = None;
        let body = to_deepseek_request(&request, &self.config.model);
        let url = self.completions_url();
        let (response, usage) = complete_http(&self.config, &url, body, None)?;
        self.last_usage = usage;
        Ok(response)
    }
}

fn completion_url(config: &OpenAiCompatConfig, override_url: Option<&String>) -> String {
    if let Some(url) = override_url {
        return url.clone();
    }
    let base = config.base_url.trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    }
}

fn complete_http(
    config: &OpenAiCompatConfig,
    url: &str,
    body: Value,
    organization: Option<&str>,
) -> Result<(CompletionResponse, Option<CompletionUsage>)> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(config.timeout_secs))
        .build();
    let mut req = agent
        .post(url)
        .set("Content-Type", "application/json")
        .set("Authorization", &format!("Bearer {}", config.api_key));
    if let Some(org) = organization {
        req = req.set("OpenAI-Organization", org);
    }
    // ureq returns every non-2xx as `Err(Error::Status(code, response))`, whose
    // `Display` is just "<url>: status code <code>" — the provider's own
    // explanation sits unread in the response body. Matching the status arm out
    // is what gets that explanation to the user; without it a rejected request
    // reports only its own status line, and a 400 that names the exact malformed
    // message reads as an unexplained failure with nowhere to start.
    let resp = match req.send_json(body) {
        Ok(resp) => resp,
        Err(ureq::Error::Status(status, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            return Err(KernelError::Model(provider_status_message(status, &body)));
        }
        Err(error) => return Err(KernelError::Model(error.to_string())),
    };
    let status = resp.status();
    let text = resp
        .into_string()
        .map_err(|e| KernelError::Model(e.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(KernelError::Model(provider_status_message(status, &text)));
    }
    let value: Value =
        serde_json::from_str(&text).map_err(|e| KernelError::Model(e.to_string()))?;
    let usage = completion_usage_from_value(&value);
    Ok((from_openai_response(&value)?, usage))
}

/// What the user is told when a provider rejects a request.
///
/// OpenAI-compatible errors carry the reason at `/error/message`; that sentence
/// is the whole diagnostic value of the response, so it is lifted to the front
/// rather than left for the reader to find inside a JSON blob. A body in some
/// other shape is passed through bounded, because an unrecognised error is still
/// better read than discarded.
fn provider_status_message(status: u16, body: &str) -> String {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let detail = parsed
        .as_ref()
        .and_then(|value| value.pointer("/error/message"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| body.trim().to_string());
    if detail.is_empty() {
        return format!("provider rejected the request with HTTP {status}");
    }
    let detail: String = detail.chars().take(400).collect();
    format!("provider rejected the request with HTTP {status}: {detail}")
}

pub fn to_openai_request(request: &CompletionRequest, model: &str) -> Value {
    let messages: Vec<Value> = request.messages.iter().map(message_to_openai).collect();
    let tools: Vec<Value> = request.tools.iter().map(tool_to_openai).collect();
    let mut body = json!({
        "model": model,
        "messages": messages,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
        body["tool_choice"] = json!("auto");
    }
    body
}

/// Build a DeepSeek V4 request. Auto is represented by an omitted effort and
/// thinking object, allowing DeepSeek's documented default to choose the
/// budget. Other UI budgets are mapped to the provider's available values.
pub fn to_deepseek_request(request: &CompletionRequest, model: &str) -> Value {
    let messages: Vec<Value> = request.messages.iter().map(message_to_openai).collect();
    let tools: Vec<Value> = request.tools.iter().map(tool_to_openai).collect();
    let mut body = json!({
        "model": model,
        "messages": messages,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
        // DeepSeek accepts tools without forcing an OpenAI-specific
        // tool_choice value; omitting it works across V4 deployments.
    }
    if let Some(effort) = request.reasoning_effort.as_deref() {
        if matches!(effort, "off" | "none" | "false" | "0") {
            body["thinking"] = json!({"type": "disabled"});
        } else {
            body["thinking"] = json!({"type": "enabled"});
            body["reasoning_effort"] = json!(deepseek_reasoning_effort(model, effort));
        }
    }
    body
}

fn deepseek_reasoning_effort(model: &str, effort: &str) -> &'static str {
    let pro = model.eq_ignore_ascii_case("deepseek-v4-pro");
    match effort.trim().to_ascii_lowercase().as_str() {
        "minimal" | "min" | "low" => {
            if pro {
                "high"
            } else {
                "low"
            }
        }
        "medium" | "high" | "extra" | "extra_high" | "x-high" | "xhigh" => {
            if effort.eq_ignore_ascii_case("xhigh") && pro {
                "max"
            } else {
                "high"
            }
        }
        "max" | "maximum" | "ultra" => "max",
        // Keep unknown values valid for the provider rather than forwarding a
        // value DeepSeek cannot parse.
        _ => "high",
    }
}

fn message_to_openai(m: &Message) -> Value {
    let role = match m.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    };
    let mut obj = json!({
        "role": role,
        "content": m.content,
    });
    if let Some(id) = &m.tool_call_id {
        obj["tool_call_id"] = json!(id);
    }
    if let Some(name) = &m.name {
        obj["name"] = json!(name);
    }
    if let Some(reasoning_content) = &m.reasoning_content {
        obj["reasoning_content"] = json!(reasoning_content);
    }
    // If assistant content is a JSON array of ToolCall, expand to tool_calls field.
    if m.role == Role::Assistant {
        if let Ok(calls) = serde_json::from_str::<Vec<ToolCall>>(&m.content) {
            if !calls.is_empty() && calls.iter().all(|c| !c.id.is_empty()) {
                let tc: Vec<Value> = calls
                    .iter()
                    .map(|c| {
                        json!({
                            "id": c.id,
                            "type": "function",
                            "function": {
                                "name": c.name,
                                "arguments": c.arguments.to_string(),
                            }
                        })
                    })
                    .collect();
                obj["content"] = Value::Null;
                obj["tool_calls"] = Value::Array(tc);
            }
        }
    }
    obj
}

fn tool_to_openai(t: &ToolSchema) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": t.id.as_str(),
            "description": t.description,
            "parameters": t.input_schema
        }
    })
}

pub fn from_openai_response(value: &Value) -> Result<CompletionResponse> {
    let choice = value
        .pointer("/choices/0/message")
        .ok_or_else(|| KernelError::Model("missing choices[0].message".into()))?;
    let text = choice
        .get("content")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let reasoning_content = choice
        .get("reasoning_content")
        .and_then(|content| content.as_str())
        .map(str::to_string)
        .filter(|content| !content.is_empty());
    let mut tool_calls = Vec::new();
    if let Some(tool_calls_value) = choice.get("tool_calls") {
        let arr = tool_calls_value
            .as_array()
            .ok_or_else(|| KernelError::Model("tool_calls must be an array".into()))?;
        for tc in arr {
            let tc = tc
                .as_object()
                .ok_or_else(|| KernelError::Model("tool_call must be an object".into()))?;
            if tc.get("type").and_then(|value| value.as_str()) != Some("function") {
                return Err(KernelError::Model("tool_call type must be function".into()));
            }
            let function = tc
                .get("function")
                .and_then(|value| value.as_object())
                .ok_or_else(|| KernelError::Model("tool_call function must be an object".into()))?;
            let id = tc
                .get("id")
                .and_then(|v| v.as_str())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| KernelError::Model("tool_call missing non-empty id".into()))?
                .to_string();
            let name = function
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| KernelError::Model("tool_call missing function name".into()))?
                .to_string();
            let args_raw = function
                .get("arguments")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    KernelError::Model(format!("tool_call {name} missing string arguments"))
                })?;
            let arguments: Value = serde_json::from_str(args_raw).map_err(|error| {
                KernelError::Model(format!(
                    "tool_call {name} has invalid JSON arguments: {error}"
                ))
            })?;
            tool_calls.push(ToolCall {
                id,
                name,
                arguments,
            });
        }
    }
    if text.is_none() && tool_calls.is_empty() {
        return Err(KernelError::Model(
            "provider returned empty content and no tool_calls".into(),
        ));
    }
    Ok(CompletionResponse {
        text,
        tool_calls,
        reasoning_content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Message, Role};
    use optimus_packs::CapabilitySession;

    #[test]
    fn maps_tools_and_messages() {
        let req = CompletionRequest {
            messages: vec![
                Message {
                    role: Role::System,
                    content: "sys".into(),
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
                Message {
                    role: Role::User,
                    content: "hi".into(),
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                },
            ],
            tools: vec![CapabilitySession::with_defaults()
                .resolve_loaded_tool("memory_recall")
                .unwrap()
                .clone()],
            ..Default::default()
        };
        let body = to_openai_request(&req, "gpt-test");
        assert_eq!(body["model"], "gpt-test");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["tools"][0]["function"]["name"], "memory_recall");
        assert_eq!(
            body["tools"][0]["function"]["parameters"],
            req.tools[0].input_schema
        );
        assert_eq!(
            body["tools"][0]["function"]["parameters"]["additionalProperties"],
            false
        );
        assert_eq!(body["tool_choice"], "auto");
    }

    #[test]
    fn a_rejected_request_reports_why_the_provider_rejected_it() {
        // Observed live: a session went permanently unusable and every turn in it
        // reported `model: https://api.deepseek.com/chat/completions: status code
        // 400` — ureq's `Display` for a status error, and nothing else. The cause
        // was named in the response body the client threw away.
        let body = serde_json::json!({
            "error": {
                "message": "Messages with role 'tool' must be a response to a preceding message with 'tool_calls'.",
                "type": "invalid_request_error",
            }
        })
        .to_string();
        let message = provider_status_message(400, &body);

        assert!(message.contains("HTTP 400"), "{message}");
        assert!(
            message.contains("must be a response to a preceding"),
            "{message}"
        );
        assert!(!message.contains("status code 400"), "{message}");
    }

    #[test]
    fn an_error_body_in_an_unexpected_shape_is_still_shown() {
        assert!(
            provider_status_message(503, "upstream unavailable").contains("upstream unavailable")
        );
        assert!(provider_status_message(500, "").contains("HTTP 500"));
        // Bounded: an HTML error page must not become the whole message.
        let long = provider_status_message(502, &"x".repeat(10_000));
        assert!(
            long.len() < 500,
            "unbounded provider body: {} chars",
            long.len()
        );
    }

    #[test]
    fn parses_text_response() {
        let v = json!({
            "choices": [{
                "message": { "content": "hello there", "role": "assistant" }
            }]
        });
        let r = from_openai_response(&v).unwrap();
        assert_eq!(r.text.as_deref(), Some("hello there"));
        assert!(r.tool_calls.is_empty());
    }

    #[test]
    fn parses_tool_calls() {
        let v = json!({
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "memory_recall",
                            "arguments": "{\"subject\":\"user\"}"
                        }
                    }]
                }
            }]
        });
        let r = from_openai_response(&v).unwrap();
        assert!(r.text.is_none());
        assert_eq!(r.tool_calls.len(), 1);
        assert_eq!(r.tool_calls[0].name, "memory_recall");
        assert_eq!(r.tool_calls[0].arguments["subject"], "user");
    }

    #[test]
    fn rejects_malformed_tool_call_arguments() {
        let value = json!({
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{
                        "id": "call_bad",
                        "type": "function",
                        "function": {
                            "name": "memory_recall",
                            "arguments": "{not-json"
                        }
                    }]
                }
            }]
        });
        assert!(matches!(
            from_openai_response(&value),
            Err(KernelError::Model(message))
                if message.contains("memory_recall") && message.contains("invalid JSON arguments")
        ));

        let mut missing = value.clone();
        missing
            .pointer_mut("/choices/0/message/tool_calls/0/function")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("arguments");
        assert!(matches!(
            from_openai_response(&missing),
            Err(KernelError::Model(message))
                if message.contains("memory_recall") && message.contains("missing string arguments")
        ));

        let wrong_container = json!({
            "choices": [{"message": {"content": "safe text", "tool_calls": {"bad": true}}}]
        });
        assert!(matches!(
            from_openai_response(&wrong_container),
            Err(KernelError::Model(message)) if message.contains("tool_calls must be an array")
        ));

        let wrong_type = json!({
            "choices": [{"message": {
                "content": "safe text",
                "tool_calls": [{
                    "id": "bogus-type",
                    "type": "bogus",
                    "function": {"name": "memory_recall", "arguments": "{}"}
                }]
            }}]
        });
        assert!(matches!(
            from_openai_response(&wrong_type),
            Err(KernelError::Model(message)) if message.contains("type must be function")
        ));
    }

    #[test]
    fn assistant_tool_history_expanded() {
        let calls = vec![ToolCall {
            id: "c1".into(),
            name: "activate_pack".into(),
            arguments: json!({"name":"browser"}),
        }];
        let m = Message {
            role: Role::Assistant,
            content: serde_json::to_string(&calls).unwrap(),
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        };
        let o = message_to_openai(&m);
        assert!(o.get("tool_calls").is_some());
        assert_eq!(o["tool_calls"][0]["function"]["name"], "activate_pack");
    }

    #[test]
    fn deepseek_auto_omits_effort_and_tools_use_compatible_shape() {
        let body = to_deepseek_request(
            &CompletionRequest {
                tools: vec![CapabilitySession::with_defaults()
                    .resolve_loaded_tool("memory_recall")
                    .unwrap()
                    .clone()],
                ..Default::default()
            },
            "deepseek-v4-flash",
        );
        assert_eq!(body["model"], "deepseek-v4-flash");
        assert!(body.get("reasoning_effort").is_none());
        assert!(body.get("thinking").is_none());
        assert!(body.get("tool_choice").is_none());
        assert_eq!(body["tools"][0]["function"]["name"], "memory_recall");
    }

    #[test]
    fn deepseek_maps_all_ui_budgets_per_model() {
        let mapped = |model: &str, effort: &str| {
            to_deepseek_request(
                &CompletionRequest {
                    reasoning_effort: Some(effort.into()),
                    ..Default::default()
                },
                model,
            )
        };
        for (effort, expected) in [
            ("minimal", "low"),
            ("low", "low"),
            ("medium", "high"),
            ("high", "high"),
            ("xhigh", "high"),
            ("max", "max"),
            ("ultra", "max"),
        ] {
            let body = mapped("deepseek-v4-flash", effort);
            assert_eq!(body["thinking"]["type"], "enabled");
            assert_eq!(body["reasoning_effort"], expected);
        }
        assert_eq!(mapped("deepseek-v4-pro", "low")["reasoning_effort"], "high");
        assert_eq!(
            mapped("deepseek-v4-pro", "xhigh")["reasoning_effort"],
            "max"
        );
        assert_eq!(mapped("deepseek-v4-pro", "max")["reasoning_effort"], "max");
    }

    #[test]
    fn deepseek_replays_reasoning_content_with_assistant_tool_calls() {
        let calls = vec![ToolCall {
            id: "call_1".into(),
            name: "memory_recall".into(),
            arguments: json!({"subject": "user"}),
        }];
        let body = to_deepseek_request(
            &CompletionRequest {
                messages: vec![Message {
                    role: Role::Assistant,
                    content: serde_json::to_string(&calls).unwrap(),
                    tool_call_id: None,
                    name: None,
                    reasoning_content: Some("private reasoning".into()),
                }],
                ..Default::default()
            },
            "deepseek-v4-flash",
        );
        assert_eq!(
            body["messages"][0]["reasoning_content"],
            "private reasoning"
        );
        assert_eq!(body["messages"][0]["tool_calls"][0]["id"], "call_1");

        let response = from_openai_response(&json!({
            "choices": [{"message": {
                "role": "assistant",
                "content": null,
                "reasoning_content": "returned reasoning",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "memory_recall", "arguments": "{\"subject\":\"user\"}"}
                }]
            }}]
        }))
        .unwrap();
        assert_eq!(
            response.reasoning_content.as_deref(),
            Some("returned reasoning")
        );
    }
}
