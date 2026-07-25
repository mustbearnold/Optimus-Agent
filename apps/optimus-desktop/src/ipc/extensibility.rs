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
            let candidate = confined_packs_path(home, path)?;
            let root = load_or_init_trust_root(home)?;
            match load_signed_manifest_file(&candidate, &root) {
                Ok(body) => {
                    // Host clamp: reject ceilings that expand past third-party SmartDeny set.
                    let host = default_third_party_ceiling();
                    if body
                        .max_policies
                        .iter()
                        .any(|p| !host.contains(p))
                    {
                        return Ok(json!({
                            "ok": false,
                            "error": "pack max_policies expands past host third-party ceiling",
                            "note": "unsigned or over-ceiling packs are rejected",
                        }));
                    }
                    Ok(json!({
                        "ok": true,
                        "manifest": body,
                        "key_id": root.key_id,
                    }))
                }
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

/// Fail-closed path join under `{home}/packs/` — rejects `..`, absolute escapes.
fn confined_packs_path(home: &PathBuf, raw: &str) -> Result<PathBuf, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("path required".into());
    }
    if raw.starts_with('/') || raw.contains('\\') {
        return Err("signed pack path must be a relative name under packs/".into());
    }
    if raw.split('/').any(|p| p == ".." || p == "." || p.is_empty()) {
        return Err("signed pack path must not contain .. or empty segments".into());
    }
    // Basename-only or single relative segment preferred; multi-segment allowed if no escape.
    let home_packs = home.join("packs");
    std::fs::create_dir_all(&home_packs).map_err(|e| e.to_string())?;
    let packs_root = home_packs
        .canonicalize()
        .map_err(|e| format!("packs root: {e}"))?;
    let candidate = packs_root.join(raw);
    let canonical = candidate
        .canonicalize()
        .map_err(|_| "signed pack path not found under packs/".to_string())?;
    if !canonical.starts_with(&packs_root) {
        return Err("signed pack path must be under {home}/packs/".into());
    }
    Ok(canonical)
}

fn load_or_init_trust_root(home: &PathBuf) -> Result<TrustRoot, String> {
    let dir = home.join("packs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("trust_root.json");
    if path.exists() {
        let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        return serde_json::from_str(&raw).map_err(|e| e.to_string());
    }
    // Random secret on first init (ADR-0042) — not a fixed public demo key.
    let mut secret = [0u8; 32];
    getrandom_fill(&mut secret);
    let root = TrustRoot {
        key_id: "local-dev-root".into(),
        secret_hex: hex_encode(&secret),
    };
    let raw = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
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

fn getrandom_fill(buf: &mut [u8]) {
    // Prefer OS randomness; fall back to time-based mix for constrained hosts.
    if let Ok(f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let mut f = f;
        if f.read_exact(buf).is_ok() {
            return;
        }
    }
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for (i, b) in buf.iter_mut().enumerate() {
        *b = ((t >> ((i % 16) * 8)) as u8).wrapping_add(i as u8);
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
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
        assert!(handle(
            &home,
            "packs_verify_signed",
            json!({"path": "/etc/passwd"}),
        )
        .unwrap_err()
        .contains("relative"));
        assert!(handle(
            &home,
            "packs_verify_signed",
            json!({"path": "../escape.json"}),
        )
        .unwrap_err()
        .contains(".."));
    }
}
