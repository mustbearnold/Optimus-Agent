//! Canonical provider/model routing policy and append-only decision ledger.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::telemetry::{route_telemetry_aggregate, RouteTelemetryAggregate};
use crate::{KernelError, Result, TraceContext};

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
            // `open-ai-compat` is the serde kebab-case form that
            // `providers_catalog` puts on the wire; a surface must be able to
            // send back the id it was given.
            "openai" | "openai_compat" | "openai-compat" | "open-ai-compat" => {
                Some(Self::OpenAiCompat)
            }
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
#[serde(transparent)]
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
    /// Multimodal / image inputs (UI may disable when absent).
    Vision,
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
    /// Ordered provider ids tried after the requested provider when `allow_fallback`.
    /// Unknown ids are skipped. Statically denied candidates never authorize.
    #[serde(default)]
    pub fallback_order: Vec<String>,
    pub telemetry_policy: Option<RouteTelemetryPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteTelemetryPolicy {
    pub evaluated_unix: u64,
    pub max_age_seconds: u64,
    pub min_samples: usize,
    pub min_success_basis_points: u16,
    pub max_p95_latency_millis: u64,
    pub allow_missing: bool,
}

impl RouteTelemetryPolicy {
    fn validate(&self) -> Result<()> {
        if self.evaluated_unix == 0
            || self.max_age_seconds == 0
            || self.min_samples == 0
            || self.min_samples > crate::MAX_TELEMETRY_SAMPLES
            || self.min_success_basis_points > 10_000
            || self.max_p95_latency_millis == 0
        {
            return Err(KernelError::Model(
                "invalid bounded route telemetry policy".into(),
            ));
        }
        Ok(())
    }
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
            fallback_order: Vec::new(),
            telemetry_policy: None,
        }
    }
}

/// Connect / readiness state for UI and failover honesty (program P27).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderConnectState {
    Connected,
    Disconnected,
    Unknown,
}

/// Catalog row with connect state and capability flags for UI (program P27.a).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCatalogStatus {
    pub id: ProviderId,
    pub default_model: ModelId,
    pub models: Vec<ModelId>,
    pub capabilities: BTreeSet<ModelCapability>,
    pub remote: bool,
    pub estimated_cost_microunits: u64,
    pub connect: ProviderConnectState,
    pub connect_detail: String,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_streaming: bool,
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
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
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
                ModelCapability::Vision,
            ]
            .into_iter()
            .collect(),
            remote: true,
            estimated_cost_microunits: 10,
        },
        ProviderDescriptor {
            id: ProviderId::OpenAiCompat,
            default_model: ModelId("gpt-4.1".into()),
            models: vec![ModelId("gpt-4.1".into()), ModelId("gpt-4o".into())],
            capabilities: [
                ModelCapability::Text,
                ModelCapability::Tools,
                ModelCapability::Streaming,
                ModelCapability::Vision,
            ]
            .into_iter()
            .collect(),
            remote: true,
            estimated_cost_microunits: 10,
        },
    ]
}

/// Live catalog with connect state (Codex credentials, openai-compat env, offline always up).
pub fn provider_catalog_status(home: impl AsRef<Path>) -> Vec<ProviderCatalogStatus> {
    let home = home.as_ref();
    let codex_status = crate::CodexAuthStore::open(home)
        .ok()
        .and_then(|store| store.status().ok());
    let openai_key = std::env::var("OPTIMUS_API_KEY")
        .ok()
        .filter(|v| !v.trim().is_empty());
    provider_catalog()
        .into_iter()
        .map(|descriptor| {
            let (connect, connect_detail) = match descriptor.id {
                ProviderId::Offline => (
                    ProviderConnectState::Connected,
                    "local scripted provider".into(),
                ),
                ProviderId::Codex => codex_connection(codex_status.as_ref()),
                ProviderId::OpenAiCompat => {
                    if openai_key.is_some() {
                        (
                            ProviderConnectState::Connected,
                            "OPTIMUS_API_KEY present".into(),
                        )
                    } else {
                        (
                            ProviderConnectState::Disconnected,
                            "OPTIMUS_API_KEY unset".into(),
                        )
                    }
                }
            };
            ProviderCatalogStatus {
                supports_tools: descriptor.capabilities.contains(&ModelCapability::Tools),
                supports_vision: descriptor.capabilities.contains(&ModelCapability::Vision),
                supports_streaming: descriptor
                    .capabilities
                    .contains(&ModelCapability::Streaming),
                id: descriptor.id,
                default_model: descriptor.default_model,
                models: descriptor.models,
                capabilities: descriptor.capabilities,
                remote: descriptor.remote,
                estimated_cost_microunits: descriptor.estimated_cost_microunits,
                connect,
                connect_detail,
            }
        })
        .collect()
}

