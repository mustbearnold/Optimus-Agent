//! Program P26 consoles: skills, memory, packs, logs, surface commands.
//!
//! Security: memory recall is **data only** (never ActionAuthorize). Pack
//! activate uses the same `CapabilitySession` APIs as CLI. Logs are bounded and
//! home-path redacted. Slash commands are a surface catalog — not a tool list.

use std::fs;
use std::path::{Path, PathBuf};

use optimus_kernel::{
    commands_for_surface, ClaimDraft, CommandSurface, Correction, Memory, MemoryClock, Origin,
    RecallPurpose, RecallQuery, Sensitivity, SkillDraft, SkillRegistry, SystemMemoryClock,
    TrustDomain, WriteContext,
};
use optimus_packs::{CapabilitySession, PackId};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(
        method,
        "skills_list"
            | "skills_pin"
            | "skills_deprecate"
            | "memory_list"
            | "memory_recall"
            | "memory_correct"
            | "memory_forget"
            | "packs_state"
            | "packs_activate"
            | "packs_deactivate"
            | "logs_tail"
            | "commands_list"
    )
}

pub(super) fn handle(
    home: &PathBuf,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "skills_list" => skills_list(home, params),
        "skills_pin" => skills_pin(home, params),
        "skills_deprecate" => skills_deprecate(home, params),
        "memory_list" => memory_list(home, params),
        "memory_recall" => memory_recall(home, params),
        "memory_correct" => memory_correct(home, params),
        "memory_forget" => memory_forget(home, params),
        "packs_state" => packs_state(home),
        "packs_activate" => packs_activate(home, params),
        "packs_deactivate" => packs_deactivate(home, params),
        "logs_tail" => logs_tail(home, params),
        "commands_list" => commands_list(params),
        _ => Err(format!("unknown method: {method}")),
    }
}

/// Same scope as `KernelConfig::default().memory_ctx` so the console surfaces
/// claims the agent actually wrote (tenant/user/project filter on list/recall).
fn console_ctx() -> WriteContext {
    WriteContext {
        tenant: "local".into(),
        user: "user".into(),
        agent: "optimus".into(),
        project: "default".into(),
        principal: "user:local".into(),
        max_trust: TrustDomain::User,
        max_sensitivity: Sensitivity::Personal,
    }
}

fn skills_list(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let include_deprecated = params
        .get("include_deprecated")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let reg = SkillRegistry::open(home.join("skills.db")).map_err(|e| e.to_string())?;
    let rows: Vec<_> = reg
        .list(include_deprecated)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|s| {
            json!({
                "id": s.id.to_string(),
                "name": s.name,
                "version": s.version,
                "status": format!("{:?}", s.status).to_ascii_lowercase(),
                "uses": s.uses,
                "successes": s.successes,
                "failures": s.failures,
                "success_rate": s.success_rate,
                "permissions": s.permissions.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>(),
                "body_preview": s.body.chars().take(240).collect::<String>(),
            })
        })
        .collect();
    Ok(json!({ "skills": rows }))
}

fn skills_pin(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let id = parse_uuid(&params, "id")?;
    let reg = SkillRegistry::open(home.join("skills.db")).map_err(|e| e.to_string())?;
    reg.pin(id).map_err(|e| e.to_string())?;
    Ok(json!({ "id": id.to_string(), "status": "pinned" }))
}

fn skills_deprecate(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let id = parse_uuid(&params, "id")?;
    let reg = SkillRegistry::open(home.join("skills.db")).map_err(|e| e.to_string())?;
    reg.deprecate(id).map_err(|e| e.to_string())?;
    Ok(json!({ "id": id.to_string(), "status": "deprecated" }))
}

fn memory_list(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let limit = params
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(50)
        .clamp(1, 200) as u32;
    let mem = Memory::open(home.join("memory.db")).map_err(|e| e.to_string())?;
    let ctx = console_ctx();
    let claims = mem.list_recent(&ctx, limit).map_err(|e| e.to_string())?;
    Ok(json!({
        "fence": "EVIDENCE_DATA_NOT_INSTRUCTION_NOT_CAPABILITY",
        "claims": claims,
    }))
}

