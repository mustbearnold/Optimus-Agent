//! Progressive capability packs with schema-token budgets.

mod signed;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

pub use signed::{
    assert_policy_within_ceiling, default_third_party_ceiling, load_signed_manifest_file,
    sign_manifest, verify_manifest, PackManifestBody, SignedPackManifest, TrustRoot,
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PackError {
    #[error("{0}")]
    Msg(String),
    #[error("unknown pack: {0}")]
    UnknownPack(String),
    #[error("core pack cannot be deactivated")]
    CorePinned,
    #[error("on-demand pack limit exceeded: {loaded} >= {max}")]
    PackLimit { loaded: usize, max: usize },
    #[error("schema token budget exceeded: {would_be} > {max}")]
    SchemaBudget { would_be: u32, max: u32 },
    #[error("schema token total overflowed u32 for {scope}")]
    SchemaTokenOverflow { scope: String },
    #[error("pack not loaded: {0}")]
    NotLoaded(String),
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("tool not loaded: {tool} (pack: {pack})")]
    ToolNotLoaded { tool: String, pack: String },
    #[error("tool was not advertised for this model step: {0}")]
    ToolNotAdvertised(String),
    #[error("tool is catalogued but unavailable: {0}")]
    ToolUnavailable(String),
    #[error("duplicate tool identity {tool}: {first_pack} and {second_pack}")]
    DuplicateTool {
        tool: String,
        first_pack: String,
        second_pack: String,
    },
    #[error("descriptor invocation identity mismatch for {tool}: expected {expected}")]
    DescriptorInvocationMismatch { tool: String, expected: String },
    #[error("descriptor policy mismatch for {tool}: expected {expected:?}, got {actual:?}")]
    DescriptorPolicyMismatch {
        tool: String,
        expected: ToolPolicy,
        actual: ToolPolicy,
    },
    #[error(
        "descriptor schema token mismatch for {tool}: available={available}, schema_tokens={schema_tokens}"
    )]
    DescriptorSchemaTokensMismatch {
        tool: String,
        available: bool,
        schema_tokens: u32,
    },
    #[error("descriptor pack identity mismatch: map key {key}, descriptor {descriptor}")]
    DescriptorPackMismatch { key: String, descriptor: String },
    #[error("invalid input schema for {tool}: {reason}")]
    InvalidInputSchema { tool: String, reason: String },
    #[error("invalid output schema for {tool}: {reason}")]
    InvalidOutputSchema { tool: String, reason: String },
    #[error("descriptor replay mismatch for {tool}: expected {expected:?}, got {actual:?}")]
    DescriptorReplayMismatch {
        tool: String,
        expected: ReplayClass,
        actual: ReplayClass,
    },
    #[error("descriptor operational metadata mismatch for {tool}")]
    DescriptorOperationsMismatch { tool: String },
    #[error("invalid arguments for {tool}: {reason}")]
    InvalidArguments { tool: String, reason: String },
    #[error("invalid canonical tool outcome for {tool}: {reason}")]
    InvalidOutcome { tool: String, reason: String },
    #[error(
        "dispatch registry mismatch: catalog available invocations must equal ToolInvocation::ALL_DISPATCHABLE"
    )]
    DispatchRegistryMismatch { detail: String },
}

impl PackError {
    /// Stable tool-outcome error codes for fail-closed pack / budget failures.
    pub fn outcome_error_code(&self) -> &'static str {
        match self {
            Self::SchemaBudget { .. } => "pack_schema_budget_exceeded",
            Self::PackLimit { .. } => "pack_on_demand_limit_exceeded",
            Self::CorePinned => "pack_core_pinned",
            Self::UnknownPack(_) => "pack_unknown",
            Self::NotLoaded(_) => "pack_not_loaded",
            Self::ToolNotAdvertised(_) => "tool_not_advertised",
            Self::ToolUnavailable(_) => "tool_unavailable",
            Self::ToolNotLoaded { .. } => "tool_not_loaded",
            Self::UnknownTool(_) => "tool_unknown",
            Self::InvalidArguments { .. } => "tool_invalid_arguments",
            Self::InvalidOutcome { .. } => "tool_invalid_outcome",
            Self::DispatchRegistryMismatch { .. } => "tool_dispatch_registry_mismatch",
            Self::SchemaTokenOverflow { .. } => "pack_schema_token_overflow",
            Self::DuplicateTool { .. }
            | Self::DescriptorInvocationMismatch { .. }
            | Self::DescriptorPolicyMismatch { .. }
            | Self::DescriptorSchemaTokensMismatch { .. }
            | Self::DescriptorPackMismatch { .. }
            | Self::InvalidInputSchema { .. }
            | Self::InvalidOutputSchema { .. }
            | Self::DescriptorReplayMismatch { .. }
            | Self::DescriptorOperationsMismatch { .. } => "pack_descriptor_invalid",
            Self::Msg(_) => "pack_error",
        }
    }

    pub fn outcome_retryable(&self) -> bool {
        matches!(self, Self::SchemaBudget { .. } | Self::PackLimit { .. })
    }
}