fn codex_connection(status: Option<&crate::CodexAuthStatus>) -> (ProviderConnectState, String) {
    match status {
        Some(status) if status.present && (!status.access_expiring || status.has_refresh) => (
            ProviderConnectState::Connected,
            "codex credentials usable".into(),
        ),
        Some(status) if status.present && status.access_expiring && !status.has_refresh => (
            ProviderConnectState::Disconnected,
            "codex access token expiring without refresh token".into(),
        ),
        _ => (
            ProviderConnectState::Disconnected,
            "no codex credentials".into(),
        ),
    }
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

fn automatic_provider_order(statuses: &[ProviderCatalogStatus]) -> Vec<ProviderId> {
    [
        ProviderId::Codex,
        ProviderId::OpenAiCompat,
        ProviderId::Offline,
    ]
    .into_iter()
    .filter(|id| {
        statuses
            .iter()
            .any(|status| status.id == *id && status.connect == ProviderConnectState::Connected)
    })
    .collect()
}

pub fn resolve_route(home: impl AsRef<Path>, request: &RouteRequest) -> Result<RouteDecision> {
    resolve_route_internal(home, request, None)
}

pub fn resolve_route_traced(
    home: impl AsRef<Path>,
    request: &RouteRequest,
    trace: TraceContext,
) -> Result<RouteDecision> {
    resolve_route_internal(home, request, Some(trace))
}

fn resolve_route_internal(
    home: impl AsRef<Path>,
    request: &RouteRequest,
    trace: Option<TraceContext>,
) -> Result<RouteDecision> {
    let home = home.as_ref();
    let automatic = request
        .requested_provider
        .trim()
        .eq_ignore_ascii_case("auto");
    if automatic && request.requested_model.is_some() {
        return Err(KernelError::Model(
            "automatic model selection is represented by an absent requested model".into(),
        ));
    }
    let requested = if automatic {
        None
    } else {
        Some(
            ProviderId::parse(&request.requested_provider).ok_or_else(|| {
                KernelError::Model(format!(
                    "unknown provider identity: {}",
                    request.requested_provider
                ))
            })?,
        )
    };
    let catalog = provider_catalog();
    let mut candidates = Vec::new();
    if automatic {
        let statuses = provider_catalog_status(home);
        for id in automatic_provider_order(&statuses) {
            if let Some(descriptor) = catalog.iter().find(|descriptor| descriptor.id == id) {
                candidates.push(descriptor);
            }
        }
    } else {
        let requested = requested.expect("explicit provider was parsed");
        let requested_descriptor = catalog
            .iter()
            .find(|descriptor| descriptor.id == requested)
            .expect("canonical provider is present in catalog");
        candidates.push(requested_descriptor);
        if request.allow_fallback {
            if !request.fallback_order.is_empty() {
                for raw in &request.fallback_order {
                    let Some(id) = ProviderId::parse(raw) else {
                        continue;
                    };
                    if id == requested {
                        continue;
                    }
                    if let Some(descriptor) = catalog.iter().find(|d| d.id == id) {
                        if !candidates.iter().any(|c| c.id == descriptor.id) {
                            candidates.push(descriptor);
                        }
                    }
                }
            } else {
                candidates.extend(
                    catalog
                        .iter()
                        .filter(|descriptor| descriptor.id != requested),
                );
            }
        }
    }
    if let Some(policy) = &request.telemetry_policy {
        policy.validate()?;
    }
    let mut rejected = Vec::new();
    let mut approved = Vec::new();
    for descriptor in candidates {
        let is_primary = requested.is_some_and(|requested| descriptor.id == requested);
        match evaluate_candidate(descriptor, request, is_primary) {
            Ok(model) => {
                let aggregate = request
                    .telemetry_policy
                    .as_ref()
                    .map(|policy| {
                        let since = policy.evaluated_unix.saturating_sub(policy.max_age_seconds);
                        route_telemetry_aggregate(
                            home,
                            descriptor.id,
                            model.as_str(),
                            since,
                            crate::MAX_TELEMETRY_SAMPLES,
                        )
                    })
                    .transpose()?
                    .flatten();
                if let Some(policy) = &request.telemetry_policy {
                    let accepted = match &aggregate {
                        None => policy.allow_missing,
                        Some(value) => {
                            value.samples >= policy.min_samples
                                && value.success_basis_points >= policy.min_success_basis_points
                                && value.p95_latency_millis <= policy.max_p95_latency_millis
                        }
                    };
                    if !accepted {
                        rejected.push(format!(
                            "{}:telemetry_policy_rejected",
                            descriptor.id.as_str()
                        ));
                        continue;
                    }
                }
                approved.push((descriptor, model, aggregate));
            }
            Err(reason) => rejected.push(format!("{}:{reason}", descriptor.id.as_str())),
        }
    }
    if request.telemetry_policy.is_some() && !automatic {
        approved.sort_by_key(telemetry_rank);
    }
    if let Some((descriptor, model, aggregate)) = approved.into_iter().next() {
        let mut reasons = if automatic {
            vec!["automatic connected provider satisfies policy".into()]
        } else if requested.is_some_and(|requested| descriptor.id == requested) {
            vec!["requested provider satisfies policy".into()]
        } else {
            vec!["explicit bounded fallback satisfies policy".into()]
        };
        if let Some(value) = aggregate {
            reasons.push(format!(
                "telemetry_snapshot_sha256={:x}",
                Sha256::digest(serde_json::to_vec(&value)?)
            ));
        } else if request.telemetry_policy.is_some() {
            reasons.push("telemetry_missing_explicitly_allowed".into());
        }
        let decision = RouteDecision {
            id: Uuid::new_v4(),
            surface: request.surface,
            provider: descriptor.id,
            model,
            fallback_from: requested.filter(|requested| descriptor.id != *requested),
            reasons,
            created_unix: now_unix(),
            trace_id: trace.map(|value| value.trace_id.to_string()),
            span_id: trace.map(|value| value.span_id.to_string()),
        };
        persist_decision(home, request, &decision)?;
        return Ok(decision);
    }
    Err(KernelError::Model(format!(
        "no policy-approved provider route: {}",
        rejected.join(",")
    )))
}

fn telemetry_rank(
    candidate: &(
        &ProviderDescriptor,
        ModelId,
        Option<RouteTelemetryAggregate>,
    ),
) -> (std::cmp::Reverse<u16>, u64, u64, ProviderId) {
    match &candidate.2 {
        Some(value) => (
            std::cmp::Reverse(value.success_basis_points),
            value.p95_latency_millis,
            value.mean_cost_microunits,
            candidate.0.id,
        ),
        None => (std::cmp::Reverse(0), u64::MAX, u64::MAX, candidate.0.id),
    }
}

fn evaluate_candidate(
    descriptor: &ProviderDescriptor,
    request: &RouteRequest,
    is_primary: bool,
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
    let requested_model = request
        .requested_model
        .as_deref()
        .map(ModelId::parse)
        .transpose()
        .map_err(|_| "invalid_model_identity".to_string())?;
    let model = match requested_model {
        Some(model) if descriptor.models.is_empty() || descriptor.models.contains(&model) => model,
        Some(_) if is_primary => {
            return Err("model_not_owned_by_provider".into());
        }
        Some(_) => {
            // Failover candidate: remap to that provider's default model.
            descriptor.default_model.clone()
        }
        None => descriptor.default_model.clone(),
    };
    if !descriptor.models.is_empty() && !descriptor.models.contains(&model) {
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
           created_unix INTEGER NOT NULL,trace_id TEXT,span_id TEXT
         );",
    )?;
    ensure_route_column(&connection, "trace_id", "TEXT")?;
    ensure_route_column(&connection, "span_id", "TEXT")?;
    connection.execute(
        "INSERT INTO route_decisions(
           id,surface,requested_provider,selected_provider,selected_model,fallback_from,
           reasons_json,request_json,created_unix,trace_id,span_id
         ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            decision.id.to_string(),
            decision.surface.as_str(),
            request.requested_provider,
            decision.provider.as_str(),
            decision.model.as_str(),
            decision.fallback_from.map(ProviderId::as_str),
            serde_json::to_string(&decision.reasons)?,
            serde_json::to_string(request)?,
            decision.created_unix as i64,
            decision.trace_id,
            decision.span_id
        ],
    )?;
    Ok(())
}

