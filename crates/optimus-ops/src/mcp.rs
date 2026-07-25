//! Pack-gated MCP client (program P27.c–d).
//!
//! MCP **never** installs a second tool catalog. Server tool offers are mapped
//! to allowlisted [`optimus_packs::ToolDesc`] rows under a pack permission
//! ceiling. Unmapped offers are dropped; collisions with built-in tool ids fail
//! closed. Adapters do not perform FS/process/network side effects themselves.

use std::collections::BTreeSet;
use std::path::Path;

use optimus_packs::{
    assert_policy_within_ceiling, ReplayClass, ToolCancellation, ToolDesc, ToolId,
    ToolIdempotency, ToolInvocation, ToolObservability, ToolOperations, ToolPolicy, ToolRetry,
    ToolTimeout,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum McpError {
    #[error("{0}")]
    Msg(String),
    #[error("mcp tool collides with built-in tool id: {0}")]
    BuiltinCollision(String),
    #[error("mcp tool not in pack allowlist: {0}")]
    NotAllowlisted(String),
    #[error("mcp tool policy outside pack ceiling: {0}")]
    Ceiling(String),
    #[error("http url rejected: {0}")]
    Url(String),
}

pub type Result<T> = std::result::Result<T, McpError>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpTransportKind {
    Stdio,
    Http,
}

/// Server-offered tool (MCP list_tools shape, simplified).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpToolOffer {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub input_schema: Value,
}

/// Session binding: pack + allowlist + ceiling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpSessionConfig {
    pub pack_id: String,
    pub transport: McpTransportKind,
    /// Tool names the pack may map (intersection with server offer).
    pub allowlist: BTreeSet<String>,
    pub max_policies: Vec<ToolPolicy>,
    /// HTTP only: remote endpoint (must pass public URL checks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_url: Option<String>,
}

/// Mapped tool ready for advertisement as ToolDesc (not a parallel catalog).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MappedMcpTool {
    pub offer_name: String,
    pub tool: ToolDesc,
}

/// Map MCP offers → ToolDesc under pack gates. Built-in id collisions fail closed.
pub fn map_mcp_offers(
    config: &McpSessionConfig,
    offers: &[McpToolOffer],
    builtin_tool_ids: &BTreeSet<String>,
) -> Result<Vec<MappedMcpTool>> {
    let mut out = Vec::new();
    for offer in offers {
        if !config.allowlist.contains(&offer.name) {
            // Drop silently — not advertised.
            continue;
        }
        if builtin_tool_ids.contains(&offer.name) {
            return Err(McpError::BuiltinCollision(offer.name.clone()));
        }
        // Third-party MCP tools are catalogued as NetworkRead (or WorkspaceRead)
        // effects only; actual host execution remains SmartDeny-gated Work Graph.
        let policy = ToolPolicy::NetworkRead;
        assert_policy_within_ceiling(policy, config.max_policies.as_slice())
            .map_err(|e| McpError::Ceiling(e.to_string()))?;
        let tool = ToolDesc {
            id: ToolId::new(&offer.name),
            description: offer.description.clone(),
            input_schema: if offer.input_schema.is_null() {
                json!({"type":"object","properties":{},"additionalProperties":false})
            } else {
                offer.input_schema.clone()
            },
            output_schema: optimus_packs::canonical_tool_output_schema(),
            replay: ReplayClass::ExternalNondeterministic,
            policy,
            // Mapped but not a built-in effector — unavailable until a host
            // effector is registered for this ToolId under SmartDeny.
            invocation: ToolInvocation::Unavailable,
            operations: ToolOperations {
                retry: ToolRetry::Never,
                idempotency: ToolIdempotency::None,
                timeout: ToolTimeout::CallerBounded,
                cancellation: ToolCancellation::Unsupported,
                observability: ToolObservability {
                    call_identity: true,
                    trace_span: true,
                    effect_provenance: false,
                },
            },
            schema_tokens: 32,
        };
        out.push(MappedMcpTool {
            offer_name: offer.name.clone(),
            tool,
        });
    }
    Ok(out)
}

/// Deterministic mock stdio MCP list_tools payload (program P27.c).
pub fn mock_stdio_list_tools() -> Vec<McpToolOffer> {
    vec![
        McpToolOffer {
            name: "mcp_echo".into(),
            description: "Echo text via mock MCP stdio server".into(),
            input_schema: json!({
                "type": "object",
                "properties": {"text": {"type": "string"}},
                "required": ["text"],
                "additionalProperties": false
            }),
        },
        McpToolOffer {
            name: "mcp_time".into(),
            description: "Return mock server time".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        },
        // Not allowlisted by default sessions — proves drop behaviour.
        McpToolOffer {
            name: "mcp_dangerous".into(),
            description: "Should never map without allowlist".into(),
            input_schema: json!({"type":"object","properties":{}}),
        },
    ]
}

/// Mock HTTP MCP: validate URL then return the same offer set as stdio.
pub fn mock_http_list_tools(url: &str) -> Result<Vec<McpToolOffer>> {
    assert_public_mcp_url(url)?;
    Ok(mock_stdio_list_tools())
}

