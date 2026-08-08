//! Provider-reported token accounting shared by model adapters and execution.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Provider-reported usage for one model completion.
///
/// Every field is optional because compatible gateways frequently omit usage
/// or expose only a subset. Optimus never estimates token counts from text
/// length when the provider did not report them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CompletionUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u64>,
}

impl CompletionUsage {
    pub(crate) fn is_empty(&self) -> bool {
        self.input_tokens.is_none()
            && self.output_tokens.is_none()
            && self.total_tokens.is_none()
            && self.reasoning_tokens.is_none()
            && self.cached_input_tokens.is_none()
            && self.cache_write_tokens.is_none()
    }
}

/// Extract the common usage shapes emitted by OpenAI-compatible and Responses
/// providers. Unknown provider-specific fields stay unknown.
pub(crate) fn completion_usage_from_value(value: &Value) -> Option<CompletionUsage> {
    let usage = value.get("usage")?;
    let number = |key: &str| usage.get(key).and_then(Value::as_u64);
    let nested_number = |objects: &[&str], key: &str| {
        objects
            .iter()
            .find_map(|object| usage.get(*object).and_then(|value| value.get(key)))
            .and_then(Value::as_u64)
    };
    let parsed = CompletionUsage {
        input_tokens: number("input_tokens").or_else(|| number("prompt_tokens")),
        output_tokens: number("output_tokens").or_else(|| number("completion_tokens")),
        total_tokens: number("total_tokens"),
        reasoning_tokens: number("reasoning_tokens").or_else(|| {
            nested_number(
                &["output_tokens_details", "completion_tokens_details"],
                "reasoning_tokens",
            )
        }),
        cached_input_tokens: number("cached_input_tokens")
            .or_else(|| number("cached_tokens"))
            .or_else(|| {
                nested_number(
                    &["input_tokens_details", "prompt_tokens_details"],
                    "cached_tokens",
                )
            }),
        cache_write_tokens: number("cache_write_tokens").or_else(|| {
            nested_number(
                &["input_tokens_details", "prompt_tokens_details"],
                "cache_write_tokens",
            )
        }),
    };
    (!parsed.is_empty()).then_some(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_openai_chat_completion_shape() {
        let value = json!({
            "model": "gpt-4o",
            "usage": {
                "prompt_tokens": 120,
                "completion_tokens": 40,
                "total_tokens": 160,
            },
        });
        let usage = completion_usage_from_value(&value).expect("usage");
        assert_eq!(usage.input_tokens, Some(120));
        assert_eq!(usage.output_tokens, Some(40));
        assert_eq!(usage.total_tokens, Some(160));
        assert_eq!(usage.reasoning_tokens, None);
        assert_eq!(usage.cached_input_tokens, None);
        assert_eq!(usage.cache_write_tokens, None);
    }

    #[test]
    fn parses_responses_shape_with_details() {
        let value = json!({
            "usage": {
                "input_tokens": 200,
                "output_tokens": 50,
                "total_tokens": 250,
                "output_tokens_details": { "reasoning_tokens": 11 },
                "input_tokens_details": {
                    "cached_tokens": 90,
                    "cache_write_tokens": 3,
                },
            },
        });
        let usage = completion_usage_from_value(&value).expect("usage");
        assert_eq!(usage.input_tokens, Some(200));
        assert_eq!(usage.output_tokens, Some(50));
        assert_eq!(usage.total_tokens, Some(250));
        assert_eq!(usage.reasoning_tokens, Some(11));
        assert_eq!(usage.cached_input_tokens, Some(90));
        assert_eq!(usage.cache_write_tokens, Some(3));
    }

    #[test]
    fn direct_fields_win_over_nested_details() {
        // Anthropic-style direct fields must shadow any nested details.
        let value = json!({
            "usage": {
                "input_tokens": 9,
                "output_tokens": 4,
                "input_tokens_details": { "cached_tokens": 999 },
            },
        });
        let usage = completion_usage_from_value(&value).expect("usage");
        assert_eq!(usage.input_tokens, Some(9));
        assert_eq!(usage.output_tokens, Some(4));
        assert_eq!(usage.cached_input_tokens, Some(999));
    }

    #[test]
    fn missing_or_empty_usage_is_none() {
        assert_eq!(completion_usage_from_value(&json!({})), None);
        assert_eq!(completion_usage_from_value(&json!({"usage": {}})), None);
    }

    #[test]
    fn unknown_fields_do_not_invent_tokens() {
        let value = json!({
            "usage": {
                "prompt_tokens": 1,
                "completion_tokens": 2,
                "weird_custom_field": 999,
            },
        });
        let usage = completion_usage_from_value(&value).expect("usage");
        assert_eq!(usage.total_tokens, None);
        assert_eq!(usage.reasoning_tokens, None);
    }
}
