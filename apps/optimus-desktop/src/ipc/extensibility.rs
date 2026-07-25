//! Program P27 extensibility IPC: provider catalog, failover preview, MCP, signed packs.

use std::path::PathBuf;

use optimus_kernel::{
    builtin_tool_id_set, default_mock_session, http_mock_bind, load_mcp_session,
    provider_catalog_status, resolve_route, stdio_mock_bind, ModelCapability, McpTransportKind,
    PrivacyPolicy, RouteRequest, RouteSurface,
};
use optimus_packs::{
    default_third_party_ceiling, load_signed_manifest_file, sign_manifest, PackManifestBody,
    TrustRoot,
};
use serde_json::json;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(
        method,
        "providers_catalog"
            | "providers_route_preview"
            | "mcp_status"
            | "mcp_tools"
            | "packs_verify_signed"
    )
}

pub(super) fn handle(
    home: &PathBuf,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "providers_catalog" => {
            let providers = provider_catalog_status(home);
            Ok(json!({
                "providers": providers,
                "note": "Connect state is local readiness only; failover never authorizes denied candidates.",
            }))
        }
        "providers_route_preview" => {
            let provider = params
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("offline");
            let model = params
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let allow_fallback = params
                .get("allow_fallback")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let fallback_order = params
                .get("fallback_order")
                .and_then(|v| v.as_array())
                .map(|rows| {
                    rows.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let privacy = match params.get("privacy").and_then(|v| v.as_str()).unwrap_or("remote") {
                "local" | "local_only" => PrivacyPolicy::LocalOnly,
                _ => PrivacyPolicy::RemoteAllowed,
            };
            let require_tools = params
                .get("require_tools")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let mut required = std::collections::BTreeSet::new();
            required.insert(ModelCapability::Text);
            if require_tools {
                required.insert(ModelCapability::Tools);
            }
            if params
                .get("require_vision")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                required.insert(ModelCapability::Vision);
            }
            let request = RouteRequest {
                surface: RouteSurface::Desktop,
                requested_provider: provider.into(),
                requested_model: model,
                required_capabilities: required,
                privacy,
                max_cost_microunits: None,
                allow_fallback,
                fallback_order,
                telemetry_policy: None,
            };
            match resolve_route(home, &request) {
                Ok(decision) => Ok(json!({
                    "ok": true,
                    "decision": decision,
                })),
                Err(e) => Ok(json!({
                    "ok": false,
                    "error": e.to_string(),
                })),
            }
        }
        "mcp_status" => {
            let session = load_mcp_session(home).unwrap_or_else(|_| default_mock_session());
            Ok(json!({
                "session": session,
                "note": "MCP maps to ToolDesc under pack allowlist; never a second tool catalog.",
            }))
        }
        "mcp_tools" => {
            let mut session = load_mcp_session(home).unwrap_or_else(|_| default_mock_session());
            if let Some(transport) = params.get("transport").and_then(|v| v.as_str()) {
                session.transport = match transport {
                    "http" => McpTransportKind::Http,
                    _ => McpTransportKind::Stdio,
                };
            }
            if let Some(url) = params.get("http_url").and_then(|v| v.as_str()) {
                session.http_url = Some(url.into());
            }
            let builtins = builtin_tool_id_set();
            let mapped = match session.transport {
                McpTransportKind::Stdio => stdio_mock_bind(&session, &builtins),
                McpTransportKind::Http => http_mock_bind(&session, &builtins),
            }
            .map_err(|e| e.to_string())?;
            let tools: Vec<_> = mapped
                .into_iter()
                .map(|m| {
                    json!({
                        "offer_name": m.offer_name,
                        "id": m.tool.id.as_str(),
                        "description": m.tool.description,
                        "policy": format!("{:?}", m.tool.policy),
                        "available": m.tool.is_available(),
                        "invocation": format!("{:?}", m.tool.invocation),
                    })
                })
                .collect();
            Ok(json!({
                "tools": tools,
                "count": tools.len(),
                "pack_id": session.pack_id,
                "transport": session.transport,
                "note": "Unavailable invocation until host effector registers under SmartDeny.",
            }))
        }
        "packs_verify_signed" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "path required".to_string())?;
            // Only allow paths under {home}/packs/ — no free-form absolute trust.
            let home_packs = home.join("packs");
            let candidate = if PathBuf::from(path).is_absolute() {
                PathBuf::from(path)
            } else {
                home_packs.join(path)
            };
            let canonical = candidate
                .canonicalize()
                .unwrap_or(candidate.clone());
            let packs_root = home_packs.canonicalize().unwrap_or(home_packs.clone());
            if !canonical.starts_with(&packs_root) {
                return Err("signed pack path must be under {home}/packs/".into());
            }
            let root = load_or_init_trust_root(home)?;
            match load_signed_manifest_file(&canonical, &root) {
                Ok(body) => Ok(json!({
                    "ok": true,
                    "manifest": body,
                    "key_id": root.key_id,
                })),
                Err(e) => Ok(json!({
                    "ok": false,
                    "error": e.to_string(),
                    "note": "unsigned packs are rejected by default",
                })),
            }
        }
        _ => Err(format!("unknown method: {method}")),
    }
}

fn load_or_init_trust_root(home: &PathBuf) -> Result<TrustRoot, String> {
    let dir = home.join("packs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("trust_root.json");
    if path.exists() {
        let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        return serde_json::from_str(&raw).map_err(|e| e.to_string());
    }
    // Dev trust root — operators replace for production rotation (ADR-0042).
    let root = TrustRoot {
        key_id: "local-dev-root".into(),
        secret_hex: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".into(),
    };
    let raw = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| e.to_string())?;
    // Seed a sample signed pack for console verification demos.
    let body = PackManifestBody {
        pack_id: "example.signed".into(),
        version: "1.0.0".into(),
        tool_ids: vec!["mcp_echo".into()],
        max_policies: default_third_party_ceiling(),
    };
    if let Ok(signed) = sign_manifest(&root, &body) {
        let _ = std::fs::write(
            dir.join("example.signed.json"),
            serde_json::to_string_pretty(&signed).unwrap_or_default(),
        );
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn providers_catalog_and_failover_preview() {
        let dir = tempdir().unwrap();
        let home = dir.path().to_path_buf();
        let cat = handle(&home, "providers_catalog", json!({})).unwrap();
        assert!(cat["providers"].as_array().unwrap().len() >= 3);
        let preview = handle(
            &home,
            "providers_route_preview",
            json!({
                "provider": "codex",
                "model": "not-real",
                "allow_fallback": true,
                "fallback_order": ["offline"],
            }),
        )
        .unwrap();
        assert_eq!(preview["ok"], true);
        assert_eq!(preview["decision"]["provider"], "offline");
    }

    #[test]
    fn mcp_stdio_tools_and_signed_pack() {
        let dir = tempdir().unwrap();
        let home = dir.path().to_path_buf();
        let tools = handle(&home, "mcp_tools", json!({"transport": "stdio"})).unwrap();
        assert!(tools["count"].as_u64().unwrap() >= 1);
        // Init trust root + sample pack via verify path creation.
        let _ = load_or_init_trust_root(&home).unwrap();
        let verified = handle(
            &home,
            "packs_verify_signed",
            json!({"path": "example.signed.json"}),
        )
        .unwrap();
        assert_eq!(verified["ok"], true);
        let bad = handle(
            &home,
            "packs_verify_signed",
            json!({"path": "/etc/passwd"}),
        )
        .unwrap_err();
        assert!(bad.contains("packs"));
    }
}
