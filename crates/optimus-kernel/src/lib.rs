//! Provider-agnostic Kernel turn loop.
//!
//! Offline evaluation and fixture replay live in `optimus-eval` (depends on this
//! crate). Operator gateway and cron storage live in `optimus-ops` and are
//! re-exported here for surface convenience without growing the turn-loop waist.

mod browser;
mod browser_coord;
mod causal;
mod chat_approval;
pub mod codex_device_login;
mod codex_oauth;
mod compress;
mod credential;
mod execution;
mod fs_sandbox;
mod fs_search;
mod home_ops;
mod model_call;
mod network_policy;
mod openai_compat;
mod page_extract;
mod product_settings;
mod profile;
mod project_authority;
mod routing;
mod scripted;
mod security_denial;
mod session;
mod skill_index;
mod system_prompt;
mod telemetry;
mod tool_dispatch;
mod tool_report;
mod trace;
mod turn_loop;
mod web_search;

use optimus_graph::{Effect, JobSpec, NodeSpec};
use optimus_packs::{
    CapabilitySession, DurableEffectProvenance, PackBudgetConfig, PackError, PackId,
    ToolErrorDetail, ToolId, ToolInvocation, ToolOutcome, ToolOutcomeKind,
};
use optimus_runtime::{ApprovalGrant, JobId, JobStatus, Runtime, RuntimeError};
use std::path::{Path, PathBuf};
use std::{cell::Cell, collections::BTreeSet};

pub use optimus_memory::{
    ClaimDraft, ClaimView, Correction, EvidencePacket, Memory, MemoryClock, Origin, RecallPurpose,
    RecallQuery, Sensitivity, SystemMemoryClock, TrustDomain, WriteContext,
};
pub use optimus_skills::{SkillDraft, SkillRegistry, SkillView};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

pub use optimus_runtime::CancellationToken;

