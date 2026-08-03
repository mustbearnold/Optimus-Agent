//! Parsers for Codex Responses JSON and server-sent events.

use crate::{CompletionResponse, KernelError, Result, ToolCall};
use serde_json::{json, Value};

pub fn from_codex_responses_sse(stream: &str) -> Result<CompletionResponse> {
    let mut text_buf = String::new();
    let mut tool_calls = Vec::new();
    let mut completed_output: Option<Value> = None;

    for line in stream.lines() {
        let line = line.trim();
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.strip_prefix(' ').unwrap_or(data);
        if data == "[DONE]" {
            break;
        }
        let ev: Value = serde_json::from_str(data).map_err(|error| {
            KernelError::Model(format!("Codex SSE event has invalid JSON: {error}"))
        })?;
        let ty = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match ty {
            "response.output_text.delta" => {
                if let Some(d) = ev.get("delta").and_then(|x| x.as_str()) {
                    text_buf.push_str(d);
                }
            }
            "response.output_item.done" => {
                if let Some(item) = ev.get("item") {
                    let item_ty = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if item_ty == "function_call" {
                        let id = item
                            .get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(|x| x.as_str())
                            .filter(|value| !value.is_empty())
                            .ok_or_else(|| {
                                KernelError::Model("tool_call missing non-empty id".into())
                            })?
                            .to_string();
                        let name = item
                            .get("name")
                            .and_then(|x| x.as_str())
                            .filter(|value| !value.is_empty())
                            .ok_or_else(|| {
                                KernelError::Model("tool_call missing function name".into())
                            })?
                            .to_string();
                        let args_raw =
                            item.get("arguments")
                                .and_then(|x| x.as_str())
                                .ok_or_else(|| {
                                    KernelError::Model(format!(
                                        "tool_call {name} missing string arguments"
                                    ))
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
            }
            "response.completed" => {
                completed_output = ev.get("response").and_then(|r| r.get("output")).cloned();
            }
            _ => {}
        }
    }

    if let Some(output) = completed_output {
        if !output.as_array().is_some_and(|items| items.is_empty()) {
            let parsed = from_codex_responses_response(&json!({ "output": output }))?;
            if tool_calls.is_empty()
                && (!parsed.tool_calls.is_empty() || (text_buf.is_empty() && parsed.text.is_some()))
            {
                return Ok(parsed);
            }
        }
    }

    if text_buf.is_empty() && tool_calls.is_empty() {
        return Err(KernelError::Model(
            "Codex SSE: empty text and no tool_calls".into(),
        ));
    }
    Ok(CompletionResponse {
        text: if text_buf.is_empty() {
            None
        } else {
            Some(text_buf)
        },
        tool_calls,
    })
}

pub fn from_codex_responses_response(value: &Value) -> Result<CompletionResponse> {
    let mut text: Option<String> = None;
    let mut tool_calls = Vec::new();
    if let Some(output_value) = value.get("output") {
        let output = output_value
            .as_array()
            .ok_or_else(|| KernelError::Model("Codex output must be an array".into()))?;
        for item in output {
            let ty = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match ty {
                "message" => {
                    if let Some(parts) = item.get("content").and_then(|c| c.as_array()) {
                        let mut buf = String::new();
                        for p in parts {
                            let pt = p.get("type").and_then(|t| t.as_str()).unwrap_or("");
                            if pt == "output_text" || pt == "text" {
                                if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                                    buf.push_str(t);
                                }
                            }
                        }
                        if !buf.is_empty() {
                            text = Some(buf);
                        }
                    }
                }
                "function_call" => {
                    let id = item
                        .get("call_id")
                        .or_else(|| item.get("id"))
                        .and_then(|x| x.as_str())
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| KernelError::Model("tool_call missing non-empty id".into()))?
                        .to_string();
                    let name = item
                        .get("name")
                        .and_then(|x| x.as_str())
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            KernelError::Model("tool_call missing function name".into())
                        })?
                        .to_string();
                    let args_raw =
                        item.get("arguments")
                            .and_then(|x| x.as_str())
                            .ok_or_else(|| {
                                KernelError::Model(format!(
                                    "tool_call {name} missing string arguments"
                                ))
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
                _ => {}
            }
        }
    }
    if text.is_none() {
        if let Some(t) = value.get("output_text").and_then(|x| x.as_str()) {
            if !t.is_empty() {
                text = Some(t.to_string());
            }
        }
    }
    if text.is_none() && tool_calls.is_empty() {
        return Err(KernelError::Model(
            "Codex responses: empty text and no tool_calls".into(),
        ));
    }
    Ok(CompletionResponse { text, tool_calls })
}