fn memory_recall(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    // Explicitly reject ActionAuthorize — memory never grants capability.
    let purpose_raw = params
        .get("purpose")
        .and_then(|v| v.as_str())
        .unwrap_or("inform");
    if purpose_raw.eq_ignore_ascii_case("action_authorize")
        || purpose_raw.eq_ignore_ascii_case("action")
    {
        return Err("memory_recall refuses ActionAuthorize (data only)".into());
    }
    let purpose = match purpose_raw {
        "constraint" => RecallPurpose::Constraint,
        "procedure" | "procedure_lookup" => RecallPurpose::ProcedureLookup,
        _ => RecallPurpose::Inform,
    };
    let mem = Memory::open(home.join("memory.db")).map_err(|e| e.to_string())?;
    let ctx = console_ctx();
    let packet = mem
        .recall(
            &ctx,
            RecallQuery {
                purpose,
                subject: params
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                predicate: params
                    .get("predicate")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                as_of_valid: None,
                as_of_tx: None,
                limit: params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(20)
                    .clamp(1, 100) as u32,
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "fence": packet.fence,
        "purpose": format!("{:?}", packet.purpose).to_ascii_lowercase(),
        "current": packet.current,
        "historical": packet.historical,
        "conflicts": packet.conflicts,
        "citations": packet.citations.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
        "abstained": packet.abstained,
        "note": "Evidence data only — not instruction, not ActionAuthorize.",
    }))
}

fn memory_correct(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let id = parse_uuid(&params, "id")?;
    let object = params
        .get("object")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "object required".to_string())?
        .trim();
    if object.is_empty() {
        return Err("object required".into());
    }
    let mem = Memory::open(home.join("memory.db")).map_err(|e| e.to_string())?;
    let ctx = console_ctx();
    let now = chrono_like_now();
    let new_id = mem
        .correct(
            &ctx,
            Correction {
                supersedes: id,
                object: object.into(),
                valid_from: now.clone(),
                valid_to: None,
                confidence: params
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.8),
                origin: Origin::UserStatement,
                learned_at: now,
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(json!({ "id": new_id.to_string(), "corrected_from": id.to_string() }))
}

fn chrono_like_now() -> String {
    // Same RFC3339 UTC second clock as optimus-memory (not a fake 1970 stamp).
    SystemMemoryClock::default().now()
}