pub use browser::{
    best_effector, chrome_binary_path, http_effector, page_to_tool_json, try_cdp_effector,
    BrowserEffector, BrowserError, BrowserLink, BrowserPage, BrowserState,
    HttpBrowserSession as BrowserSession,
};
pub use browser_coord::{
    BrowserCoordBus, BrowserTrustDomain, CoordError, CoordEvent, CoordEventKind, CoordSnapshot,
    BROWSER_COORD_SCHEMA_VERSION,
};
pub use causal::{
    export_causal_document, export_causal_json, list_recent_causal_turns, load_causal_turn,
    parse_causal_query, write_causal_export, CausalExportDocument, CausalQuery, CausalQueryKind,
    CausalTurnReport, CAUSAL_EXPORT_VERSION,
};
pub use chat_approval::{ChatApprovalDecision, ChatApprovalResolution, ChatApprovalStatus};
pub use codex_oauth::{
    chatgpt_account_id_from_jwt, extract_codex_tokens_from_codex_cli,
    extract_codex_tokens_from_hermes, from_codex_responses_response, from_codex_responses_sse,
    jwt_expiring, refresh_codex_tokens, to_codex_responses_request, CodexAuthStatus,
    CodexAuthStore, CodexOAuthConfig, CodexOAuthModel, CodexTokens, DEFAULT_CODEX_BASE_URL,
};
pub use compress::{estimate_chars, CompressionConfig, COMPRESSED_MARKER};
pub use credential::{
    atomic_write_user_only, harden_user_only, verify_user_only, CredentialProtector,
    SystemCredentialProtector,
};
pub use execution::{
    ExecutionManifest, ExecutionModelCallSummary, ExecutionStatus, ExecutionStore,
    ExecutionTimingSummary, ExecutionToolCallSummary, ExecutionToolLifecycleSummary,
    PersistedToolLifecycle, ReplayClassification, ReplayReport, TimingEvent, TimingEventKind,
    EXECUTION_MANIFEST_VERSION,
};
pub use fs_sandbox::{
    is_denied_name, FsEntry, FsEntryKind, FsRoots, FsSandboxError, ReadTextResult,
};
pub use home_ops::{get_session, list_sessions, open_cron, tick_cron, SessionDetail};
pub use model_call::{apply_fast_mode, normalize_thinking_level};
pub use network_policy::{
    assert_public_http_url, assert_public_http_url_str, host_blocked, ip_blocked, socket_blocked,
    EgressError,
};
pub use openai_compat::{
    from_openai_response, to_openai_request, OpenAiCompatConfig, OpenAiCompatModel,
};
pub use optimus_agent::{
    AgentArtifactRef, AgentBudget, AgentContextRef, AgentDescriptor, AgentError, AgentFailure,
    AgentId, AgentInvocation, AgentInvocationEvent, AgentInvocationStatus, AgentInvocationStore,
    AgentPermissions, AgentRegistry, AgentRequest, AgentResult, AgentResultKind, AgentVersion,
    AGENT_REQUEST_SCHEMA_VERSION, AGENT_RESULT_SCHEMA_VERSION,
};
pub use optimus_artifacts::{
    ArtifactError, ArtifactRecord, ArtifactStore, BulkDeleteFailure, BulkDeleteResult,
};
pub use optimus_graph::PolicyMode;
/// Operator gateway + cron store (owned by `optimus-ops`).
pub use optimus_ops::{
    acknowledge_delivery, assert_public_mcp_url, builtin_surface_commands, builtin_tool_id_set,
    cancel_claim, claim_one, commands_for_surface, complete_claim, default_mock_session,
    delivery_state, drain_one, enqueue, fail_claim, gateway_status, http_mock_bind,
    list_ambiguous_sends, list_inbox, list_outbox, list_outbox_receipts, load_mcp_session,
    load_telegram_config, map_mcp_offers, mark_external_send_failed, mock_http_list_tools,
    mock_stdio_list_tools, reconcile, release_claim, renew_claim, save_telegram_config,
    stdio_mock_bind, telegram_poll_once, CommandSurface, CronAttemptView, CronClaim, CronError,
    CronJob, CronStore, DrainResult, GatewayClaim, GatewayError, GatewayPaths, GatewayStatus,
    InboundMessage, MappedMcpTool, McpError, McpSessionConfig, McpToolOffer, McpTransportKind,
    MockTelegramTransport, OutboundMessage, OutboxReceipt, SurfaceCommand, TelegramConfig,
    TelegramError, TelegramPollResult, TelegramTransport, TelegramUpdate,
};
pub use optimus_packs::ToolDesc as ToolSchema;
pub use optimus_workflow::{
    adapt_campaign_status, adapt_cron_attempt_status, adapt_gateway_status, adapt_job_status,
    builtin_agent_permission_ceiling, builtin_workflow_adapters, cancel_workflow_run,
    cancel_write_file_handoff, content_sha256, get_workflow_run, open_seeded_agent_registry,
    open_seeded_workflow_registry, open_workflow_run_store, read_file_handoff_workflow,
    run_read_file_handoff, run_registered_workflow, run_write_file_handoff,
    run_write_then_read_handoff, vertical_workspace, workspace_reader_descriptor,
    workspace_writer_descriptor, write_file_handoff_workflow, write_then_read_handoff_workflow,
    AdapterCapability, AdapterLifecycleStatus, ApprovalPolicy, CancellationPolicy,
    CapabilitySupport, ReadFileHandoffRequest, RetryPolicy, RollbackPolicy,
    WorkflowAdapterDescriptor, WorkflowAdapterKind, WorkflowAgentRef, WorkflowDagReport,
    WorkflowDagRequest, WorkflowDefinition, WorkflowError, WorkflowId, WorkflowNode,
    WorkflowNodeRun, WorkflowNodeRunStatus, WorkflowObservability, WorkflowPort, WorkflowRegistry,
    WorkflowRun, WorkflowRunChild, WorkflowRunEvent, WorkflowRunLease, WorkflowRunStatus,
    WorkflowRunStore, WorkflowTerminalKind, WorkflowTerminalPolicy, WorkflowTrigger,
    WorkflowVersion, WriteFileHandoffReport, WriteFileHandoffRequest,
    READ_FILE_HANDOFF_WORKFLOW_ID, READ_FILE_HANDOFF_WORKFLOW_VERSION, WORKFLOW_SCHEMA_VERSION,
    WORKSPACE_READER_ID, WORKSPACE_READER_VERSION, WORKSPACE_WRITER_ID, WORKSPACE_WRITER_VERSION,
    WRITE_FILE_HANDOFF_WORKFLOW_ID, WRITE_FILE_HANDOFF_WORKFLOW_VERSION,
    WRITE_THEN_READ_HANDOFF_WORKFLOW_ID, WRITE_THEN_READ_HANDOFF_WORKFLOW_VERSION,
};
pub use product_settings::{ProductSettings, WorkIsolationMode};
pub use profile::{
    create_default_profiles, ProfileError, ProfileHome, ProfileId, ProfileRegistryView,
    ProfileStore,
};
pub use project_authority::{
    ProjectAuthorityStore, ProjectRootSelection, ProjectScope, PROJECT_AUTHORITY_VERSION,
};
pub use routing::{
    is_known_codex_model, provider_catalog, provider_catalog_status, resolve_route,
    resolve_route_traced, route_decision_count, sanitize_codex_oauth_model, ModelCapability,
    ModelId, PrivacyPolicy, ProviderCatalogStatus, ProviderConnectState, ProviderDescriptor,
    ProviderId, RouteDecision, RouteRequest, RouteSurface, RouteTelemetryPolicy,
    CODEX_MODEL_CATALOG, DEFAULT_CODEX_MODEL,
};
pub use scripted::ScriptedModel;
pub use security_denial::{classify_security_denial, kernel_or_security_code, SecurityDenialCode};
pub use session::{
    ListFilter, SessionEffectLink, SessionMeta, SessionStore, TurnRecord, TurnStatus,
};
pub(crate) use {model_call::pack_names, tool_report::*};

