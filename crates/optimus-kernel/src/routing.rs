//! Canonical provider/model routing policy and append-only decision ledger.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{KernelError, Result};

pub const CODEX_MODEL_CATALOG: &[(&str, &str)] = &[
    ("gpt-5.6-sol", "GPT-5.6 Sol"),
    ("gpt-5.6-terra", "GPT-5.6 Terra"),
    ("gpt-5.6-luna", "GPT-5.6 Luna"),
    ("gpt-5.5", "GPT-5.5"),
    ("gpt-5.4", "GPT-5.4"),
    ("gpt-5.4-mini", "GPT-5.4 Mini"),
    ("gpt-5.3-codex-spark", "GPT-5.3 Codex Spark"),
];
pub const DEFAULT_CODEX_MODEL: &str = "gpt-5.6-terra";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderId {
    Offline,
    Codex,
    OpenAiCompat,
}

impl ProviderId {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "offline" => Some(Self::Offline),
            "codex" | "codex-oauth" => Some(Self::Codex),
            "openai" | "openai-compat" => Some(Self::OpenAiCompat),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Codex => "codex",
            Self::OpenAiCompat => "openai-compat",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModelId(String);

impl ModelId {
    pub fn parse(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 128
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/' | b':')
            });
        if !valid {
            return Err(KernelError::Model(
                "invalid canonical model identity".into(),
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteSurface {
    Cli,
    Desktop,
    Cron,
    Gateway,
}

impl RouteSurface {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Desktop => "desktop",
            Self::Cron => "cron",
            Self::Gateway => "gateway",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
    Text,
    Tools,
    Streaming,
    Reasoning,
    Local,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyPolicy {
    LocalOnly,
    RemoteAllowed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderDescriptor {
    pub id: ProviderId,
    pub default_model: ModelId,
    pub models: Vec<ModelId>,
    pub capabilities: BTreeSet<ModelCapability>,
    pub remote: bool,
    pub estimated_cost_microunits: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteRequest {
    pub surface: RouteSurface,
    pub requested_provider: String,
    pub requested_model: Option<String>,
    pub required_capabilities: BTreeSet<ModelCapability>,
    pub privacy: PrivacyPolicy,
    pub max_cost_microunits: Option<u64>,
    pub allow_fallback: bool,
}

impl RouteRequest {
    pub fn standard(
        surface: RouteSurface,
        provider: impl Into<String>,
        model: Option<String>,
    ) -> Self {
        Self {
            surface,
            requested_provider: provider.into(),
            requested_model: model,
            required_capabilities: [ModelCapability::Text, ModelCapability::Tools]
                .into_iter()
                .collect(),
            privacy: PrivacyPolicy::RemoteAllowed,
            max_cost_microunits: None,
            allow_fallback: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteDecision {
    pub id: Uuid,
    pub surface: RouteSurface,
    pub provider: ProviderId,
    pub model: ModelId,
    pub fallback_from: Option<ProviderId>,
    pub reasons: Vec<String>,
    pub created_unix: u64,
}

pub fn provider_catalog() -> Vec<ProviderDescriptor> {
    vec![
        ProviderDescriptor {
            id: ProviderId::Offline,
            default_model: ModelId("offline-scripted".into()),
            models: vec![ModelId("offline-scripted".into())],
            capabilities: [
                ModelCapability::Text,
                ModelCapability::Tools,
                ModelCapability::Local,
            ]
            .into_iter()
            .collect(),
            remote: false,
            estimated_cost_microunits: 0,
        },
        ProviderDescriptor {
            id: ProviderId::Codex,
            default_model: ModelId(DEFAULT_CODEX_MODEL.into()),
            models: CODEX_MODEL_CATALOG
                .iter()
                .map(|(id, _)| ModelId((*id).into()))
                .collect(),
            capabilities: [
                ModelCapability::Text,
                ModelCapability::Tools,
                ModelCapability::Streaming,
                ModelCapability::Reasoning,
            ]
            .into_iter()
            .collect(),
            remote: true,
            estimated_cost_microunits: 10,
        },
        ProviderDescriptor {
            id: ProviderId::OpenAiCompat,
            default_model: ModelId("gpt-4.1".into()),
            models: vec![],
            capabilities: [
                ModelCapability::Text,
                ModelCapability::Tools,
                ModelCapability::Streaming,
            ]
            .into_iter()
            .collect(),
            remote: true,
            estimated_cost_microunits: 10,
        },
    ]
}

pub fn is_known_codex_model(model: &str) -> bool {
    CODEX_MODEL_CATALOG.iter().any(|(id, _)| *id == model)
}

pub fn sanitize_codex_oauth_model(model: &str) -> String {
    let model = model.trim();
    if is_known_codex_model(model) {
        return model.into();
    }
    match model {
        "gpt-5.6" | "sol" => "gpt-5.6-sol".into(),
        "terra" => "gpt-5.6-terra".into(),
        "luna" => "gpt-5.6-luna".into(),
        _ => DEFAULT_CODEX_MODEL.into(),
    }
}

pub fn resolve_route(home: impl AsRef<Path>, request: &RouteRequest) -> Result<RouteDecision> {
    let requested = ProviderId::parse(&request.requested_provider).ok_or_else(|| {
        KernelError::Model(format!(
            "unknown provider identity: {}",
            request.requested_provider
        ))
    })?;
    let catalog = provider_catalog();
    let requested_descriptor = catalog
        .iter()
        .find(|descriptor| descriptor.id == requested)
        .expect("canonical provider is present in catalog");
    let mut candidates = vec![requested_descriptor];
    if request.allow_fallback {
        candidates.extend(
            catalog
                .iter()
                .filter(|descriptor| descriptor.id != requested),
        );
    }
    let mut rejected = Vec::new();
    for descriptor in candidates {
        match evaluate_candidate(descriptor, request) {
            Ok(model) => {
                let decision = RouteDecision {
                    id: Uuid::new_v4(),
                    surface: request.surface,
                    provider: descriptor.id,
                    model,
                    fallback_from: (descriptor.id != requested).then_some(requested),
                    reasons: if descriptor.id == requested {
                        vec!["requested provider satisfies policy".into()]
                    } else {
                        vec!["explicit bounded fallback satisfies policy".into()]
                    },
                    created_unix: now_unix(),
                };
                persist_decision(home, request, &decision)?;
                return Ok(decision);
            }
            Err(reason) => rejected.push(format!("{}:{reason}", descriptor.id.as_str())),
        }
    }
    Err(KernelError::Model(format!(
        "no policy-approved provider route: {}",
        rejected.join(",")
    )))
}

fn evaluate_candidate(
    descriptor: &ProviderDescriptor,
    request: &RouteRequest,
) -> std::result::Result<ModelId, String> {
    if request.privacy == PrivacyPolicy::LocalOnly && descriptor.remote {
        return Err("privacy_requires_local".into());
    }
    if !request
        .required_capabilities
        .is_subset(&descriptor.capabilities)
    {
        return Err("missing_capability".into());
    }
    if request
        .max_cost_microunits
        .is_some_and(|budget| descriptor.estimated_cost_microunits > budget)
    {
        return Err("budget_exceeded".into());
    }
    let model = request
        .requested_model
        .as_deref()
        .map(ModelId::parse)
        .transpose()
        .map_err(|_| "invalid_model_identity".to_string())?
        .unwrap_or_else(|| descriptor.default_model.clone());
    if descriptor.id == ProviderId::Codex && !descriptor.models.contains(&model) {
        return Err("model_not_owned_by_provider".into());
    }
    if descriptor.id == ProviderId::Offline && !descriptor.models.contains(&model) {
        return Err("model_not_owned_by_provider".into());
    }
    Ok(model)
}

fn persist_decision(
    home: impl AsRef<Path>,
    request: &RouteRequest,
    decision: &RouteDecision,
) -> Result<()> {
    std::fs::create_dir_all(home.as_ref())?;
    let connection = Connection::open(home.as_ref().join("routing.db"))?;
    connection.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS route_decisions(
           id TEXT PRIMARY KEY,surface TEXT NOT NULL,requested_provider TEXT NOT NULL,
           selected_provider TEXT NOT NULL,selected_model TEXT NOT NULL,
           fallback_from TEXT,reasons_json TEXT NOT NULL,request_json TEXT NOT NULL,
           created_unix INTEGER NOT NULL
         );",
    )?;
    connection.execute(
        "INSERT INTO route_decisions(
           id,surface,requested_provider,selected_provider,selected_model,fallback_from,
           reasons_json,request_json,created_unix
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            decision.id.to_string(),
            decision.surface.as_str(),
            request.requested_provider,
            decision.provider.as_str(),
            decision.model.as_str(),
            decision.fallback_from.map(ProviderId::as_str),
            serde_json::to_string(&decision.reasons)?,
            serde_json::to_string(request)?,
            decision.created_unix as i64
        ],
    )?;
    Ok(())
}

pub fn route_decision_count(home: impl AsRef<Path>) -> Result<usize> {
    let path = home.as_ref().join("routing.db");
    if !path.exists() {
        return Ok(0);
    }
    let connection = Connection::open(path)?;
    connection
        .query_row("SELECT count(*) FROM route_decisions", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|count| count as usize)
        .map_err(KernelError::Sqlite)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn provider_aliases_resolve_to_canonical_identity() {
        assert_eq!(ProviderId::parse("openai"), Some(ProviderId::OpenAiCompat));
        assert_eq!(ProviderId::parse("codex-oauth"), Some(ProviderId::Codex));
        assert!(ModelId::parse("bad model with spaces").is_err());
    }

    #[test]
    fn privacy_capability_and_budget_constraints_fail_closed() {
        let directory = tempdir().unwrap();
        let mut request = RouteRequest::standard(RouteSurface::Desktop, "codex", None);
        request.privacy = PrivacyPolicy::LocalOnly;
        assert!(resolve_route(directory.path(), &request).is_err());
        request.privacy = PrivacyPolicy::RemoteAllowed;
        request.required_capabilities.insert(ModelCapability::Local);
        assert!(resolve_route(directory.path(), &request).is_err());
        request
            .required_capabilities
            .remove(&ModelCapability::Local);
        request.max_cost_microunits = Some(0);
        assert!(resolve_route(directory.path(), &request).is_err());
        assert_eq!(route_decision_count(directory.path()).unwrap(), 0);
    }

    #[test]
    fn explicit_fallback_is_bounded_policy_checked_and_recorded() {
        let directory = tempdir().unwrap();
        let mut request = RouteRequest::standard(RouteSurface::Gateway, "codex", None);
        request.privacy = PrivacyPolicy::LocalOnly;
        request.allow_fallback = true;
        let decision = resolve_route(directory.path(), &request).unwrap();
        assert_eq!(decision.provider, ProviderId::Offline);
        assert_eq!(decision.fallback_from, Some(ProviderId::Codex));
        assert_eq!(route_decision_count(directory.path()).unwrap(), 1);
    }

    #[test]
    fn model_ownership_is_enforced_for_codex() {
        let directory = tempdir().unwrap();
        let request = RouteRequest::standard(RouteSurface::Cli, "codex", Some("grok-4.5".into()));
        assert!(resolve_route(directory.path(), &request).is_err());
    }
}
