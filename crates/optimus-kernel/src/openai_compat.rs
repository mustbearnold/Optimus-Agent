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
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(self.config.timeout_secs))
            .build();
        let mut req = agent
            .post(&url)
            .set("Content-Type", "application/json")
            .set("Authorization", &format!("Bearer {}", self.config.api_key));
        if let Some(org) = &self.config.organization {
            req = req.set("OpenAI-Organization", org);
        }
        let resp = req
            .send_json(body)
            .map_err(|e| KernelError::Model(e.to_string()))?;
        let status = resp.status();
        let text = resp
            .into_string()
            .map_err(|e| KernelError::Model(e.to_string()))?;
        if !(200..300).contains(&status) {
            let snippet: String = text.chars().take(400).collect();
            return Err(KernelError::Model(format!(
                "HTTP {status} from provider: {snippet}"
            )));
        }
        let value: Value =
            serde_json::from_str(&text).map_err(|e| KernelError::Model(e.to_string()))?;
        self.last_usage = completion_usage_from_value(&value);
        from_openai_response(&value)
    }
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
    Ok(CompletionResponse { text, tool_calls })
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
                },
                Message {
                    role: Role::User,
                    content: "hi".into(),
                    tool_call_id: None,
                    name: None,
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
        };
        let o = message_to_openai(&m);
        assert!(o.get("tool_calls").is_some());
        assert_eq!(o["tool_calls"][0]["function"]["name"], "activate_pack");
    }
}