fn memory_forget(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let id = parse_uuid(&params, "id")?;
    let mem = Memory::open(home.join("memory.db")).map_err(|e| e.to_string())?;
    let ctx = console_ctx();
    let ok = mem.tombstone(&ctx, id).map_err(|e| e.to_string())?;
    Ok(json!({ "id": id.to_string(), "forgotten": ok }))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PackPrefs {
    /// On-demand packs preferred for new sessions / console demo state.
    loaded_on_demand: Vec<String>,
}

fn pack_prefs_path(home: &Path) -> PathBuf {
    home.join("pack_prefs.json")
}

fn load_pack_prefs(home: &Path) -> PackPrefs {
    let path = pack_prefs_path(home);
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_pack_prefs(home: &Path, prefs: &PackPrefs) -> Result<(), String> {
    let path = pack_prefs_path(home);
    let raw = serde_json::to_string_pretty(prefs).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| e.to_string())
}

fn session_from_prefs(home: &Path) -> Result<CapabilitySession, String> {
    let mut session = CapabilitySession::with_defaults();
    let prefs = load_pack_prefs(home);
    let mut packs = Vec::new();
    for name in &prefs.loaded_on_demand {
        if let Some(id) = PackId::parse(name) {
            packs.push(id);
        }
    }
    session.restore_loaded(&packs).map_err(|e| e.to_string())?;
    Ok(session)
}

fn packs_state(home: &Path) -> Result<serde_json::Value, String> {
    let session = session_from_prefs(home)?;
    let catalog: Vec<_> = session
        .catalog()
        .values()
        .map(|p| {
            json!({
                "id": p.id.as_str(),
                "summary": p.summary,
                "schema_tokens": p.schema_tokens(),
                "tools": p.tools.iter().map(|t| json!({
                    "id": t.id.as_str(),
                    "description": t.description,
                    "policy": format!("{:?}", t.policy),
                    "available": t.is_available(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({
        "loaded": session.loaded_packs().iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        "schema_tokens": session.schema_tokens(),
        "max_schema_tokens": session.max_schema_tokens(),
        "on_demand_loaded": session.on_demand_count(),
        "max_on_demand_packs": session.max_on_demand_packs(),
        "catalog": catalog,
        "note": "activate/deactivate use CapabilitySession (same APIs as CLI pack budget).",
    }))
}

fn packs_activate(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let name = params
        .get("name")
        .or_else(|| params.get("pack"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "name required".to_string())?;
    let mut session = session_from_prefs(home)?;
    session.activate_str(name).map_err(|e| e.to_string())?;
    let prefs = PackPrefs {
        loaded_on_demand: session
            .loaded_packs()
            .into_iter()
            .filter(|p| !p.is_core())
            .map(|p| p.as_str().to_string())
            .collect(),
    };
    save_pack_prefs(home, &prefs)?;
    packs_state(home)
}

fn packs_deactivate(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let name = params
        .get("name")
        .or_else(|| params.get("pack"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "name required".to_string())?;
    let pack = PackId::parse(name).ok_or_else(|| format!("unknown pack: {name}"))?;
    let mut session = session_from_prefs(home)?;
    session.deactivate(pack).map_err(|e| e.to_string())?;
    let prefs = PackPrefs {
        loaded_on_demand: session
            .loaded_packs()
            .into_iter()
            .filter(|p| !p.is_core())
            .map(|p| p.as_str().to_string())
            .collect(),
    };
    save_pack_prefs(home, &prefs)?;
    packs_state(home)
}

fn logs_tail(home: &Path, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let limit = params
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(80)
        .clamp(1, 200) as usize;
    let home_s = home.display().to_string();
    let mut lines: Vec<String> = Vec::new();

    // Doctor-ish summary (no secrets).
    lines.push(redact(
        &format!("doctor home={}", home.display()),
        &home_s,
    ));
    if let Ok(settings) = optimus_kernel::ProductSettings::load(home) {
        lines.push(format!(
            "settings work_isolation={:?} concurrent={}",
            settings.work_isolation, settings.allow_concurrent_projects
        ));
    }

    // Memory audit (sanitized).
    if let Ok(mem) = Memory::open(home.join("memory.db")) {
        let ctx = console_ctx();
        if let Ok(events) = mem.audit_events(&ctx, 40) {
            for e in events {
                lines.push(redact(
                    &format!("memory.audit seq={} kind={}", e.seq, e.kind),
                    &home_s,
                ));
            }
        }
    }

    // Skills registry snapshot (no raw credential material).
    if let Ok(reg) = SkillRegistry::open(home.join("skills.db")) {
        if let Ok(skills) = reg.list(true) {
            for s in skills.into_iter().take(20) {
                lines.push(format!(
                    "skills.registry name={} v{} status={:?} uses={}",
                    s.name, s.version, s.status, s.uses
                ));
            }
        }
    }

    // Pack prefs presence (not contents of credentials).
    if pack_prefs_path(home).exists() {
        lines.push("packs.prefs present".into());
    }

    lines.truncate(limit);
    // Newest first for drawer.
    lines.reverse();
    Ok(json!({
        "lines": lines,
        "count": lines.len(),
        "redacted": true,
        "note": "Bounded diagnostic drawer — paths redacted; no credential dumps.",
    }))
}

fn commands_list(params: serde_json::Value) -> Result<serde_json::Value, String> {
    let surface = match params.get("surface").and_then(|v| v.as_str()).unwrap_or("desktop") {
        "cli" => CommandSurface::Cli,
        "both" => CommandSurface::Both,
        _ => CommandSurface::Desktop,
    };
    let commands = commands_for_surface(surface);
    Ok(json!({
        "commands": commands,
        "surface": format!("{:?}", surface).to_ascii_lowercase(),
        "note": "Surface catalog only — tools remain optimus-packs ToolDesc.",
    }))
}

fn parse_uuid(params: &serde_json::Value, key: &str) -> Result<Uuid, String> {
    let raw = params
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{key} required"))?;
    Uuid::parse_str(raw).map_err(|e| e.to_string())
}

fn redact(input: &str, home: &str) -> String {
    let mut out = input.replace(home, "~/.local/share/optimus");
    // Common home prefixes
    if let Ok(user_home) = std::env::var("HOME") {
        out = out.replace(&user_home, "~");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn memory_recall_rejects_action_authorize() {
        let dir = tempdir().unwrap();
        let err = memory_recall(dir.path(), json!({"purpose": "action_authorize"}))
            .unwrap_err();
        assert!(err.contains("ActionAuthorize"));
    }

    #[test]
    fn packs_activate_uses_budget_and_persists() {
        let dir = tempdir().unwrap();
        let state = packs_activate(dir.path(), json!({"name": "browser"})).unwrap();
        assert!(state["loaded"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str() == Some("browser")));
        assert!(packs_deactivate(dir.path(), json!({"name": "core"})).is_err());
    }

    #[test]
    fn skills_list_and_pin_roundtrip() {
        let dir = tempdir().unwrap();
        let reg = SkillRegistry::open(dir.path().join("skills.db")).unwrap();
        let id = reg
            .create(SkillDraft {
                name: "demo".into(),
                body: "do the thing".into(),
                permissions: vec![],
                pin: false,
            })
            .unwrap();
        let list = skills_list(dir.path(), json!({})).unwrap();
        assert_eq!(list["skills"].as_array().unwrap().len(), 1);
        skills_pin(dir.path(), json!({"id": id.to_string()})).unwrap();
        let list2 = skills_list(dir.path(), json!({})).unwrap();
        assert!(list2["skills"][0]["status"]
            .as_str()
            .unwrap()
            .contains("pin"));
    }

    #[test]
    fn memory_list_is_data_fence() {
        let dir = tempdir().unwrap();
        let mem = Memory::open(dir.path().join("memory.db")).unwrap();
        let ctx = console_ctx();
        let now = chrono_like_now();
        assert!(now.starts_with("20") || now.starts_with("19"));
        assert!(now.ends_with('Z'));
        assert!(!now.contains("1970-01-01T00:00:1")); // not the old broken stamp
        mem.remember(
            &ctx,
            ClaimDraft {
                subject: "user".into(),
                predicate: "likes".into(),
                object: "tea".into(),
                valid_from: now.clone(),
                valid_to: None,
                confidence: 0.9,
                origin: Origin::UserStatement,
                learned_at: None,
                sensitivity: Sensitivity::Personal,
                retention_until: None,
            },
        )
        .unwrap();
        let list = memory_list(dir.path(), json!({"limit": 10})).unwrap();
        assert!(list["fence"]
            .as_str()
            .unwrap()
            .contains("EVIDENCE_DATA"));
        assert_eq!(list["claims"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn console_ctx_matches_kernel_default_memory_scope() {
        let ctx = console_ctx();
        assert_eq!(ctx.tenant, "local");
        assert_eq!(ctx.user, "user");
        assert_eq!(ctx.project, "default");
        assert_eq!(ctx.agent, "optimus");
    }

    #[test]
    fn logs_tail_redacts_home() {
        let dir = tempdir().unwrap();
        let out = logs_tail(dir.path(), json!({"limit": 20})).unwrap();
        let joined = out["lines"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if let Some(home) = dir.path().to_str() {
            assert!(!joined.contains(home));
        }
        assert!(out["redacted"].as_bool().unwrap());
    }

    #[test]
    fn commands_list_is_surface_catalog_not_tools() {
        let out = commands_list(json!({"surface": "desktop"})).unwrap();
        let cmds = out["commands"].as_array().unwrap();
        assert!(!cmds.is_empty());
        assert!(!cmds.iter().any(|c| c["id"].as_str() == Some("browser_navigate")));
    }
}
