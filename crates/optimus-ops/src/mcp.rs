//! Pack-gated MCP client (program P27.c–d).
//!
//! MCP **never** installs a second tool catalog. Server tool offers are mapped
//! to allowlisted [`optimus_packs::ToolDesc`] rows under a pack permission
//! ceiling. Unmapped offers are dropped; collisions with built-in tool ids fail
//! closed. Adapters do not perform FS/process/network side effects themselves.

use std::collections::BTreeSet;
use std::path::Path;

use optimus_packs::{
    assert_policy_within_ceiling, ReplayClass, ToolCancellation, ToolDesc, ToolId, ToolIdempotency,
    ToolInvocation, ToolObservability, ToolOperations, ToolPolicy, ToolRetry, ToolTimeout,
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
///
/// Intentionally mirrors kernel `assert_public_http_url` strength for schemes
/// and host classes (ops cannot depend on kernel). Live DNS sampling remains a
/// residual when a real HTTP client is added.
pub fn assert_public_mcp_url(url: &str) -> Result<()> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err(McpError::Url("only http(s) schemes allowed".into()));
    }
    let rest = lower
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    // strip userinfo
    let authority = rest
        .split('/')
        .next()
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("");
    let hostport = authority.split('@').last().unwrap_or("");
    let host = if hostport.starts_with('[') {
        // IPv6 literal [::1]:port
        hostport
            .trim_start_matches('[')
            .split(']')
            .next()
            .unwrap_or("")
            .to_string()
    } else {
        hostport.split(':').next().unwrap_or(hostport).to_string()
    };
    if host.is_empty() {
        return Err(McpError::Url("missing host".into()));
    }
    if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host == "metadata.google.internal"
    {
        return Err(McpError::Url(format!("blocked host {host}")));
    }
    // IPv6 loopback / ULA / link-local (string forms)
    if host == "::1"
        || host == "0:0:0:0:0:0:0:1"
        || host.starts_with("fe80:")
        || host.starts_with("fc")
        || host.starts_with("fd")
        || host.starts_with("::ffff:127.")
        || host.starts_with("::ffff:10.")
        || host.starts_with("::ffff:192.168.")
        || host.starts_with("::ffff:169.254.")
    {
        return Err(McpError::Url(format!("blocked ip literal {host}")));
    }
    // IPv4 dotted or decimal 32-bit
    if let Ok(ip) = host.parse::<u32>() {
        // decimal form of IPv4 e.g. 2130706433 == 127.0.0.1
        let a = ((ip >> 24) & 0xff) as u8;
        let b = ((ip >> 16) & 0xff) as u8;
        if is_private_v4(a, b, 0, 0) {
            return Err(McpError::Url(format!("blocked decimal ip {host}")));
        }
    }
    if let Some((a, b, c, d)) = parse_dotted_v4(&host) {
        if is_private_v4(a, b, c, d) {
            return Err(McpError::Url(format!("blocked private/link-local {host}")));
        }
    }
    // hex/octal-ish 0x7f.0.0.1
    if host.contains("0x")
        || host
            .split('.')
            .any(|p| p.starts_with('0') && p.len() > 1 && p.chars().all(|c| c.is_ascii_digit()))
    {
        // conservative reject for non-decimal dotted forms
        if host.starts_with("0x") || host.contains(".0x") {
            return Err(McpError::Url(format!("blocked non-decimal ip form {host}")));
        }
    }
    Ok(())
}

fn parse_dotted_v4(host: &str) -> Option<(u8, u8, u8, u8)> {
    let parts: Vec<_> = host.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let a = parts[0].parse().ok()?;
    let b = parts[1].parse().ok()?;
    let c = parts[2].parse().ok()?;
    let d = parts[3].parse().ok()?;
    Some((a, b, c, d))
}

fn is_private_v4(a: u8, b: u8, c: u8, d: u8) -> bool {
    let _ = (c, d);
    a == 0
        || a == 10
        || a == 127
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 100 && (64..=127).contains(&b)) // CGNAT
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
        return Err(McpError::Msg(
            "stdio_mock_bind requires stdio transport".into(),
        ));
    }
    map_mcp_offers(config, &mock_stdio_list_tools(), builtin_tool_ids)
}

/// End-to-end HTTP mock: URL gate → list → map.
pub fn http_mock_bind(
    config: &McpSessionConfig,
    builtin_tool_ids: &BTreeSet<String>,
) -> Result<Vec<MappedMcpTool>> {
    if config.transport != McpTransportKind::Http {
        return Err(McpError::Msg(
            "http_mock_bind requires http transport".into(),
        ));
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
///
/// Host clamps `max_policies` to the third-party ceiling (cannot expand
/// Process/NetworkWrite/Desktop via session JSON).
pub fn load_mcp_session(home: impl AsRef<Path>) -> Result<McpSessionConfig> {
    let path = home.as_ref().join("mcp").join("session.json");
    let mut session = if !path.exists() {
        default_mock_session()
    } else {
        let raw = std::fs::read_to_string(path).map_err(|e| McpError::Msg(e.to_string()))?;
        serde_json::from_str(&raw).map_err(|e| McpError::Msg(e.to_string()))?
    };
    let host_ceiling = optimus_packs::default_third_party_ceiling();
    session.max_policies.retain(|p| host_ceiling.contains(p));
    if session.max_policies.is_empty() {
        session.max_policies = host_ceiling;
    }
    Ok(session)
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
        assert!(mapped
            .iter()
            .all(|m| m.tool.invocation == ToolInvocation::Unavailable));
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
        assert!(assert_public_mcp_url("http://[::1]/mcp").is_err());
        assert!(assert_public_mcp_url("http://2130706433/").is_err());
        assert!(assert_public_mcp_url("http://foo.localhost/mcp").is_err());
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
        config
            .max_policies
            .retain(|p| *p != ToolPolicy::NetworkRead);
        // NetworkRead not in ceiling → mapping fails for allowlisted tools.
        let err = stdio_mock_bind(&config, &builtin_tool_id_set()).unwrap_err();
        assert!(matches!(err, McpError::Ceiling(_)));
    }
}