pub use telemetry::{
    record_route_telemetry, route_telemetry_aggregate, RouteTelemetryAggregate,
    RouteTelemetryObservation, RouteTelemetryOutcome, MAX_TELEMETRY_LATENCY_MILLIS,
    MAX_TELEMETRY_SAMPLES,
};
pub use trace::{
    SpanId, SpanStatus, TraceContext, TraceEvent, TraceEventKind, TraceId, TraceSpan, TraceStore,
};
pub use web_search::{
    canonicalize_provenance_url, web_search, web_search_json, SearchError, SearchHit,
    WEB_SEARCH_EXTRACT_SCHEMA_VERSION,
};

#[derive(Debug, Error)]
pub enum KernelError {
    #[error("runtime: {0}")]
    Runtime(#[from] optimus_runtime::RuntimeError),
    #[error("memory: {0}")]
    Memory(#[from] optimus_memory::MemoryError),
    #[error("skills: {0}")]
    Skills(#[from] optimus_skills::SkillError),
    #[error("packs: {0}")]
    Packs(#[from] optimus_packs::PackError),
    #[error("agent: {0}")]
    Agent(#[from] optimus_agent::AgentError),
    #[error("workflow: {0}")]
    Workflow(#[from] optimus_workflow::WorkflowError),
    #[error("artifact: {0}")]
    Artifact(#[from] optimus_artifacts::ArtifactError),
    #[error("model: {0}")]
    Model(String),
    #[error("tool: {0}")]
    Tool(String),
    #[error("max steps exceeded ({0})")]
    MaxSteps(u32),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("uuid: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("browser: {0}")]
    Browser(#[from] browser::BrowserError),
    #[error("turn cancelled")]
    Cancelled,
    #[error("cron lease ownership was lost for {job_id}")]
    CronLeaseLost { job_id: Uuid },
    #[error("cron lease expired for {job_id}")]
    CronLeaseExpired { job_id: Uuid },
    #[error("cron: {0}")]
    Cron(String),
    #[error("gateway: {0}")]
    Gateway(#[from] optimus_ops::GatewayError),
}

impl From<optimus_ops::CronError> for KernelError {
    fn from(error: optimus_ops::CronError) -> Self {
        match error {
            optimus_ops::CronError::LeaseLost { job_id } => Self::CronLeaseLost { job_id },
            optimus_ops::CronError::LeaseExpired { job_id } => Self::CronLeaseExpired { job_id },
            other => Self::Cron(other.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, KernelError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    #[default]
    User,
    System,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSchema>,
    /// Reasoning effort: low | medium | high | xhigh | max | ultra (None = omit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// Prefer faster completions when true (may lower effort floor).
    #[serde(default)]
    pub fast_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompletionResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

/// Incremental events during a model completion (for UI streaming).
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// UTF-8 text fragment of the assistant answer.
    TextDelta(String),
    /// Reasoning/thinking fragment — never mixed into assistant answer text.
    ThinkingDelta(String),
    /// Versioned, runtime-owned lifecycle state for one stable tool call.
    Tool(Box<ToolLifecycleEvent>),
    /// Soft status line for the UI (e.g. "thinking").
    Status(String),
    /// Typed monotonic timing evidence for the active turn.
    Timing(TimingEvent),
}

pub const TOOL_LIFECYCLE_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolLifecyclePhase {
    Started,
    ApprovalRequired,
    Succeeded,
    Failed,
    Cancelled,
    Suppressed,
    Ambiguous,
}

impl ToolLifecyclePhase {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::ApprovalRequired => "approval_required",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Suppressed => "suppressed",
            Self::Ambiguous => "ambiguous",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolLifecycleEvent {
    pub schema_version: u16,
    pub event_id: String,
    pub run_id: String,
    pub call_id: String,
    pub tool_id: ToolId,
    pub phase: ToolLifecyclePhase,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<ToolOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval: Option<ToolApprovalBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolApprovalBinding {
    pub run_id: Uuid,
    pub call_id: String,
    pub tool_id: ToolId,
    pub job_id: optimus_runtime::JobId,
    pub node_id: Uuid,
    pub node_index: u32,
    pub effect_sha256: String,
    pub summary: String,
}

/// Control returned by a streaming consumer after each event delivery attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamControl {
    Continue,
    Cancel,
}

pub(crate) fn check_cancellation(token: &CancellationToken) -> Result<()> {
    if token.is_cancelled() {
        Err(KernelError::Cancelled)
    } else {
        Ok(())
    }
}

pub trait ModelProvider {
    fn complete(&mut self, request: CompletionRequest) -> Result<CompletionResponse>;

    fn identity(&self) -> (String, String) {
        ("custom".into(), "unknown".into())
    }

    /// Streaming completion. Default: one-shot `complete` then a single TextDelta.
    fn complete_streaming(
        &mut self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<CompletionResponse> {
        let resp = self.complete(request)?;
        if let Some(t) = &resp.text {
            if !t.is_empty() {
                sink(StreamEvent::TextDelta(t.clone()));
            }
        }
        Ok(resp)
    }

    fn complete_streaming_cancellable(
        &mut self,
        request: CompletionRequest,
        sink: &mut dyn FnMut(StreamEvent),
        cancellation: &CancellationToken,
    ) -> Result<CompletionResponse> {
        check_cancellation(cancellation)?;
        let response = self.complete_streaming(request, sink)?;
        check_cancellation(cancellation)?;
        Ok(response)
    }
}

#[derive(Debug, Clone)]
pub struct KernelConfig {
    /// Model round-trips one turn may take, spanning any approval pause.
    pub max_steps: u32,
    pub max_tool_calls_per_step: usize,
    pub pack_budget: PackBudgetConfig,
    pub memory_ctx: WriteContext,
    pub compression: CompressionConfig,
    /// Reasoning effort: low|medium|high|xhigh|max|ultra (None or "off" = omit).
    pub thinking_level: Option<String>,
    pub fast_mode: bool,
    /// SmartDeny by default; unrestricted is an explicit user/test choice.
    pub effect_policy: optimus_graph::PolicyMode,
    /// Per-turn ADR-0044 profile; ReviewChanges unless the surface asks.
    pub autonomy_profile: optimus_graph::AutonomyProfile,
    /// Overrides product-settings command FS envelope; `None` → settings.json work_isolation.
    pub command_fs_envelope: Option<optimus_graph::CommandFsEnvelope>,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            // ADR-0047: eight starved real turns; a retry that cannot happen
            // is the same as no retry.
            max_steps: 32,
            max_tool_calls_per_step: 8,
            pack_budget: PackBudgetConfig::default(),
            memory_ctx: WriteContext {
                tenant: "local".into(),
                user: "user".into(),
                agent: "optimus".into(),
                project: "default".into(),
                principal: "user:local".into(),
                max_trust: TrustDomain::User,
                max_sensitivity: Sensitivity::Personal,
            },
            compression: CompressionConfig::default(),
            thinking_level: None,
            fast_mode: false,
            effect_policy: optimus_graph::PolicyMode::SmartDeny,
            autonomy_profile: optimus_graph::AutonomyProfile::ReviewChanges,
            command_fs_envelope: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnResult {
    pub assistant_text: String,
    pub steps: u32,
    pub tool_trace: Vec<String>,
    pub invoked_tools: Vec<ToolId>,
    pub trace_context: TraceContext,
    pub schema_tokens_final: u32,
    pub loaded_packs: Vec<String>,
    /// True if extractive compression ran at least once this turn.
    pub compressed: bool,
    pub timings: TurnTimings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TurnTimings {
    pub total_ms: u64,
    pub first_response_ms: Option<u64>,
    pub model_ms: u64,
    pub tool_ms: u64,
}

#[derive(Debug, Default)]
struct TimingAccumulator {
    first_response_ms: Option<u64>,
    model_ms: u64,
    tool_ms: u64,
}

const HARD_MAX_TOOL_CALLS_PER_STEP: usize = 64;

#[derive(Debug, Clone, Copy)]
struct RecordedExecution {
    manifest_id: Uuid,
    trace_context: TraceContext,
}

pub struct Kernel {
    pub config: KernelConfig,
    pub packs: CapabilitySession,
    pub runtime: Runtime,
    pub memory: Memory,
    pub skills: SkillRegistry,
    pub messages: Vec<Message>,
    workspace: PathBuf,
    browser: Option<Box<dyn browser::BrowserEffector>>,
    home: PathBuf,
    session_id: Uuid,
    session_title: String,
    sessions: SessionStore,
    executions: ExecutionStore,
    project_roots: Vec<PathBuf>,
}

impl Kernel {
    pub fn open(home: impl AsRef<Path>, config: KernelConfig) -> Result<Self> {
        Self::open_session(home, config, None)
    }

    /// Open a new session or resume an existing one by id.
    pub fn open_session(
        home: impl AsRef<Path>,
        config: KernelConfig,
        session_id: Option<Uuid>,
    ) -> Result<Self> {
        Self::open_session_with_project(home, config, session_id, None)
    }

    /// Open a session with filesystem and effect authority bound to a durable project scope.
    pub fn open_project_session(
        home: impl AsRef<Path>,
        config: KernelConfig,
        session_id: Option<Uuid>,
        project_id: &str,
    ) -> Result<Self> {
        Self::open_session_with_project(home, config, session_id, Some(project_id))
    }

    fn open_session_with_project(
        home: impl AsRef<Path>,
        mut config: KernelConfig,
        session_id: Option<Uuid>,
        project_id: Option<&str>,
    ) -> Result<Self> {
        let home = home.as_ref().to_path_buf();
        std::fs::create_dir_all(&home)?;
        let (workspace, project_roots) = if let Some(project_id) = project_id {
            let scope = ProjectAuthorityStore::open(&home)?
                .scope(project_id)?
                .ok_or_else(|| {
                    KernelError::Tool(format!(
                        "project {project_id} has no runtime-authorized root"
                    ))
                })?;
            config.memory_ctx.project = project_id.to_string();
            (scope.primary_root, scope.roots)
        } else {
            let workspace = home.join("workspace");
            (workspace.clone(), vec![workspace])
        };
        std::fs::create_dir_all(&workspace)?;
        let command_fs_envelope = match config.command_fs_envelope {
            Some(envelope) => envelope,
            None => ProductSettings::load(&home)?
                .work_isolation
                .command_fs_envelope(),
        };
        let runtime_config = optimus_graph::RuntimeConfig {
            policy: config.effect_policy,
            command_fs_envelope,
            autonomy_profile: config.autonomy_profile,
        };
        let runtime =
            Runtime::open_with_config(&home.join("optimus.db"), &workspace, runtime_config)?;
        let memory = Memory::open(home.join("memory.db"))?;
        let skills = SkillRegistry::open(home.join("skills.db"))?;
        let sessions = SessionStore::open(home.join("sessions.db"))?;
        let executions = ExecutionStore::open(home.join("execution.db"))?;
        let mut packs = CapabilitySession::new(config.pack_budget.clone())?;

        let (session_id, session_title, messages) = if let Some(id) = session_id {
            let (pack_names, messages, title, _repaired) =
                sessions.load_bound_transcript(id, project_id)?;
            let pack_ids: Vec<PackId> = pack_names
                .iter()
                .map(|name| PackId::parse(name).ok_or_else(|| PackError::UnknownPack(name.clone())))
                .collect::<std::result::Result<_, _>>()?;
            packs.restore_loaded(&pack_ids)?;
            (id, title, messages)
        } else {
            let id = sessions.create_scoped("session", project_id)?;
            let system = Message {
                role: Role::System,
                content: system_prompt::system_prompt(
                    &packs,
                    &skills.list(false).unwrap_or_default(),
                    command_fs_envelope,
                ),
                tool_call_id: None,
                name: None,
            };
            let messages = vec![system];
            sessions.save(id, "session", &pack_names(&packs), &messages)?;
            (id, "session".into(), messages)
        };

        Ok(Self {
            config,
            packs,
            runtime,
            memory,
            skills,
            messages,
            workspace,
            browser: None,
            home,
            session_id,
            session_title,
            sessions,
            executions,
            project_roots,
        })
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    pub fn session_title(&self) -> &str {
        &self.session_title
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Durable session projection store (for offline eval / operator inspection).
    pub fn session_store(&self) -> &SessionStore {
        &self.sessions
    }

    /// Durable execution manifest store (for offline eval / operator inspection).
    pub fn execution_store(&self) -> &ExecutionStore {
        &self.executions
    }

    pub fn set_title(&mut self, title: impl Into<String>) -> Result<()> {
        self.session_title = title.into();
        self.save_session()
    }

    pub fn save_session(&self) -> Result<()> {
        self.sessions.save(
            self.session_id,
            &self.session_title,
            &pack_names(&self.packs),
            &self.messages,
        )
    }

    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    pub fn turn(&mut self, model: &mut dyn ModelProvider, user_text: &str) -> Result<TurnResult> {
        let cancellation = CancellationToken::new();
        self.turn_with_sink_cancellable(model, user_text, &mut |_| {}, &cancellation)
    }

    /// Same as [`Self::turn`] but forwards model/tool stream events to `sink` for live UI.
    pub fn turn_with_sink(
        &mut self,
        model: &mut dyn ModelProvider,
        user_text: &str,
        sink: &mut dyn FnMut(StreamEvent),
    ) -> Result<TurnResult> {
        let cancellation = CancellationToken::new();
        self.turn_with_sink_cancellable(model, user_text, sink, &cancellation)
    }

    /// Stream a turn while allowing the consumer to cancel when delivery is lost.
    ///
    /// The first [`StreamControl::Cancel`] sets the same cooperative token observed
    /// by providers and tool-loop boundaries. Later events are not forwarded.
    pub fn turn_with_controlled_sink(
        &mut self,
        model: &mut dyn ModelProvider,
        user_text: &str,
        sink: &mut dyn FnMut(StreamEvent) -> StreamControl,
    ) -> Result<TurnResult> {
        let cancellation = CancellationToken::new();
        self.turn_with_controlled_sink_cancellable(model, user_text, sink, &cancellation)
    }

    /// Stream a turn with consumer control and a caller-owned cancellation token.
    pub fn turn_with_controlled_sink_cancellable(
        &mut self,
        model: &mut dyn ModelProvider,
        user_text: &str,
        sink: &mut dyn FnMut(StreamEvent) -> StreamControl,
        cancellation: &CancellationToken,
    ) -> Result<TurnResult> {
        let delivery_cancellation = cancellation.clone();
        let mut adapting_sink = |event| {
            if delivery_cancellation.is_cancelled() {
                return;
            }
            if sink(event) == StreamControl::Cancel {
                delivery_cancellation.cancel();
            }
        };
        self.turn_with_sink_cancellable(model, user_text, &mut adapting_sink, cancellation)
    }

    pub fn turn_with_sink_cancellable(
        &mut self,
        model: &mut dyn ModelProvider,
        user_text: &str,
        sink: &mut dyn FnMut(StreamEvent),
        cancellation: &CancellationToken,
    ) -> Result<TurnResult> {
        check_cancellation(cancellation)?;
        if self.sessions.active_turn(self.session_id)?.is_some() {
            return Err(KernelError::Model(
                "session has an interrupted turn; resume it before starting another".into(),
            ));
        }
        let start_message_count = self.messages.len();
        // Auto-title from first user line.
        if self.session_title == "session" {
            let t: String = user_text.chars().take(48).collect();
            if !t.trim().is_empty() {
                self.session_title = t;
            }
        }
        self.messages.push(Message {
            role: Role::User,
            content: user_text.into(),
            tool_call_id: None,
            name: None,
        });
        // Refresh system prompt pack waist note on each turn start (new segment).
        if let Some(sys) = self.messages.first_mut() {
            if sys.role == Role::System {
                let idx = self.skills.list(false).unwrap_or_default();
                sys.content = system_prompt::system_prompt(
                    &self.packs,
                    &idx,
                    self.runtime.command_fs_envelope(),
                );
            }
        }

        let turn_id = self.sessions.begin_turn(
            self.session_id,
            &self.session_title,
            &pack_names(&self.packs),
            &self.messages,
            start_message_count,
        )?;
        let execution = self.begin_execution_manifest(turn_id, model, user_text)?;
        self.run_recorded_turn(model, sink, cancellation, turn_id, execution)
    }

    pub fn resume_pending_turn(&mut self, model: &mut dyn ModelProvider) -> Result<TurnResult> {
        let cancellation = CancellationToken::new();
        self.resume_pending_turn_with_sink(model, &mut |_| {}, &cancellation)
    }

    pub fn resume_pending_turn_with_sink(
        &mut self,
        model: &mut dyn ModelProvider,
        sink: &mut dyn FnMut(StreamEvent),
        cancellation: &CancellationToken,
    ) -> Result<TurnResult> {
        check_cancellation(cancellation)?;
        let turn = self.sessions.active_turn(self.session_id)?.ok_or_else(|| {
            KernelError::Model("session has no interrupted turn to resume".into())
        })?;
        if self.messages.len() < turn.accepted_message_count {
            return Err(KernelError::Model(
                "interrupted turn transcript is shorter than its accepted boundary".into(),
            ));
        }
        let execution = if let Some(manifest_id) = self.executions.find_by_turn(turn.id)? {
            let manifest = self.executions.manifest(manifest_id)?;
            if manifest.session_id != self.session_id || manifest.turn_id != turn.id {
                return Err(KernelError::Model(
                    "interrupted execution manifest identity does not match session turn".into(),
                ));
            }
            if manifest.status != ExecutionStatus::Running {
                return Err(KernelError::Model(
                    "interrupted execution manifest is already terminal".into(),
                ));
            }
            let trace_context = self.executions.trace_context(manifest_id)?.ok_or_else(|| {
                KernelError::Model(
                    "interrupted execution manifest is missing trace evidence".into(),
                )
            })?;
            RecordedExecution {
                manifest_id,
                trace_context,
            }
        } else {
            let prompt = self
                .messages
                .iter()
                .rev()
                .find(|message| message.role == Role::User)
                .map(|message| message.content.clone())
                .ok_or_else(|| {
                    KernelError::Model("interrupted turn has no accepted user segment".into())
                })?;
            self.begin_execution_manifest(turn.id, model, &prompt)?
        };
        if self
            .executions
            .has_pending_chat_approval(execution.manifest_id)?
        {
            return Err(KernelError::Model(
                "interrupted turn is awaiting exact in-transcript approval resolution".into(),
            ));
        }
        self.run_recorded_turn(model, sink, cancellation, turn.id, execution)
    }

    /// Run a single-node project-bound file effect job (SmartDeny + workspace hash).
    fn run_project_file_job(
        &mut self,
        label: String,
        node_label: &str,
        effect: Effect,
    ) -> Result<String> {
        let job = self.runtime.create_job(JobSpec {
            label,
            budget: Default::default(),
            nodes: vec![NodeSpec {
                label: node_label.into(),
                effect,
            }],
        })?;
        let status = self.runtime.run_all(job)?;
        if status == optimus_runtime::JobStatus::AwaitingApproval {
            let node_index = self
                .runtime
                .list_pending_approvals()?
                .into_iter()
                .find(|pending| pending.job_id == job)
                .and_then(|pending| pending.node_index)
                .unwrap_or(0);
            return Err(optimus_runtime::RuntimeError::NeedsApproval {
                job_id: job,
                node_index,
            }
            .into());
        }
        Ok(json!({
            "ok": status == optimus_runtime::JobStatus::Succeeded,
            "job": job.to_string(),
            "status": format!("{status:?}")
        })
        .to_string())
    }

    /// Seed a memory claim for demos/tests.
    pub fn remember_demo(&self, subject: &str, predicate: &str, object: &str) -> Result<Uuid> {
        use optimus_memory::ClaimDraft;
        Ok(self.memory.remember(
            &self.config.memory_ctx,
            ClaimDraft {
                subject: subject.into(),
                predicate: predicate.into(),
                object: object.into(),
                valid_from: "2026-01-01T00:00:00Z".into(),
                valid_to: None,
                confidence: 0.9,
                origin: Origin::UserStatement,
                learned_at: Some("2026-01-01T00:00:00Z".into()),
                sensitivity: Sensitivity::Personal,
                retention_until: None,
            },
        )?)
    }
}

/// Product runtime constitution. Separate from repository development `AGENTS.md`.
const OPTIMUS_RUNTIME_AGENTS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../OPTIMUS_AGENTS.md"
));

/// ADR-0040: publish agent-domain navigation onto the host coordination bus.
/// Best-effort — coord I/O must never fail the tool turn.
fn record_agent_browser_coord(home: &Path, tool_json: &str, fallback_url: &str) {
    let Ok(mut bus) = BrowserCoordBus::open(home) else {
        return;
    };
    let v: Value = serde_json::from_str(tool_json).unwrap_or(Value::Null);
    let title = v
        .get("title")
        .or_else(|| v.get("page_title"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    let final_url = v
        .get("final_url")
        .or_else(|| v.get("url"))
        .and_then(|u| u.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_url.to_string());
    if final_url.is_empty() {
        return;
    }
    let _ = bus.record_agent_navigate(&final_url, title);
}

#[cfg(test)]
mod turn_guard_tests {
    use super::*;

    #[test]
    fn web_search_signature_is_canonical_and_mutating_tools_are_excluded() {
        let first = ToolCall {
            id: "one".into(),
            name: "web_search".into(),
            arguments: json!({"query":"latest ai news","limit":5}),
        };
        let reordered = ToolCall {
            id: "two".into(),
            name: "web_search".into(),
            arguments: json!({"limit":5,"query":"latest ai news"}),
        };
        assert_eq!(
            evidence_tool_signature(&first),
            evidence_tool_signature(&reordered)
        );
        assert!(evidence_tool_signature(&ToolCall {
            id: "write".into(),
            name: "write_file".into(),
            arguments: json!({"path":"x","content":"y"}),
        })
        .is_none());
        assert!(evidence_tool_signature(&ToolCall {
            id: "snapshot".into(),
            name: "browser_snapshot".into(),
            arguments: json!({}),
        })
        .is_none());
    }

    #[test]
    fn approval_summary_keeps_command_arguments_unambiguous() {
        let effect = Effect::ProjectRunCommand {
            workspace_sha256: "0".repeat(64),
            program: "tool runner".into(),
            args: vec!["--label".into(), "two words".into()],
        };
        assert_eq!(
            exact_action_summary(&serde_json::to_string(&effect).unwrap()),
            "Run \"tool runner\" with args [\"--label\",\"two words\"]"
        );
    }
}