fn ensure_route_column(connection: &Connection, column: &str, declaration: &str) -> Result<()> {
    let mut statement = connection.prepare("PRAGMA table_info(route_decisions)")?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !names.iter().any(|name| name == column) {
        connection.execute(
            &format!("ALTER TABLE route_decisions ADD COLUMN {column} {declaration}"),
            [],
        )?;
    }
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
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvVarGuard {
        key: &'static str,
        original: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let guard = Self {
                key,
                original: std::env::var_os(key),
            };
            std::env::set_var(key, value);
            guard
        }

        fn remove(key: &'static str) -> Self {
            let guard = Self {
                key,
                original: std::env::var_os(key),
            };
            std::env::remove_var(key);
            guard
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    /// Any id a surface reads from the catalog must parse back to the same
    /// provider. The picker, the CLI, and the React capabilities page all do this
    /// round-trip, and serde's kebab-case form is not the same string as
    /// `as_str()`.
    #[test]
    fn every_catalog_id_round_trips_through_parse() {
        for descriptor in super::provider_catalog() {
            let wire = serde_json::to_value(descriptor.id).unwrap();
            let wire = wire.as_str().expect("provider id serializes as a string");
            assert_eq!(
                super::ProviderId::parse(wire),
                Some(descriptor.id),
                "catalog id {wire:?} must parse back"
            );
            assert_eq!(
                super::ProviderId::parse(descriptor.id.as_str()),
                Some(descriptor.id),
                "as_str form must parse back too"
            );
        }
    }
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn provider_aliases_resolve_to_canonical_identity() {
        assert_eq!(ProviderId::parse("openai"), Some(ProviderId::OpenAiCompat));
        assert_eq!(
            ProviderId::parse("openai_compat"),
            Some(ProviderId::OpenAiCompat),
            "legacy persisted provider spelling remains readable"
        );
        assert_eq!(ProviderId::parse("codex-oauth"), Some(ProviderId::Codex));
        assert_eq!(
            ProviderId::parse("auto"),
            None,
            "auto is a selector, never a concrete provider identity"
        );
        assert!(ModelId::parse("bad model with spaces").is_err());
    }

    #[test]
    fn automatic_selection_prefers_connected_codex_then_openai_then_offline() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _key = EnvVarGuard::remove("OPTIMUS_API_KEY");
        let directory = tempdir().unwrap();
        let mut statuses = provider_catalog_status(directory.path());

        assert_eq!(
            automatic_provider_order(&statuses),
            vec![ProviderId::Offline]
        );
        statuses
            .iter_mut()
            .find(|status| status.id == ProviderId::OpenAiCompat)
            .unwrap()
            .connect = ProviderConnectState::Connected;
        assert_eq!(
            automatic_provider_order(&statuses),
            vec![ProviderId::OpenAiCompat, ProviderId::Offline]
        );
        statuses
            .iter_mut()
            .find(|status| status.id == ProviderId::Codex)
            .unwrap()
            .connect = ProviderConnectState::Connected;
        assert_eq!(
            automatic_provider_order(&statuses),
            vec![
                ProviderId::Codex,
                ProviderId::OpenAiCompat,
                ProviderId::Offline
            ]
        );
    }

    #[test]
    fn automatic_codex_readiness_requires_usable_or_refreshable_access() {
        let status = |access_expiring, has_refresh| crate::CodexAuthStatus {
            present: true,
            access_expiring,
            has_refresh,
            source_note: "test".into(),
            base_url: crate::DEFAULT_CODEX_BASE_URL.into(),
            account_id: None,
        };

        assert_eq!(
            codex_connection(Some(&status(true, false))),
            (
                ProviderConnectState::Disconnected,
                "codex access token expiring without refresh token".into()
            )
        );
        assert_eq!(
            codex_connection(Some(&status(true, true))).0,
            ProviderConnectState::Connected,
            "an expiring access token is usable when it can refresh"
        );
        assert_eq!(
            codex_connection(Some(&status(false, false))).0,
            ProviderConnectState::Connected,
            "a non-expiring access token does not require refresh authority"
        );
    }

    #[test]
    fn automatic_fresh_home_selects_and_persists_concrete_offline_default() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _key = EnvVarGuard::remove("OPTIMUS_API_KEY");
        let directory = tempdir().unwrap();
        let request = RouteRequest::standard(RouteSurface::Desktop, "auto", None);

        let decision = resolve_route(directory.path(), &request).unwrap();

        assert_eq!(decision.provider, ProviderId::Offline);
        assert_eq!(decision.model.as_str(), "offline-scripted");
        assert_eq!(decision.fallback_from, None);
        assert_eq!(
            decision.reasons,
            vec!["automatic connected provider satisfies policy"]
        );
        let connection = Connection::open(directory.path().join("routing.db")).unwrap();
        let persisted = connection
            .query_row(
                "SELECT requested_provider,selected_provider,selected_model,reasons_json
                 FROM route_decisions WHERE id=?1",
                [decision.id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(persisted.0, "auto");
        assert_eq!(persisted.1, "offline");
        assert_eq!(persisted.2, "offline-scripted");
        assert!(persisted
            .3
            .contains("automatic connected provider satisfies policy"));
    }

    #[test]
    fn automatic_openai_readiness_matches_the_adapter_environment() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _key = EnvVarGuard::remove("OPTIMUS_API_KEY");
        let _irrelevant_base =
            EnvVarGuard::set("OPTIMUS_OPENAI_BASE_URL", "https://wrong.example/v1");
        let directory = tempdir().unwrap();

        let disconnected = provider_catalog_status(directory.path());
        let openai = disconnected
            .iter()
            .find(|status| status.id == ProviderId::OpenAiCompat)
            .unwrap();
        assert_eq!(openai.connect, ProviderConnectState::Disconnected);
        assert_eq!(openai.connect_detail, "OPTIMUS_API_KEY unset");

        let _configured_key = EnvVarGuard::set("OPTIMUS_API_KEY", "test-key");
        let connected = provider_catalog_status(directory.path());
        let openai = connected
            .iter()
            .find(|status| status.id == ProviderId::OpenAiCompat)
            .unwrap();
        assert_eq!(openai.connect, ProviderConnectState::Connected);
        assert_eq!(openai.connect_detail, "OPTIMUS_API_KEY present");

        let route = resolve_route(
            directory.path(),
            &RouteRequest::standard(RouteSurface::Desktop, "auto", None),
        )
        .unwrap();
        assert_eq!(route.provider, ProviderId::OpenAiCompat);
        assert_eq!(route.model.as_str(), "gpt-4.1");
    }

    #[test]
    fn automatic_model_is_absence_not_a_fake_model_identity() {
        let directory = tempdir().unwrap();
        let request = RouteRequest::standard(RouteSurface::Desktop, "auto", Some("auto".into()));
        let error = resolve_route(directory.path(), &request).unwrap_err();
        assert!(error.to_string().contains("absent requested model"));
        assert_eq!(route_decision_count(directory.path()).unwrap(), 0);
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

    #[test]
    fn ordered_failover_prefers_fallback_order_over_catalog_scan() {
        let directory = tempdir().unwrap();
        // Invalid codex model → denied; ordered failover to openai then offline.
        let mut request = RouteRequest::standard(
            RouteSurface::Desktop,
            "codex",
            Some("not-a-codex-model".into()),
        );
        request.allow_fallback = true;
        request.fallback_order = vec!["openai-compat".into(), "offline".into()];
        let decision = resolve_route(directory.path(), &request).unwrap();
        assert_eq!(decision.provider, ProviderId::OpenAiCompat);
        assert_eq!(decision.fallback_from, Some(ProviderId::Codex));

        let mut offline_first = request.clone();
        offline_first.fallback_order = vec!["offline".into(), "openai-compat".into()];
        let decision2 = resolve_route(directory.path(), &offline_first).unwrap();
        assert_eq!(decision2.provider, ProviderId::Offline);
    }

    #[test]
    fn failover_never_authorizes_statically_denied_candidates() {
        let directory = tempdir().unwrap();
        let mut request = RouteRequest::standard(RouteSurface::Cli, "codex", None);
        request.privacy = PrivacyPolicy::LocalOnly;
        request.allow_fallback = true;
        // openai-compat is remote → denied under LocalOnly; only offline survives.
        request.fallback_order = vec!["openai-compat".into(), "offline".into()];
        let decision = resolve_route(directory.path(), &request).unwrap();
        assert_eq!(decision.provider, ProviderId::Offline);
    }

    #[test]
    fn catalog_status_exposes_capability_flags_and_connect() {
        let directory = tempdir().unwrap();
        let rows = provider_catalog_status(directory.path());
        assert!(rows.len() >= 3);
        let offline = rows.iter().find(|r| r.id == ProviderId::Offline).unwrap();
        assert_eq!(offline.connect, ProviderConnectState::Connected);
        assert!(offline.supports_tools);
        assert!(!offline.supports_vision);
        let codex = rows.iter().find(|r| r.id == ProviderId::Codex).unwrap();
        assert!(codex.supports_streaming);
        assert!(codex.supports_vision);
        assert_eq!(codex.connect, ProviderConnectState::Disconnected);
    }
}