pub type Result<T> = std::result::Result<T, PackError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackId {
    Core,
    Browser,
    Desktop,
    Media,
    Devex,
    Social,
    /// Home-automation / IoT breadth (Track Z.10).
    Home,
    /// Office docs breadth (Track Z.10).
    Office,
}

impl PackId {
    pub fn as_str(self) -> &'static str {
        match self {
            PackId::Core => "core",
            PackId::Browser => "browser",
            PackId::Desktop => "desktop",
            PackId::Media => "media",
            PackId::Devex => "devex",
            PackId::Social => "social",
            PackId::Home => "home",
            PackId::Office => "office",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "core" => Some(Self::Core),
            "browser" => Some(Self::Browser),
            "desktop" => Some(Self::Desktop),
            "media" => Some(Self::Media),
            "devex" => Some(Self::Devex),
            "social" => Some(Self::Social),
            "home" => Some(Self::Home),
            "office" => Some(Self::Office),
            _ => None,
        }
    }

    pub fn is_core(self) -> bool {
        matches!(self, PackId::Core)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct ToolId(String);

impl ToolId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ToolId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ToolId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

pub const TOOL_OUTCOME_VERSION: u16 = 1;
pub const MAX_TOOL_OUTCOME_SUMMARY_BYTES: usize = 4096;
pub const MAX_TOOL_OUTCOME_DATA_BYTES: usize = 128 * 1024;
pub const MAX_TOOL_OUTCOME_ARTIFACTS: usize = 64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutcomeKind {
    Succeeded,
    Failed,
    Cancelled,
    Ambiguous,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplayClass {
    Deterministic,
    Convergent,
    FixtureReplayable,
    ModelNondeterministic,
    ExternalNondeterministic,
    Destructive,
    Ambiguous,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolErrorDetail {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactRef {
    pub id: String,
    pub media_type: String,
    pub path: Option<String>,
    pub uri: Option<String>,
    pub sha256: Option<String>,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DurableEffectProvenance {
    pub job_id: uuid::Uuid,
    pub node_id: uuid::Uuid,
    pub effect_attempt_id: uuid::Uuid,
    pub effect_sha256: String,
    pub receipt_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolOutcome {
    pub version: u16,
    pub call_id: String,
    pub tool_id: ToolId,
    pub kind: ToolOutcomeKind,
    pub summary: String,
    pub data: Value,
    pub artifacts: Vec<ArtifactRef>,
    pub error: Option<ToolErrorDetail>,
    pub replay: ReplayClass,
    pub provenance: Option<DurableEffectProvenance>,
}

impl ToolOutcome {
    pub fn succeeded(
        call_id: impl Into<String>,
        tool_id: impl Into<ToolId>,
        summary: impl Into<String>,
        data: Value,
        replay: ReplayClass,
    ) -> Self {
        Self {
            version: TOOL_OUTCOME_VERSION,
            call_id: call_id.into(),
            tool_id: tool_id.into(),
            kind: ToolOutcomeKind::Succeeded,
            summary: summary.into(),
            data,
            artifacts: Vec::new(),
            error: None,
            replay,
            provenance: None,
        }
    }

    pub fn failed(
        call_id: impl Into<String>,
        tool_id: impl Into<ToolId>,
        summary: impl Into<String>,
        error: ToolErrorDetail,
        replay: ReplayClass,
    ) -> Self {
        Self {
            version: TOOL_OUTCOME_VERSION,
            call_id: call_id.into(),
            tool_id: tool_id.into(),
            kind: ToolOutcomeKind::Failed,
            summary: summary.into(),
            data: json!({}),
            artifacts: Vec::new(),
            error: Some(error),
            replay,
            provenance: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        let fail = |reason: String| PackError::InvalidOutcome {
            tool: self.tool_id.as_str().into(),
            reason,
        };
        if self.version != TOOL_OUTCOME_VERSION {
            return Err(fail(format!(
                "unsupported version {}; expected {TOOL_OUTCOME_VERSION}",
                self.version
            )));
        }
        validate_identifier(&self.call_id, 128).map_err(&fail)?;
        validate_identifier(self.tool_id.as_str(), 128).map_err(&fail)?;
        if self.summary.is_empty() || self.summary.len() > MAX_TOOL_OUTCOME_SUMMARY_BYTES {
            return Err(fail(format!(
                "summary must contain 1..={MAX_TOOL_OUTCOME_SUMMARY_BYTES} bytes"
            )));
        }
        let data_bytes = serde_json::to_vec(&self.data)
            .map_err(|error| fail(format!("data is not serializable: {error}")))?
            .len();
        if data_bytes > MAX_TOOL_OUTCOME_DATA_BYTES {
            return Err(fail(format!(
                "data exceeds {MAX_TOOL_OUTCOME_DATA_BYTES} bytes"
            )));
        }
        if self.artifacts.len() > MAX_TOOL_OUTCOME_ARTIFACTS {
            return Err(fail(format!(
                "artifact count exceeds {MAX_TOOL_OUTCOME_ARTIFACTS}"
            )));
        }
        match self.kind {
            ToolOutcomeKind::Succeeded if self.error.is_some() => {
                return Err(fail("succeeded outcome cannot carry an error".into()));
            }
            ToolOutcomeKind::Failed | ToolOutcomeKind::Cancelled | ToolOutcomeKind::Ambiguous
                if self.error.is_none() =>
            {
                return Err(fail("non-success outcome requires an error".into()));
            }
            _ => {}
        }
        if self.kind == ToolOutcomeKind::Ambiguous && self.replay != ReplayClass::Ambiguous {
            return Err(fail(
                "ambiguous outcome requires ambiguous replay classification".into(),
            ));
        }
        if let Some(error) = &self.error {
            validate_error_code(&error.code).map_err(&fail)?;
            if error.message.is_empty() || error.message.len() > MAX_TOOL_OUTCOME_SUMMARY_BYTES {
                return Err(fail(format!(
                    "error message must contain 1..={MAX_TOOL_OUTCOME_SUMMARY_BYTES} bytes"
                )));
            }
        }
        for artifact in &self.artifacts {
            validate_identifier(&artifact.id, 128).map_err(&fail)?;
            if artifact.media_type.is_empty() || artifact.media_type.len() > 255 {
                return Err(fail(
                    "artifact media_type must contain 1..=255 bytes".into(),
                ));
            }
            if artifact.path.is_none() && artifact.uri.is_none() {
                return Err(fail("artifact requires path or uri".into()));
            }
            if let Some(hash) = &artifact.sha256 {
                validate_sha256(hash).map_err(&fail)?;
            }
        }
        if let Some(provenance) = &self.provenance {
            validate_sha256(&provenance.effect_sha256).map_err(&fail)?;
            if let Some(hash) = &provenance.receipt_sha256 {
                validate_sha256(hash).map_err(&fail)?;
            }
        }
        Ok(())
    }
}

fn validate_identifier(value: &str, max_bytes: usize) -> std::result::Result<(), String> {
    if value.is_empty() || value.len() > max_bytes {
        return Err(format!("identifier must contain 1..={max_bytes} bytes"));
    }
    if value.chars().any(char::is_control) {
        return Err("identifier cannot contain control characters".into());
    }
    Ok(())
}

fn validate_error_code(value: &str) -> std::result::Result<(), String> {
    validate_identifier(value, 128)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte))
    {
        return Err(
            "error code must use lowercase ASCII letters, digits, dot, dash, or underscore".into(),
        );
    }
    Ok(())
}

fn validate_sha256(value: &str) -> std::result::Result<(), String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("sha256 must be exactly 64 hexadecimal characters".into());
    }
    Ok(())
}

mod invocation;

pub use invocation::{
    ToolCancellation, ToolIdempotency, ToolInvocation, ToolObservability, ToolOperations,
    ToolRetry, ToolTimeout,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolPolicy {
    WorkspaceRead,
    WorkspaceWrite,
    Process,
    NetworkRead,
    MemoryRead,
    SkillRead,
    Capability,
    Browser,
    UserInteraction,
    Desktop,
    Media,
    NetworkWrite,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDesc {
    pub id: ToolId,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub replay: ReplayClass,
    pub policy: ToolPolicy,
    pub invocation: ToolInvocation,
    pub operations: ToolOperations,
    /// Estimated provider-visible schema tokens when this tool is available.
    pub schema_tokens: u32,
}

impl ToolDesc {
    pub fn is_available(&self) -> bool {
        self.invocation != ToolInvocation::Unavailable
    }

    pub fn validate_outcome(&self, outcome: &ToolOutcome) -> Result<()> {
        outcome.validate()?;
        let fail = |reason: String| PackError::InvalidOutcome {
            tool: self.id.as_str().into(),
            reason,
        };
        if outcome.tool_id != self.id {
            return Err(fail(format!(
                "outcome tool identity {} does not match descriptor {}",
                outcome.tool_id.as_str(),
                self.id.as_str()
            )));
        }
        if outcome.replay != self.replay {
            return Err(fail(format!(
                "outcome replay {:?} does not match descriptor {:?}",
                outcome.replay, self.replay
            )));
        }
        if self.output_schema != canonical_tool_output_schema() {
            return Err(fail("descriptor output schema is not canonical".into()));
        }
        Ok(())
    }

    pub fn validate_arguments(&self, arguments: &Value) -> Result<()> {
        let fail = |reason: String| PackError::InvalidArguments {
            tool: self.id.as_str().into(),
            reason,
        };
        let object = arguments
            .as_object()
            .ok_or_else(|| fail("expected object".into()))?;
        let properties = self.input_schema["properties"]
            .as_object()
            .ok_or_else(|| fail("descriptor properties are not an object".into()))?;
        if let Some(required) = self.input_schema["required"].as_array() {
            for name in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(name) {
                    return Err(fail(format!("missing required property {name}")));
                }
            }
        }
        if self.input_schema["additionalProperties"] == Value::Bool(false) {
            for name in object.keys() {
                if !properties.contains_key(name) {
                    return Err(fail(format!("unexpected property {name}")));
                }
            }
        }
        for (name, value) in object {
            let Some(schema) = properties.get(name) else {
                continue;
            };
            if let Some(expected) = schema.get("type").and_then(Value::as_str) {
                let type_ok = match expected {
                    "string" => value.is_string(),
                    "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
                    "number" => value.is_number(),
                    "boolean" => value.is_boolean(),
                    "array" => value.is_array(),
                    "object" => value.is_object(),
                    _ => false,
                };
                if !type_ok {
                    return Err(fail(format!("property {name} must be {expected}")));
                }
            }
            if let Some(options) = schema.get("enum").and_then(Value::as_array) {
                if !options.contains(value) {
                    return Err(fail(format!("property {name} is not an allowed value")));
                }
            }
            if let (Some(minimum), Some(actual)) = (
                schema.get("minimum").and_then(Value::as_i64),
                value.as_i64(),
            ) {
                if actual < minimum {
                    return Err(fail(format!("property {name} must be >= {minimum}")));
                }
            }
            if let (Some(items), Some(values)) = (schema.get("items"), value.as_array()) {
                if items.get("type").and_then(Value::as_str) == Some("string")
                    && values.iter().any(|item| !item.is_string())
                {
                    return Err(fail(format!("property {name} items must be string")));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackDesc {
    pub id: PackId,
    pub summary: String,
    pub tools: Vec<ToolDesc>,
}

impl PackDesc {
    pub fn schema_tokens(&self) -> u32 {
        self.checked_schema_tokens().unwrap_or(u32::MAX)
    }

    fn checked_schema_tokens(&self) -> Result<u32> {
        self.tools.iter().try_fold(0u32, |total, tool| {
            total
                .checked_add(tool.schema_tokens)
                .ok_or_else(|| PackError::SchemaTokenOverflow {
                    scope: self.id.as_str().into(),
                })
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackBudgetConfig {
    /// Max simultaneous on-demand packs (excludes core).
    pub max_on_demand_packs: usize,
    /// Hard ceiling on summed schema tokens for loaded packs.
    pub max_schema_tokens: u32,
}

impl Default for PackBudgetConfig {
    fn default() -> Self {
        Self {
            max_on_demand_packs: 2,
            max_schema_tokens: 2500,
        }
    }
}

/// Fail closed unless every available catalog tool maps to
/// [`ToolInvocation::ALL_DISPATCHABLE`] and every dispatchable appears once.
pub fn assert_dispatch_registry_closed(catalog: &BTreeMap<PackId, PackDesc>) -> Result<()> {
    let mut seen: BTreeSet<ToolInvocation> = BTreeSet::new();
    for pack in catalog.values() {
        for tool in &pack.tools {
            if !tool.is_available() {
                continue;
            }
            if !ToolInvocation::ALL_DISPATCHABLE.contains(&tool.invocation) {
                return Err(PackError::DispatchRegistryMismatch {
                    detail: format!(
                        "available tool {} uses invocation {:?} outside ALL_DISPATCHABLE",
                        tool.id.as_str(),
                        tool.invocation
                    ),
                });
            }
            if !seen.insert(tool.invocation) {
                return Err(PackError::DispatchRegistryMismatch {
                    detail: format!(
                        "duplicate available invocation {:?} (tool {})",
                        tool.invocation,
                        tool.id.as_str()
                    ),
                });
            }
        }
    }
    for inv in ToolInvocation::ALL_DISPATCHABLE {
        if !seen.contains(inv) {
            return Err(PackError::DispatchRegistryMismatch {
                detail: format!(
                    "dispatchable {:?} missing from available catalog tools",
                    inv
                ),
            });
        }
    }
    Ok(())
}

/// Catalog of built-in packs (Hermes waist lesson: core small, edges on demand).
pub fn builtin_catalog() -> BTreeMap<PackId, PackDesc> {
    let mut m = BTreeMap::new();
    m.insert(
        PackId::Core,
        PackDesc {
            id: PackId::Core,
            summary: "Always-on waist: fs, terminal, web, memory, skills, packs, jobs, clarify"
                .into(),
            tools: vec![
                tool(
                    ToolInvocation::ReadFile,
                    "Read a workspace file, optionally a line range",
                    150,
                    object_schema(
                        json!({
                            "path":{"type":"string"},
                            "offset":{"type":"integer"},
                            "limit":{"type":"integer"}
                        }),
                        &["path"],
                    ),
                ),
                tool(
                    ToolInvocation::SearchContent,
                    "Search file contents by regular expression. Prefer this over \
                     running grep or rg through the terminal",
                    200,
                    object_schema(
                        json!({
                            "pattern":{"type":"string"},
                            "path":{"type":"string"},
                            "glob":{"type":"string"},
                            "case_sensitive":{"type":"boolean"},
                            "max_results":{"type":"integer"}
                        }),
                        &["pattern"],
                    ),
                ),
                tool(
                    ToolInvocation::FindFiles,
                    "Find files by glob pattern. Prefer this over running find or \
                     fd through the terminal",
                    140,
                    object_schema(
                        json!({
                            "glob":{"type":"string"},
                            "path":{"type":"string"},
                            "max_results":{"type":"integer"}
                        }),
                        &["glob"],
                    ),
                ),
                tool(
                    ToolInvocation::ListDir,
                    "List one workspace directory. Prefer this over running ls \
                     through the terminal",
                    100,
                    object_schema(json!({"path":{"type":"string"}}), &[]),
                ),
                tool(
                    ToolInvocation::WriteFile,
                    "Write workspace file; returns relative_path and absolute_path without terminal",
                    120,
                    object_schema(
                        json!({"path":{"type":"string"},"contents":{"type":"string"}}),
                        &["path", "contents"],
                    ),
                ),
                tool(
                    ToolInvocation::Mkdir,
                    "Create workspace directory (and parents)",
                    80,
                    object_schema(json!({"path":{"type":"string"}}), &["path"]),
                ),
                tool(
                    ToolInvocation::DeletePath,
                    "Delete workspace file or empty directory",
                    80,
                    object_schema(json!({"path":{"type":"string"}}), &["path"]),
                ),
                tool(
                    ToolInvocation::RenamePath,
                    "Rename or move within the workspace",
                    100,
                    object_schema(
                        json!({
                            "from":{"type":"string"},
                            "to":{"type":"string"}
                        }),
                        &["from", "to"],
                    ),
                ),
                tool(
                    ToolInvocation::PatchFile,
                    "Exact single-occurrence string replace in a workspace file",
                    140,
                    object_schema(
                        json!({
                            "path":{"type":"string"},
                            "old_string":{"type":"string"},
                            "new_string":{"type":"string"}
                        }),
                        &["path", "old_string", "new_string"],
                    ),
                ),
                tool(
                    ToolInvocation::Terminal,
                    "Run bounded shell command",
                    150,
                    object_schema(
                        json!({
                            "program":{"type":"string"},
                            "args":{"type":"array","items":{"type":"string"}}
                        }),
                        &["program"],
                    ),
                ),
                tool(
                    ToolInvocation::WebSearch,
                    "Search the web",
                    140,
                    object_schema(
                        json!({"query":{"type":"string"},"limit":{"type":"integer","minimum":1}}),
                        &["query"],
                    ),
                ),
                tool(
                    ToolInvocation::MemoryRecall,
                    "Recall EvidencePacket",
                    130,
                    object_schema(
                        json!({"subject":{"type":"string"},"predicate":{"type":"string"}}),
                        &[],
                    ),
                ),
                tool(
                    ToolInvocation::SkillResolve,
                    "Resolve procedural skill by name",
                    120,
                    object_schema(json!({"name":{"type":"string"}}), &["name"]),
                ),
                tool(
                    ToolInvocation::ActivatePack,
                    "Load an on-demand capability pack",
                    100,
                    object_schema(
                        json!({"name":{"type":"string","enum":["browser","desktop","media","devex","social"]}}),
                        &["name"],
                    ),
                ),
                unavailable("clarify", "Ask the user", ToolPolicy::UserInteraction),
            ],
        },
    );
    m.insert(
        PackId::Browser,
        PackDesc {
            id: PackId::Browser,
            summary: "CDP/browser automation".into(),
            tools: vec![
                tool(
                    ToolInvocation::BrowserNavigate,
                    "Navigate URL",
                    200,
                    object_schema(json!({"url":{"type":"string"}}), &["url"]),
                ),
                tool(
                    ToolInvocation::BrowserClick,
                    "Click element",
                    180,
                    object_schema(json!({"index":{"type":"integer","minimum":0}}), &["index"]),
                ),
                tool(
                    ToolInvocation::BrowserSnapshot,
                    "A11y snapshot",
                    220,
                    object_schema(json!({}), &[]),
                ),
            ],
        },
    );
    m.insert(
        PackId::Desktop,
        PackDesc {
            id: PackId::Desktop,
            summary: "Computer-use / OS UI automation".into(),
            tools: vec![
                unavailable("desktop_screenshot", "Capture screen", ToolPolicy::Desktop),
                unavailable("desktop_click", "Click coordinates", ToolPolicy::Desktop),
                unavailable("desktop_type", "Type keys", ToolPolicy::Desktop),
            ],
        },
    );
    m.insert(
        PackId::Media,
        PackDesc {
            id: PackId::Media,
            summary: "Vision (imagegen/TTS return with their lane; ADR-0068)".into(),
            tools: vec![unavailable(
                "vision_analyze",
                "Analyze image",
                ToolPolicy::Media,
            )],
        },
    );
    m.insert(
        PackId::Devex,
        PackDesc {
            id: PackId::Devex,
            summary: "Git/GH/deep dev workflows (no tools until designed; ADR-0068)".into(),
            tools: vec![],
        },
    );
    m.insert(
        PackId::Social,
        PackDesc {
            id: PackId::Social,
            summary: "Messaging (returns with a live gateway transport; ADR-0068)".into(),
            tools: vec![],
        },
    );
    m.insert(
        PackId::Home,
        PackDesc {
            id: PackId::Home,
            summary: "Home automation (no tools until integrated; ADR-0068)".into(),
            tools: vec![],
        },
    );
    m.insert(
        PackId::Office,
        PackDesc {
            id: PackId::Office,
            summary: "Office documents (no tools until integrated; ADR-0068)".into(),
            tools: vec![],
        },
    );
    m
}

fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn validate_input_schema(schema: &Value) -> std::result::Result<(), String> {
    let object = schema
        .as_object()
        .ok_or_else(|| "schema must be an object".to_string())?;
    for keyword in object.keys() {
        if !matches!(
            keyword.as_str(),
            "type" | "properties" | "required" | "additionalProperties"
        ) {
            return Err(format!("unsupported top-level keyword {keyword}"));
        }
    }
    if object.get("type").and_then(Value::as_str) != Some("object") {
        return Err("top-level type must be object".into());
    }
    let properties = object
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| "properties must be an object".to_string())?;
    let required = object
        .get("required")
        .and_then(Value::as_array)
        .ok_or_else(|| "required must be an array".to_string())?;
    if !object
        .get("additionalProperties")
        .is_some_and(Value::is_boolean)
    {
        return Err("additionalProperties must be boolean".into());
    }

    let mut required_names = BTreeSet::new();
    for value in required {
        let name = value
            .as_str()
            .ok_or_else(|| "required entries must be strings".to_string())?;
        if !properties.contains_key(name) {
            return Err(format!("required property {name} is not declared"));
        }
        if !required_names.insert(name) {
            return Err(format!("required property {name} is duplicated"));
        }
    }

    for (name, schema) in properties {
        let property = schema
            .as_object()
            .ok_or_else(|| format!("property {name} schema must be an object"))?;
        for keyword in property.keys() {
            if !matches!(keyword.as_str(), "type" | "enum" | "minimum" | "items") {
                return Err(format!("property {name} has unsupported keyword {keyword}"));
            }
        }
        let property_type = property
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("property {name} must declare a string type"))?;
        if !matches!(
            property_type,
            "string" | "integer" | "number" | "boolean" | "array" | "object"
        ) {
            return Err(format!(
                "property {name} has unsupported type {property_type}"
            ));
        }
        if let Some(values) = property.get("enum") {
            let values = values
                .as_array()
                .ok_or_else(|| format!("property {name} enum must be an array"))?;
            if values
                .iter()
                .any(|value| !value_matches_type(value, property_type))
            {
                return Err(format!("property {name} enum value has wrong type"));
            }
        }
        if let Some(minimum) = property.get("minimum") {
            if property_type != "integer" || minimum.as_i64().is_none() {
                return Err(format!(
                    "property {name} minimum requires an integer type and value"
                ));
            }
        }
        match property.get("items") {
            Some(items) if property_type == "array" => {
                let items = items
                    .as_object()
                    .ok_or_else(|| format!("property {name} items must be an object"))?;
                if items.len() != 1 || items.get("type").and_then(Value::as_str) != Some("string") {
                    return Err(format!("property {name} supports only string array items"));
                }
            }
            Some(_) => return Err(format!("property {name} items requires array type")),
            None if property_type == "array" => {
                return Err(format!("property {name} array must declare string items"));
            }
            None => {}
        }
    }
    Ok(())
}

fn value_matches_type(value: &Value, expected: &str) -> bool {
    match expected {
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => false,
    }
}

pub fn canonical_tool_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "version": {"type":"integer"},
            "call_id": {"type":"string"},
            "tool_id": {"type":"string"},
            "kind": {"type":"string", "enum":["succeeded","failed","cancelled","ambiguous"]},
            "summary": {"type":"string"},
            "data": {},
            "artifacts": {"type":"array", "items":{"type":"object"}},
            "error": {"type":["object","null"]},
            "replay": {"type":"string", "enum":["deterministic","convergent","fixture_replayable","model_nondeterministic","external_nondeterministic","destructive","ambiguous"]},
            "provenance": {"type":["object","null"]}
        },
        "required": ["version","call_id","tool_id","kind","summary","data","artifacts","error","replay","provenance"],
        "additionalProperties": false
    })
}

fn tool(
    invocation: ToolInvocation,
    description: &str,
    schema_tokens: u32,
    input_schema: Value,
) -> ToolDesc {
    ToolDesc {
        id: ToolId::new(
            invocation
                .id()
                .expect("available tool invocation has an id"),
        ),
        description: description.into(),
        input_schema,
        output_schema: canonical_tool_output_schema(),
        replay: invocation.replay(),
        policy: invocation
            .policy()
            .expect("available tool invocation has a policy"),
        invocation,
        operations: invocation.operations(),
        schema_tokens,
    }
}

fn unavailable(id: &str, description: &str, policy: ToolPolicy) -> ToolDesc {
    ToolDesc {
        id: ToolId::new(id),
        description: description.into(),
        input_schema: object_schema(json!({}), &[]),
        output_schema: canonical_tool_output_schema(),
        replay: ToolInvocation::Unavailable.replay(),
        policy,
        invocation: ToolInvocation::Unavailable,
        operations: ToolInvocation::Unavailable.operations(),
        schema_tokens: 0,
    }
}

#[derive(Debug, Clone)]
pub struct CapabilitySession {
    catalog: BTreeMap<PackId, PackDesc>,
    loaded: BTreeSet<PackId>,
    config: PackBudgetConfig,
    /// Activation log (segment boundaries).
    pub activations: Vec<PackId>,
}

impl CapabilitySession {
    pub fn new(config: PackBudgetConfig) -> Result<Self> {
        Self::try_from_catalog(config, builtin_catalog())
    }

    pub fn try_from_catalog(
        config: PackBudgetConfig,
        catalog: BTreeMap<PackId, PackDesc>,
    ) -> Result<Self> {
        let mut owners = BTreeMap::<ToolId, PackId>::new();
        for (pack_id, pack) in &catalog {
            if *pack_id != pack.id {
                return Err(PackError::DescriptorPackMismatch {
                    key: pack_id.as_str().into(),
                    descriptor: pack.id.as_str().into(),
                });
            }
            for tool in &pack.tools {
                if let Some(previous) = owners.insert(tool.id.clone(), *pack_id) {
                    return Err(PackError::DuplicateTool {
                        tool: tool.id.as_str().into(),
                        first_pack: previous.as_str().into(),
                        second_pack: pack_id.as_str().into(),
                    });
                }
                let available = tool.is_available();
                if (available && tool.schema_tokens == 0) || (!available && tool.schema_tokens != 0)
                {
                    return Err(PackError::DescriptorSchemaTokensMismatch {
                        tool: tool.id.as_str().into(),
                        available,
                        schema_tokens: tool.schema_tokens,
                    });
                }
                if tool.is_available() {
                    let expected_id = tool
                        .invocation
                        .id()
                        .expect("available invocation has canonical id");
                    if tool.id.as_str() != expected_id {
                        return Err(PackError::DescriptorInvocationMismatch {
                            tool: tool.id.as_str().into(),
                            expected: expected_id.into(),
                        });
                    }
                    let expected_policy = tool
                        .invocation
                        .policy()
                        .expect("available invocation has canonical policy");
                    if tool.policy != expected_policy {
                        return Err(PackError::DescriptorPolicyMismatch {
                            tool: tool.id.as_str().into(),
                            expected: expected_policy,
                            actual: tool.policy,
                        });
                    }
                }
                if let Err(reason) = validate_input_schema(&tool.input_schema) {
                    return Err(PackError::InvalidInputSchema {
                        tool: tool.id.as_str().into(),
                        reason,
                    });
                }
                if tool.output_schema != canonical_tool_output_schema() {
                    return Err(PackError::InvalidOutputSchema {
                        tool: tool.id.as_str().into(),
                        reason: "descriptor must use the canonical tool outcome schema".into(),
                    });
                }
                let expected_replay = tool.invocation.replay();
                if tool.replay != expected_replay {
                    return Err(PackError::DescriptorReplayMismatch {
                        tool: tool.id.as_str().into(),
                        expected: expected_replay,
                        actual: tool.replay,
                    });
                }
                if tool.operations != tool.invocation.operations() {
                    return Err(PackError::DescriptorOperationsMismatch {
                        tool: tool.id.as_str().into(),
                    });
                }
            }
            pack.checked_schema_tokens()?;
        }
        assert_dispatch_registry_closed(&catalog)?;
        let core_tokens = catalog
            .get(&PackId::Core)
            .ok_or_else(|| PackError::UnknownPack("core".into()))?
            .checked_schema_tokens()?;
        if core_tokens > config.max_schema_tokens {
            return Err(PackError::SchemaBudget {
                would_be: core_tokens,
                max: config.max_schema_tokens,
            });
        }
        let mut loaded = BTreeSet::new();
        loaded.insert(PackId::Core);
        Ok(Self {
            catalog,
            loaded,
            config,
            activations: vec![PackId::Core],
        })
    }

    /// Hard ceiling configured for this session (schema tokens).
    pub fn max_schema_tokens(&self) -> u32 {
        self.config.max_schema_tokens
    }

    /// Max simultaneous on-demand packs (excludes core).
    pub fn max_on_demand_packs(&self) -> usize {
        self.config.max_on_demand_packs
    }

    /// Progressive activation report for tool outcomes / UI.
    pub fn activation_snapshot(&self) -> Value {
        json!({
            "ok": true,
            "loaded": self.loaded_packs().iter().map(|p| p.as_str()).collect::<Vec<_>>(),
            "schema_tokens": self.schema_tokens(),
            "max_schema_tokens": self.config.max_schema_tokens,
            "on_demand_loaded": self.on_demand_count(),
            "max_on_demand_packs": self.config.max_on_demand_packs,
            "activations": self.activations.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        })
    }

    pub fn with_defaults() -> Self {
        Self::new(PackBudgetConfig::default())
            .expect("default pack budget must fit the built-in core pack")
    }

    pub fn loaded_packs(&self) -> Vec<PackId> {
        self.loaded.iter().copied().collect()
    }

    pub fn catalog(&self) -> &BTreeMap<PackId, PackDesc> {
        &self.catalog
    }

    pub fn schema_tokens(&self) -> u32 {
        self.checked_loaded_schema_tokens().unwrap_or(u32::MAX)
    }

    fn checked_loaded_schema_tokens(&self) -> Result<u32> {
        self.loaded
            .iter()
            .filter_map(|id| self.catalog.get(id))
            .try_fold(0u32, |total, pack| {
                let pack_tokens = pack.checked_schema_tokens()?;
                total
                    .checked_add(pack_tokens)
                    .ok_or_else(|| PackError::SchemaTokenOverflow {
                        scope: "loaded packs".into(),
                    })
            })
    }

    pub fn loaded_tools(&self) -> Vec<&ToolDesc> {
        let mut tools = Vec::new();
        for id in &self.loaded {
            if let Some(p) = self.catalog.get(id) {
                tools.extend(p.tools.iter().filter(|tool| tool.is_available()));
            }
        }
        tools
    }

    pub fn resolve_loaded_tool(&self, name: &str) -> Result<&ToolDesc> {
        let (pack_id, tool) = self
            .catalog
            .iter()
            .find_map(|(pack_id, pack)| {
                pack.tools
                    .iter()
                    .find(|tool| tool.id.as_str() == name)
                    .map(|tool| (*pack_id, tool))
            })
            .ok_or_else(|| PackError::UnknownTool(name.into()))?;
        if !self.loaded.contains(&pack_id) {
            return Err(PackError::ToolNotLoaded {
                tool: name.into(),
                pack: pack_id.as_str().into(),
            });
        }
        if !tool.is_available() {
            return Err(PackError::ToolUnavailable(name.into()));
        }
        Ok(tool)
    }

    pub fn on_demand_count(&self) -> usize {
        self.loaded.iter().filter(|p| !p.is_core()).count()
    }

    pub fn activate(&mut self, pack: PackId) -> Result<()> {
        if !self.catalog.contains_key(&pack) {
            return Err(PackError::UnknownPack(pack.as_str().into()));
        }
        if self.loaded.contains(&pack) {
            return Ok(());
        }
        if !pack.is_core() {
            let n = self.on_demand_count();
            if n >= self.config.max_on_demand_packs {
                return Err(PackError::PackLimit {
                    loaded: n,
                    max: self.config.max_on_demand_packs,
                });
            }
        }
        let add = self
            .catalog
            .get(&pack)
            .ok_or_else(|| PackError::UnknownPack(pack.as_str().into()))?
            .checked_schema_tokens()?;
        let would_be = self
            .checked_loaded_schema_tokens()?
            .checked_add(add)
            .ok_or_else(|| PackError::SchemaTokenOverflow {
                scope: "loaded packs".into(),
            })?;
        if would_be > self.config.max_schema_tokens {
            return Err(PackError::SchemaBudget {
                would_be,
                max: self.config.max_schema_tokens,
            });
        }
        self.loaded.insert(pack);
        self.activations.push(pack);
        Ok(())
    }

    pub fn activate_str(&mut self, name: &str) -> Result<()> {
        let pack = PackId::parse(name).ok_or_else(|| PackError::UnknownPack(name.into()))?;
        self.activate(pack)
    }

    pub fn deactivate(&mut self, pack: PackId) -> Result<()> {
        if pack.is_core() {
            return Err(PackError::CorePinned);
        }
        if !self.loaded.remove(&pack) {
            return Err(PackError::NotLoaded(pack.as_str().into()));
        }
        Ok(())
    }

    /// Restore persisted packs against the current catalog and budget. Always includes Core.
    pub fn restore_loaded(&mut self, packs: &[PackId]) -> Result<()> {
        self.loaded.clear();
        self.loaded.insert(PackId::Core);
        self.activations = vec![PackId::Core];
        for p in packs {
            if *p == PackId::Core {
                continue;
            }
            if let Err(error) = self.activate(*p) {
                self.loaded.clear();
                self.loaded.insert(PackId::Core);
                self.activations = vec![PackId::Core];
                return Err(error);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn core_tokens_under_default_budget() {
        let s = CapabilitySession::with_defaults();
        assert!(s.schema_tokens() < 2500);
        assert!(s.schema_tokens() > 0);
        assert_eq!(s.loaded_packs(), vec![PackId::Core]);
    }
}