/// Strict public HTTP(S) URL gate for MCP (no private/metadata destinations).
pub fn assert_public_mcp_url(url: &str) -> Result<()> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err(McpError::Url("only http(s) schemes allowed".into()));
    }
    let rest = lower
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host = rest.split('/').next().unwrap_or("").split('@').last().unwrap_or("");
    let host = host.split(':').next().unwrap_or(host);
    if host.is_empty() {
        return Err(McpError::Url("missing host".into()));
    }
    let blocked = [
        "localhost",
        "127.0.0.1",
        "0.0.0.0",
        "::1",
        "metadata.google.internal",
        "169.254.169.254",
    ];
    if blocked.iter().any(|b| host == *b) {
        return Err(McpError::Url(format!("blocked host {host}")));
    }
    if host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("169.254.")
        || host.ends_with(".local")
    {
        return Err(McpError::Url(format!("private or link-local host {host}")));
    }
    // 172.16.0.0/12
    if let Some(rest) = host.strip_prefix("172.") {
        if let Some(second) = rest.split('.').next() {
            if let Ok(n) = second.parse::<u8>() {
                if (16..=31).contains(&n) {
                    return Err(McpError::Url(format!("private host {host}")));
                }
            }
        }
    }
    Ok(())
}

/// Default mock session for tests / desktop status.
pub fn default_mock_session() -> McpSessionConfig {
    McpSessionConfig {
        pack_id: "mcp.mock".into(),
        transport: McpTransportKind::Stdio,
        allowlist: ["mcp_echo".into(), "mcp_time".into()].into_iter().collect(),
        max_policies: optimus_packs::default_third_party_ceiling(),
        http_url: None,
    }
}

/// End-to-end stdio mock: list → map under session.
pub fn stdio_mock_bind(
    config: &McpSessionConfig,
    builtin_tool_ids: &BTreeSet<String>,
) -> Result<Vec<MappedMcpTool>> {
    if config.transport != McpTransportKind::Stdio {
        return Err(McpError::Msg("stdio_mock_bind requires stdio transport".into()));
    }
    map_mcp_offers(config, &mock_stdio_list_tools(), builtin_tool_ids)
}

/// End-to-end HTTP mock: URL gate → list → map.
pub fn http_mock_bind(
    config: &McpSessionConfig,
    builtin_tool_ids: &BTreeSet<String>,
) -> Result<Vec<MappedMcpTool>> {
    if config.transport != McpTransportKind::Http {
        return Err(McpError::Msg("http_mock_bind requires http transport".into()));
    }
    let url = config
        .http_url
        .as_deref()
        .ok_or_else(|| McpError::Msg("http_url required".into()))?;
    let offers = mock_http_list_tools(url)?;
    map_mcp_offers(config, &offers, builtin_tool_ids)
}

/// Collect built-in tool ids from a home-independent packs catalog.
pub fn builtin_tool_id_set() -> BTreeSet<String> {
    optimus_packs::CapabilitySession::with_defaults()
        .catalog()
        .values()
        .flat_map(|pack| pack.tools.iter().map(|t| t.id.as_str().to_string()))
        .collect()
}

/// Persist optional MCP session config under `{home}/mcp/session.json`.
pub fn load_mcp_session(home: impl AsRef<Path>) -> Result<McpSessionConfig> {
    let path = home.as_ref().join("mcp").join("session.json");
    if !path.exists() {
        return Ok(default_mock_session());
    }
    let raw = std::fs::read_to_string(path).map_err(|e| McpError::Msg(e.to_string()))?;
    serde_json::from_str(&raw).map_err(|e| McpError::Msg(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use optimus_packs::ToolPolicy;

    #[test]
    fn stdio_mock_maps_allowlisted_only() {
        let config = default_mock_session();
        let builtins = builtin_tool_id_set();
        let mapped = stdio_mock_bind(&config, &builtins).unwrap();
        let names: BTreeSet<_> = mapped.iter().map(|m| m.offer_name.as_str()).collect();
        assert!(names.contains("mcp_echo"));
        assert!(names.contains("mcp_time"));
        assert!(!names.contains("mcp_dangerous"));
        assert!(mapped.iter().all(|m| m.tool.invocation == ToolInvocation::Unavailable));
    }

    #[test]
    fn builtin_collision_fails_closed() {
        let mut config = default_mock_session();
        config.allowlist.insert("web_search".into());
        let offers = vec![McpToolOffer {
            name: "web_search".into(),
            description: "collide".into(),
            input_schema: json!({}),
        }];
        let builtins = builtin_tool_id_set();
        let err = map_mcp_offers(&config, &offers, &builtins).unwrap_err();
        assert!(matches!(err, McpError::BuiltinCollision(_)));
    }

    #[test]
    fn http_rejects_private_and_accepts_public() {
        assert!(assert_public_mcp_url("https://api.example.com/mcp").is_ok());
        assert!(assert_public_mcp_url("http://127.0.0.1/mcp").is_err());
        assert!(assert_public_mcp_url("http://192.168.1.1/mcp").is_err());
        assert!(assert_public_mcp_url("http://169.254.169.254/latest").is_err());
        assert!(assert_public_mcp_url("ftp://example.com").is_err());
        let mut config = default_mock_session();
        config.transport = McpTransportKind::Http;
        config.http_url = Some("https://mcp.example.com/v1".into());
        let mapped = http_mock_bind(&config, &builtin_tool_id_set()).unwrap();
        assert!(!mapped.is_empty());
    }

    #[test]
    fn ceiling_blocks_process_policy() {
        let mut config = default_mock_session();
        config.max_policies.retain(|p| *p != ToolPolicy::NetworkRead);
        // NetworkRead not in ceiling → mapping fails for allowlisted tools.
        let err = stdio_mock_bind(&config, &builtin_tool_id_set()).unwrap_err();
        assert!(matches!(err, McpError::Ceiling(_)));
    }
}
