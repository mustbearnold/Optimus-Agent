use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod doctor;
mod gateway_http;

use clap::{Parser, Subcommand};
use optimus_eval::{
    compare_evaluation_reports, run_offline_trajectory_suite, run_priority2_offline_evaluation,
    CandidateBinding, EvaluationReportV1, EvaluationResourceMeasurement, MetricThreshold,
    MAX_EVALUATION_DATASET_BYTES,
};
use optimus_graph::PolicyMode;
use optimus_kernel::{
    acknowledge_delivery, device_code_login, drain_one, enqueue, gateway_status,
    list_ambiguous_sends, list_inbox, list_outbox, list_outbox_receipts, list_recent_causal_turns,
    list_sessions, load_causal_turn, load_telegram_config, open_cron, open_seeded_agent_registry,
    open_seeded_workflow_registry, parse_causal_query, resolve_route, run_read_file_handoff,
    write_causal_export,
    run_write_file_handoff, run_write_then_read_handoff, sanitize_codex_oauth_model, tick_cron,
    BrowserSession, CodexAuthStore, CodexOAuthConfig, CodexOAuthModel, CompletionResponse, Kernel,
    KernelConfig, OpenAiCompatConfig, OpenAiCompatModel, ProviderId, ReadFileHandoffRequest,
    RouteRequest, RouteSurface, ScriptedModel, ToolCall, WriteFileHandoffRequest,
};
use optimus_packs::{builtin_catalog, CapabilitySession, PackId};
use optimus_runtime::{
    CampaignStepSpec, CampaignStore, Effect, JobSpec, NodeSpec, Runtime, StepKind,
};
use optimus_skills::{Permission, SkillDraft, SkillRegistry};
use serde_json::json;

const OPTIMUS_VERSION_MANIFEST: &str =
    include_str!("../../../docs/architecture/optimus-version.json");

#[derive(Parser, Debug)]
#[command(name = "optimus", version, about = "Optimus Agent CLI")]
struct Cli {
    /// Path to Optimus home (db + default workspace)
    #[arg(long, global = true, default_value = ".optimus")]
    home: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show product version, tracked Hermes target, and verified parity version
    Version {
        #[arg(long)]
        json: bool,
    },
    /// Durability inventory: multi-DB schema/quarantine + backup set (P18)
    Doctor {
        #[command(subcommand)]
        cmd: Option<DoctorCmd>,
        /// Emit machine-readable JSON
        #[arg(long, global = true)]
        json: bool,
    },
    /// Run the Phase 0 golden multi-node job in a workspace
    Demo {
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Recover interrupted jobs and resume one job id (uuid)
    Resume {
        job_id: String,
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Recover crashed nodes and resume every resumable job
    ResumeAll {
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Skills 2.0 registry
    Skills {
        #[command(subcommand)]
        cmd: SkillsCmd,
    },
    /// Progressive capability packs
    Packs {
        #[command(subcommand)]
        cmd: PacksCmd,
    },
    /// Offline scripted turn (no network model)
    ChatOffline {
        /// User message
        message: String,
        /// Demo mode: seed memory + run memory_recall script
        #[arg(long)]
        demo_memory: bool,
        /// Resume session uuid
        #[arg(long)]
        session: Option<String>,
    },
    /// Live chat (openai-compat API key or --provider codex OAuth)
    Chat {
        message: String,
        /// Provider: openai (default) | codex
        #[arg(long, default_value = "openai")]
        provider: String,
        /// Override model
        #[arg(long)]
        model: Option<String>,
        /// Override base URL (openai provider only)
        #[arg(long)]
        base_url: Option<String>,
        /// Resume session uuid
        #[arg(long)]
        session: Option<String>,
        /// Thinking level: off|minimal|low|medium|high|xhigh|max|ultra
        #[arg(long)]
        thinking: Option<String>,
        /// Prefer lower-latency effort cap
        #[arg(long, default_value_t = false)]
        fast: bool,
    },
    /// List durable chat sessions
    Sessions,
    /// Authentication (Codex OAuth, …)
    Auth {
        #[command(subcommand)]
        cmd: AuthCmd,
    },
    /// Durable cron schedules (interval jobs)
    Cron {
        #[command(subcommand)]
        cmd: CronCmd,
    },
    /// HTTP browser effector (SSRF-safe)
    Browse {
        #[command(subcommand)]
        cmd: BrowseCmd,
    },
    /// SmartDeny approvals
    Approvals {
        #[command(subcommand)]
        cmd: ApprovalsCmd,
    },
    /// Work Graph jobs
    Jobs {
        #[command(subcommand)]
        cmd: JobsCmd,
    },
    /// Local operator gateway (durable inbox/outbox)
    Gateway {
        #[command(subcommand)]
        cmd: GatewayCmd,
    },
    /// Deterministic trajectory eval (offline suite)
    Eval {
        #[command(subcommand)]
        cmd: EvalCmd,
    },
    /// Multi-agent durable campaigns (Work Graph)
    Campaign {
        #[command(subcommand)]
        cmd: CampaignCmd,
    },
    /// Built-in multi-agent verticals (P10 DAG + specialists)
    Vertical {
        #[command(subcommand)]
        cmd: VerticalCmd,
    },
    /// Reconstruct durable turn causality (Phase 5; stores, not logs)
    Trace {
        #[command(subcommand)]
        cmd: TraceCmd,
    },
}

#[derive(Subcommand, Debug)]
enum DoctorCmd {
    /// List the process-local backup path set for this home
    BackupList,
}

#[derive(Subcommand, Debug)]
enum SkillsCmd {
    /// List non-deprecated skills
    List {
        #[arg(long)]
        all: bool,
    },
    /// Create a candidate skill
    Create {
        name: String,
        /// Skill body text
        body: String,
        /// Comma-separated permissions: fs,terminal,net,browser,memory_write
        #[arg(long, default_value = "fs")]
        perms: String,
        #[arg(long)]
        pin: bool,
    },
    /// Resolve best skill by name
    Resolve { name: String },
}

#[derive(Subcommand, Debug)]
enum PacksCmd {
    /// List built-in packs and schema token costs
    List,
    /// Show core waist vs sample progressive load budget
    DemoBudget,
}

#[derive(Subcommand, Debug)]
enum AuthCmd {
    /// Codex OAuth (ChatGPT backend)
    Codex {
        #[command(subcommand)]
        cmd: CodexAuthCmd,
    },
}

#[derive(Subcommand, Debug)]
enum CodexAuthCmd {
    /// Show Optimus-stored Codex credential status (no secrets)
    Status,
    /// Import tokens from Hermes auth.json (read-only)
    ImportHermes,
    /// Import tokens from ~/.codex/auth.json (read-only)
    ImportCli,
    /// Interactive device-code login (Optimus-owned session)
    Login,
    /// Remove Optimus Codex credentials
    Logout,
}

#[derive(Subcommand, Debug)]
enum CronCmd {
    /// List scheduled jobs
    List,
    /// Add interval job (every N seconds)
    Add {
        name: String,
        #[arg(long, default_value_t = 3600)]
        every: u64,
        prompt: String,
        #[arg(long, default_value = "offline")]
        provider: String,
    },
    /// Enable/disable by id
    SetEnabled {
        id: String,
        #[arg(long)]
        enabled: bool,
    },
    /// Remove by id
    Remove { id: String },
    /// Run all due jobs once
    Tick,
    /// Loop: tick cron forever (operator daemon)
    Serve {
        #[arg(long, default_value_t = 5)]
        interval: u64,
        /// Also drain gateway inbox each loop
        #[arg(long)]
        with_gateway: bool,
        /// Max loops (0 = forever)
        #[arg(long, default_value_t = 0)]
        max_loops: u64,
    },
}

#[derive(Subcommand, Debug)]
enum BrowseCmd {
    /// GET url and print title/text/links
    Navigate { url: String },
    /// Show last loaded page snapshot
    Snapshot,
    /// Click link by index from last page
    Click { index: usize },
}

#[derive(Subcommand, Debug)]
enum ApprovalsCmd {
    /// List jobs waiting on SmartDeny
    List,
    /// Grant job-scoped approval and resume
    Grant { job_id: String },
}

#[derive(Subcommand, Debug)]
enum JobsCmd {
    /// List all jobs
    List,
    /// Resume one job
    Resume { job_id: String },
    /// Create a SmartDeny-gated RunCommand job and step once (usually blocks on approval)
    SubmitCommand {
        /// Program to run
        program: String,
        /// Arguments
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
enum GatewayCmd {
    /// Enqueue a local inbound message (simulates channel webhook)
    Send {
        text: String,
        #[arg(long, default_value = "local")]
        channel: String,
        #[arg(long, default_value = "offline")]
        provider: String,
        #[arg(long)]
        session: Option<String>,
    },
    /// List durable inbox
    Inbox,
    /// List recent outbox
    Outbox {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Process one inbox message through Kernel
    Drain,
    /// Drain all pending inbox messages
    DrainAll,
    /// HTTP webhook server (127.0.0.1 only)
    Serve {
        #[arg(long, default_value_t = 8788)]
        port: u16,
        /// Exit after N requests (0 = forever). Useful for deterministic tests.
        #[arg(long, default_value_t = 0)]
        max_requests: u64,
    },
    /// List succeeded outbox rows missing a local delivery receipt
    Ambiguous {
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Record a local delivery receipt after operator confirms external handoff
    Ack {
        message_id: String,
        outbound_id: String,
    },
    /// Show gateway inbox/outbox/ambiguous counts (doctor-friendly)
    Status,
    /// Telegram adapter status (config-gated; no secrets printed)
    Telegram,
}

#[derive(Subcommand, Debug)]
enum EvalCmd {
    /// Run built-in offline trajectory suite
    Run {
        #[arg(long)]
        json: bool,
    },
    /// Produce the exact ten-case candidate report from explicit JSON evidence
    Report {
        #[arg(long)]
        binding: PathBuf,
        #[arg(long)]
        measurements: PathBuf,
        #[arg(long)]
        thresholds: Option<PathBuf>,
    },
    /// Compare two exact reports without changing evaluation state
    Compare {
        #[arg(long)]
        baseline: PathBuf,
        #[arg(long)]
        candidate: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum CampaignCmd {
    /// List campaigns
    List,
    /// Create campaign from write steps: path=contents (repeatable)
    Create {
        name: String,
        /// Step as relative_path=contents
        #[arg(long = "write", value_name = "PATH=CONTENTS")]
        writes: Vec<String>,
        /// Optional command step: program then args (one step)
        #[arg(long)]
        cmd: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        cmd_arg: Vec<String>,
    },
    /// Show campaign + steps
    Status { id: String },
    /// Run or resume campaign
    Run { id: String },
    /// Inspect schema and persisted campaign integrity without executing work
    Diagnose {
        #[arg(long)]
        json: bool,
    },
    /// Deterministically repair projection drift, then report unresolved issues
    Repair {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
enum TraceCmd {
    /// Show one causal turn report by trace:, manifest:, turn:, or bare trace UUID
    Show {
        /// Identity: bare UUID (trace), or prefixed trace:|manifest:|turn:
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// List recent execution manifests (newest first)
    Recent {
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Export versioned local causal JSON (P14; store-backed, redacted home path)
    Export {
        /// Identity: bare UUID (trace), or prefixed trace:|manifest:|turn:
        id: String,
        /// Output path for optimus.causal.v1 JSON
        #[arg(long, short = 'o')]
        out: std::path::PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum VerticalCmd {
    /// List seeded built-in specialists and workflows
    List,
    /// Run write_file_handoff → workspace_writer vertical
    WriteFile {
        /// Relative path under the Optimus workspace
        #[arg(long)]
        path: String,
        /// File contents (UTF-8)
        #[arg(long)]
        contents: String,
        /// Auto-grant SmartDeny for the exact WriteFile effect
        #[arg(long, default_value_t = false)]
        auto_grant: bool,
        /// Runtime policy: smart_deny (default) or unrestricted
        #[arg(long, default_value = "smart_deny")]
        policy: String,
        #[arg(long)]
        json: bool,
    },
    /// Run read_file_handoff → workspace_reader vertical
    ReadFile {
        /// Relative path under the Optimus workspace (must already exist)
        #[arg(long)]
        path: String,
        #[arg(long)]
        json: bool,
    },
    /// Run write_then_read_handoff DAG (workspace_writer → workspace_reader)
    WriteThenRead {
        #[arg(long)]
        path: String,
        #[arg(long)]
        contents: String,
        #[arg(long, default_value_t = false)]
        auto_grant: bool,
        #[arg(long, default_value = "smart_deny")]
        policy: String,
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parse_policy_mode(policy: &str) -> Result<PolicyMode, Box<dyn std::error::Error>> {
    match policy.to_ascii_lowercase().as_str() {
        "smart_deny" | "smartdeny" | "deny" => Ok(PolicyMode::SmartDeny),
        "unrestricted" | "open" => Ok(PolicyMode::Unrestricted),
        other => Err(format!("unknown policy {other}; use smart_deny or unrestricted").into()),
    }
}

fn parse_perms(s: &str) -> Result<Vec<Permission>, String> {
    let mut out = Vec::new();
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        out.push(match part {
            "fs" | "fs_workspace" => Permission::FsWorkspace,
            "terminal" => Permission::Terminal,
            "net" => Permission::Net,
            "browser" => Permission::Browser,
            "memory_write" => Permission::MemoryWrite,
            other => return Err(format!("unknown permission: {other}")),
        });
    }
    Ok(out)
}

fn read_bounded_json<T: serde::de::DeserializeOwned>(
    path: &Path,
    label: &str,
) -> Result<T, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_EVALUATION_DATASET_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > MAX_EVALUATION_DATASET_BYTES {
        return Err(format!("{label} JSON size is outside policy").into());
    }
    serde_json::from_slice(&bytes).map_err(Into::into)
}

fn run_read_only_eval(cli: &Cli) -> Option<Result<(), Box<dyn std::error::Error>>> {
    let Commands::Eval {
        cmd: EvalCmd::Compare {
            baseline,
            candidate,
        },
    } = &cli.command
    else {
        return None;
    };
    Some((|| {
        let baseline: EvaluationReportV1 = read_bounded_json(baseline, "baseline report")?;
        let candidate: EvaluationReportV1 = read_bounded_json(candidate, "candidate report")?;
        let comparison = compare_evaluation_reports(&baseline, &candidate)?;
        println!("{}", serde_json::to_string_pretty(&comparison)?);
        Ok(())
    })())
}

fn embedded_version_status() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let manifest: serde_json::Value = serde_json::from_str(OPTIMUS_VERSION_MANIFEST)?;
    let target = manifest
        .pointer("/hermes_target/version")
        .and_then(serde_json::Value::as_str)
        .ok_or("embedded version manifest is missing hermes_target.version")?;
    let claim_status = manifest
        .pointer("/parity_claim/status")
        .and_then(serde_json::Value::as_str)
        .ok_or("embedded version manifest is missing parity_claim.status")?;
    let parity_version = if claim_status == "verified" {
        manifest
            .pointer("/parity_claim/hermes_version")
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    };
    let feature_contracts = manifest
        .pointer("/baseline/feature_count")
        .and_then(serde_json::Value::as_u64)
        .ok_or("embedded version manifest is missing baseline.feature_count")?;
    Ok(json!({
        "product": "Optimus Agent",
        "product_version": env!("CARGO_PKG_VERSION"),
        "hermes_target_version": target,
        "hermes_parity_version": parity_version,
        "parity_claim_status": claim_status,
        "frozen_hermes_feature_contracts": feature_contracts,
    }))
}

fn run_read_only_version(cli: &Cli) -> Option<Result<(), Box<dyn std::error::Error>>> {
    let Commands::Version { json: as_json } = &cli.command else {
        return None;
    };
    Some((|| {
        let status = embedded_version_status()?;
        if *as_json {
            println!("{}", serde_json::to_string_pretty(&status)?);
        } else {
            println!(
                "Optimus Agent {}",
                status["product_version"].as_str().unwrap_or("unknown")
            );
            println!(
                "Hermes target: {}",
                status["hermes_target_version"]
                    .as_str()
                    .unwrap_or("unknown")
            );
            println!(
                "Hermes parity: {}",
                status["hermes_parity_version"]
                    .as_str()
                    .unwrap_or("unverified")
            );
            println!(
                "Frozen Hermes feature contracts: {}",
                status["frozen_hermes_feature_contracts"]
                    .as_u64()
                    .unwrap_or(0)
            );
        }
        Ok(())
    })())
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(result) = run_read_only_version(&cli) {
        return result;
    }
    if let Some(result) = run_read_only_eval(&cli) {
        return result;
    }
    std::fs::create_dir_all(&cli.home)?;
    let db = cli.home.join("optimus.db");
    let skills_db = cli.home.join("skills.db");
    match cli.command {
        Commands::Version { .. } => unreachable!("version is handled before opening Optimus state"),
        Commands::Doctor { cmd, json } => match cmd {
            None => {
                // Read-only: never migrate or create DBs during diagnosis.
                let report = doctor::inventory(&cli.home, env!("CARGO_PKG_VERSION"));
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    doctor::print_inventory_text(&report);
                    let version_status = embedded_version_status()?;
                    println!(
                        "hermes: target={} parity={} contracts={}",
                        version_status["hermes_target_version"]
                            .as_str()
                            .unwrap_or("unknown"),
                        version_status["hermes_parity_version"]
                            .as_str()
                            .unwrap_or("unverified"),
                        version_status["frozen_hermes_feature_contracts"]
                            .as_u64()
                            .unwrap_or(0),
                    );
                    let s = CapabilitySession::with_defaults();
                    println!(
                        "packs: core_schema_tokens={} max_budget=2500 max_on_demand=2",
                        s.schema_tokens()
                    );
                }
                if report.issues.is_empty() {
                    Ok(())
                } else {
                    Err("doctor found durability issues (see report)".into())
                }
            }
            Some(DoctorCmd::BackupList) => {
                let list = doctor::backup_list(&cli.home);
                if json {
                    println!("{}", serde_json::to_string_pretty(&list)?);
                } else {
                    doctor::print_backup_list_text(&list);
                }
                Ok(())
            }
        },
        Commands::Demo { workspace } => {
            let workspace = workspace.unwrap_or_else(|| cli.home.join("workspace"));
            let rt = Runtime::open(&db, &workspace)?;
            let job = rt.create_job(JobSpec {
                label: "demo".into(),
                budget: Default::default(),
                nodes: vec![
                    NodeSpec {
                        label: "write-hello".into(),
                        effect: Effect::WriteFile {
                            relative_path: "hello.txt".into(),
                            contents: "hello from optimus\n".into(),
                        },
                    },
                    NodeSpec {
                        label: "verify-hello".into(),
                        effect: Effect::AssertFileEquals {
                            relative_path: "hello.txt".into(),
                            expected: "hello from optimus\n".into(),
                        },
                    },
                    NodeSpec {
                        label: "write-done".into(),
                        effect: Effect::WriteFile {
                            relative_path: "done.marker".into(),
                            contents: "ok\n".into(),
                        },
                    },
                ],
            })?;
            let status = rt.run_all(job)?;
            println!("job {job} => {status:?}");
            println!("workspace: {}", workspace.display());
            Ok(())
        }
        Commands::Resume { job_id, workspace } => {
            let workspace = workspace.unwrap_or_else(|| cli.home.join("workspace"));
            let rt = Runtime::open(&db, &workspace)?;
            let recovered = rt.recover_crashed_running()?;
            println!("recovered: {recovered:?}");
            let id = uuid::Uuid::parse_str(&job_id)?;
            let status = rt.resume(optimus_graph::JobId(id))?;
            println!("job {job_id} => {status:?}");
            Ok(())
        }
        Commands::ResumeAll { workspace } => {
            let workspace = workspace.unwrap_or_else(|| cli.home.join("workspace"));
            let rt = Runtime::open(&db, &workspace)?;
            let results = rt.resume_all()?;
            for (id, status) in results {
                println!("job {id} => {status:?}");
            }
            Ok(())
        }
        Commands::Skills { cmd } => {
            let reg = SkillRegistry::open(&skills_db)?;
            match cmd {
                SkillsCmd::List { all } => {
                    for s in reg.list(all)? {
                        println!(
                            "{} v{} {:?} uses={} rate={:.2} perms={:?}",
                            s.name, s.version, s.status, s.uses, s.success_rate, s.permissions
                        );
                    }
                    Ok(())
                }
                SkillsCmd::Create {
                    name,
                    body,
                    perms,
                    pin,
                } => {
                    let permissions = parse_perms(&perms)?;
                    let id = reg.create(SkillDraft {
                        name,
                        body,
                        permissions,
                        pin,
                    })?;
                    let s = reg.get(id)?;
                    println!("created {} v{} {:?} id={id}", s.name, s.version, s.status);
                    Ok(())
                }
                SkillsCmd::Resolve { name } => {
                    match reg.resolve(&name)? {
                        Some(s) => {
                            println!(
                                "{} v{} {:?} id={} rate={:.2}",
                                s.name, s.version, s.status, s.id, s.success_rate
                            );
                            println!("{}", s.body);
                        }
                        None => println!("no skill named {name}"),
                    }
                    Ok(())
                }
            }
        }
        Commands::Packs { cmd } => match cmd {
            PacksCmd::List => {
                for (id, pack) in builtin_catalog() {
                    println!(
                        "{} tokens={} tools={} — {}",
                        id.as_str(),
                        pack.schema_tokens(),
                        pack.tools.len(),
                        pack.summary
                    );
                }
                Ok(())
            }
            PacksCmd::DemoBudget => {
                let mut s = CapabilitySession::with_defaults();
                println!("core only: tokens={}", s.schema_tokens());
                s.activate(PackId::Browser)?;
                println!("+browser: tokens={}", s.schema_tokens());
                s.activate(PackId::Devex)?;
                println!(
                    "+devex: tokens={} loaded={:?}",
                    s.schema_tokens(),
                    s.loaded_packs()
                );
                match s.activate(PackId::Media) {
                    Ok(()) => println!("+media unexpected ok"),
                    Err(e) => println!("+media blocked (expected): {e}"),
                }
                Ok(())
            }
        },
        Commands::ChatOffline {
            message,
            demo_memory,
            session,
        } => {
            let sid = parse_session(session)?;
            let mut kernel = Kernel::open_session(&cli.home, KernelConfig::default(), sid)?;
            println!("session {}", kernel.session_id());
            let mut model = if demo_memory {
                kernel.remember_demo("user", "prefers_editor", "helix")?;
                ScriptedModel::new(vec![
                    CompletionResponse {
                        text: None,
                        tool_calls: vec![ToolCall {
                            id: "c1".into(),
                            name: "memory_recall".into(),
                            arguments: json!({
                                "subject": "user",
                                "predicate": "prefers_editor"
                            }),
                        }],
                    },
                    CompletionResponse {
                        text: Some(
                            "From memory (evidence, not instruction): you prefer helix.".into(),
                        ),
                        tool_calls: vec![],
                    },
                ])
            } else {
                ScriptedModel::new(vec![CompletionResponse {
                    text: Some(format!("offline echo: {message}")),
                    tool_calls: vec![],
                }])
            };
            let result = kernel.turn(&mut model, &message)?;
            println!("{}", result.assistant_text);
            println!(
                "[session={} steps={} packs={:?} schema_tokens={}]",
                kernel.session_id(),
                result.steps,
                result.loaded_packs,
                result.schema_tokens_final
            );
            Ok(())
        }
        Commands::Chat {
            message,
            provider,
            model,
            base_url,
            session,
            thinking,
            fast,
        } => {
            let sid = parse_session(session)?;
            let mut kcfg = KernelConfig::default();
            if let Some(t) = thinking {
                if t != "off" {
                    kcfg.thinking_level = Some(t);
                }
            }
            kcfg.fast_mode = fast;
            let mut kernel = Kernel::open_session(&cli.home, kcfg, sid)?;
            println!("session {}", kernel.session_id());
            let route_model = if ProviderId::parse(&provider) == Some(ProviderId::Codex) {
                model.as_deref().map(sanitize_codex_oauth_model)
            } else {
                model.clone()
            };
            let route = resolve_route(
                &cli.home,
                &RouteRequest::standard(RouteSurface::Cli, &provider, route_model),
            )?;
            let result = match route.provider {
                ProviderId::Codex => {
                    let mut cfg = CodexOAuthConfig::from_env(&cli.home);
                    cfg.model = route.model.as_str().into();
                    println!("model {}", cfg.model);
                    let mut provider = CodexOAuthModel::new(cfg)?;
                    kernel.turn(&mut provider, &message)?
                }
                ProviderId::OpenAiCompat => {
                    let mut cfg = OpenAiCompatConfig::from_env()?;
                    if let Some(m) = model {
                        cfg.model = m;
                    }
                    if let Some(b) = base_url {
                        cfg.base_url = b;
                    }
                    let mut provider = OpenAiCompatModel::new(cfg);
                    kernel.turn(&mut provider, &message)?
                }
                ProviderId::Offline => return Err("offline is not supported by live chat".into()),
            };
            println!("{}", result.assistant_text);
            if !result.tool_trace.is_empty() {
                println!("[tools: {}]", result.tool_trace.join(" | "));
            }
            println!(
                "[provider={provider} session={} steps={} packs={:?} schema_tokens={} compressed={}]",
                kernel.session_id(),
                result.steps,
                result.loaded_packs,
                result.schema_tokens_final,
                result.compressed
            );
            Ok(())
        }
        Commands::Sessions => {
            for s in list_sessions(&cli.home)? {
                println!(
                    "{}  msgs={}  packs={:?}  {}",
                    s.id, s.message_count, s.packs, s.title
                );
            }
            Ok(())
        }
        Commands::Auth { cmd } => match cmd {
            AuthCmd::Codex { cmd } => {
                let store = CodexAuthStore::open(&cli.home)?;
                match cmd {
                    CodexAuthCmd::Status => {
                        let s = store.status()?;
                        println!("codex auth path: {}", store.path().display());
                        println!(
                            "present={} expiring={} has_refresh={} mode={} base={} account={}",
                            s.present,
                            s.access_expiring,
                            s.has_refresh,
                            s.source_note,
                            s.base_url,
                            s.account_id.as_deref().unwrap_or("-")
                        );
                    }
                    CodexAuthCmd::ImportHermes => {
                        println!("{}", store.import_from_hermes()?);
                    }
                    CodexAuthCmd::ImportCli => {
                        println!("{}", store.import_from_codex_cli()?);
                    }
                    CodexAuthCmd::Login => {
                        device_code_login(&store)?;
                    }
                    CodexAuthCmd::Logout => {
                        store.clear()?;
                        println!("Codex credentials cleared from Optimus home.");
                    }
                }
                Ok(())
            }
        },
        Commands::Cron { cmd } => {
            let store = open_cron(&cli.home)?;
            match cmd {
                CronCmd::List => {
                    for j in store.list()? {
                        println!(
                            "{}  every={}s  enabled={}  next={}  last={:?}  {}  [{}] {}",
                            j.id,
                            j.every_secs,
                            j.enabled,
                            j.next_run_unix,
                            j.last_status,
                            j.name,
                            j.provider,
                            j.prompt
                        );
                    }
                }
                CronCmd::Add {
                    name,
                    every,
                    prompt,
                    provider,
                } => {
                    let j = store.add(&name, every, &prompt, &provider)?;
                    println!("created {} next_run_unix={}", j.id, j.next_run_unix);
                }
                CronCmd::SetEnabled { id, enabled } => {
                    let id = uuid::Uuid::parse_str(&id)?;
                    store.set_enabled(id, enabled)?;
                    println!("updated {id} enabled={enabled}");
                }
                CronCmd::Remove { id } => {
                    let id = uuid::Uuid::parse_str(&id)?;
                    let ok = store.remove(id)?;
                    println!("removed={ok} {id}");
                }
                CronCmd::Tick => {
                    let rows = tick_cron(&cli.home)?;
                    if rows.is_empty() {
                        println!("no due cron jobs");
                    } else {
                        for r in rows {
                            println!("{}", serde_json::to_string(&r)?);
                        }
                    }
                }
                CronCmd::Serve {
                    interval,
                    with_gateway,
                    max_loops,
                } => {
                    let mut loops = 0u64;
                    loop {
                        let rows = tick_cron(&cli.home)?;
                        for r in rows {
                            println!("[cron] {}", serde_json::to_string(&r)?);
                        }
                        if with_gateway {
                            loop {
                                match drain_gateway_once(&cli.home)? {
                                    None => break,
                                    Some(r) => println!(
                                        "[gateway] {} {} {}",
                                        r.id, r.status, r.reply_preview
                                    ),
                                }
                            }
                        }
                        loops += 1;
                        if max_loops > 0 && loops >= max_loops {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_secs(interval.max(1)));
                    }
                }
            }
            Ok(())
        }
        Commands::Browse { cmd } => {
            let workspace = cli.home.join("workspace");
            std::fs::create_dir_all(&workspace)?;
            let mut browser = BrowserSession::open(&workspace)?;
            match cmd {
                BrowseCmd::Navigate { url } => {
                    let page = browser.navigate(&url)?;
                    println!("status={} title={}", page.status, page.title);
                    println!("final_url={}", page.final_url);
                    println!("--- text ---");
                    println!("{}", page.text.chars().take(2000).collect::<String>());
                    println!("--- links ({}) ---", page.links.len());
                    for l in page.links.iter().take(20) {
                        println!("[{}] {} -> {}", l.index, l.text, l.href);
                    }
                }
                BrowseCmd::Snapshot => {
                    let page = browser.snapshot()?;
                    println!("status={} title={}", page.status, page.title);
                    println!("{}", page.text.chars().take(4000).collect::<String>());
                }
                BrowseCmd::Click { index } => {
                    let page = browser.click(index)?;
                    println!("clicked -> {} title={}", page.final_url, page.title);
                    println!("{}", page.text.chars().take(1500).collect::<String>());
                }
            }
            Ok(())
        }
        Commands::Approvals { cmd } => {
            let workspace = cli.home.join("workspace");
            let rt = Runtime::open(&db, &workspace)?;
            match cmd {
                ApprovalsCmd::List => {
                    let pending = rt.list_pending_approvals()?;
                    if pending.is_empty() {
                        println!("no pending approvals");
                    }
                    for p in pending {
                        println!(
                            "{}  {}  status={:?}  node={:?}  grant={}  effect={}",
                            p.job_id,
                            p.job_label,
                            p.job_status,
                            p.node_label,
                            p.has_grant,
                            p.effect_json.chars().take(80).collect::<String>()
                        );
                    }
                }
                ApprovalsCmd::Grant { job_id } => {
                    let id = uuid::Uuid::parse_str(&job_id)?;
                    let status = rt.grant_and_resume(optimus_runtime::job_id(id))?;
                    println!("job {job_id} => {status:?}");
                }
            }
            Ok(())
        }
        Commands::Jobs { cmd } => {
            let workspace = cli.home.join("workspace");
            let rt = Runtime::open(&db, &workspace)?;
            match cmd {
                JobsCmd::List => {
                    for j in rt.list_jobs_summary()? {
                        println!(
                            "{}  {}  {:?}  steps={}/{}",
                            j.job_id, j.label, j.status, j.steps_executed, j.max_steps
                        );
                    }
                }
                JobsCmd::Resume { job_id } => {
                    let id = uuid::Uuid::parse_str(&job_id)?;
                    let status = rt.resume(optimus_runtime::job_id(id))?;
                    println!("job {job_id} => {status:?}");
                }
                JobsCmd::SubmitCommand { program, args } => {
                    let job = rt.create_job(JobSpec {
                        label: format!("cmd:{program}"),
                        budget: Default::default(),
                        nodes: vec![NodeSpec {
                            label: "run".into(),
                            effect: Effect::RunCommand {
                                program: program.clone(),
                                args: args.clone(),
                            },
                        }],
                    })?;
                    match rt.run_next(job) {
                        Ok(out) => {
                            println!(
                                "job {} ran node {} status={:?}",
                                job, out.node_index, out.node_status
                            );
                        }
                        Err(optimus_runtime::RuntimeError::NeedsApproval {
                            job_id,
                            node_index,
                        }) => {
                            println!(
                                "job {job_id} awaiting approval (node {node_index}) — grant with: optimus approvals grant {job_id}"
                            );
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
            }
            Ok(())
        }
        Commands::Gateway { cmd } => match cmd {
            GatewayCmd::Send {
                text,
                channel,
                provider,
                session,
            } => {
                let m = enqueue(&cli.home, &channel, &text, &provider, session.as_deref())?;
                println!("enqueued {} channel={}", m.id, m.channel);
                Ok(())
            }
            GatewayCmd::Inbox => {
                let rows = list_inbox(&cli.home)?;
                if rows.is_empty() {
                    println!("inbox empty");
                }
                for m in rows {
                    println!(
                        "{}  [{}]  {}  {}",
                        m.id,
                        m.channel,
                        m.provider,
                        m.text.chars().take(80).collect::<String>()
                    );
                }
                Ok(())
            }
            GatewayCmd::Outbox { limit } => {
                let rows = list_outbox(&cli.home, limit)?;
                if rows.is_empty() {
                    println!("outbox empty");
                }
                for m in rows {
                    println!(
                        "{}  reply_to={}  {}  {}",
                        m.id,
                        m.in_reply_to,
                        m.status,
                        m.text.chars().take(100).collect::<String>()
                    );
                }
                Ok(())
            }
            GatewayCmd::Drain => {
                match drain_gateway_once(&cli.home)? {
                    None => println!("inbox empty"),
                    Some(r) => println!("{} {} {}", r.id, r.status, r.reply_preview),
                }
                Ok(())
            }
            GatewayCmd::DrainAll => {
                let mut n = 0usize;
                loop {
                    match drain_gateway_once(&cli.home)? {
                        None => break,
                        Some(r) => {
                            println!("{} {} {}", r.id, r.status, r.reply_preview);
                            n += 1;
                        }
                    }
                }
                println!("drained={n}");
                Ok(())
            }
            GatewayCmd::Serve { port, max_requests } => {
                let token = std::env::var("OPTIMUS_GATEWAY_TOKEN").unwrap_or_default();
                let security = gateway_http::GatewaySecurity::new(port, token)?;
                gateway_http::run_gateway_http(cli.home.clone(), port, max_requests, security)?;
                Ok(())
            }
            GatewayCmd::Ambiguous { limit } => {
                let rows = list_ambiguous_sends(&cli.home, limit)?;
                if rows.is_empty() {
                    println!("no ambiguous sends");
                }
                for row in rows {
                    println!(
                        "{}  outbound={}  channel={}  {}",
                        row.message_id,
                        row.outbound.id,
                        row.outbound.channel,
                        row.outbound.text.chars().take(80).collect::<String>()
                    );
                    println!(
                        "  recover: optimus gateway ack {} {}",
                        row.message_id, row.outbound.id
                    );
                }
                println!(
                    "note: local receipt only — external exactly-once is not claimed"
                );
                Ok(())
            }
            GatewayCmd::Ack {
                message_id,
                outbound_id,
            } => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let ok = acknowledge_delivery(&cli.home, &message_id, &outbound_id, now)?;
                if ok {
                    println!("acked message_id={message_id} outbound_id={outbound_id} at {now}");
                } else {
                    println!("ack refused (id mismatch or message not terminal)");
                    return Ok(());
                }
                Ok(())
            }
            GatewayCmd::Status => {
                let status = gateway_status(&cli.home)?;
                println!(
                    "inbox_pending={} inbox_claimed={} outbox_total={} ambiguous_sends={}",
                    status.inbox_pending,
                    status.inbox_claimed,
                    status.outbox_total,
                    status.ambiguous_sends
                );
                println!("{}", status.note);
                // Also show recent outbox receipt flags for operator visibility.
                for row in list_outbox_receipts(&cli.home, 5)? {
                    let receipt = row
                        .delivered_unix
                        .map(|t| format!("delivered={t}"))
                        .unwrap_or_else(|| {
                            if row.ambiguous_send {
                                "AMBIGUOUS".into()
                            } else {
                                "no-receipt".into()
                            }
                        });
                    println!(
                        "  {}  {}  {}",
                        row.message_id,
                        row.outbound.status,
                        receipt
                    );
                }
                Ok(())
            }
            GatewayCmd::Telegram => {
                let cfg = load_telegram_config(&cli.home)?;
                let token_present = std::env::var(&cfg.bot_token_env)
                    .map(|v| !v.trim().is_empty())
                    .unwrap_or(false);
                println!(
                    "telegram enabled={} token_env={} token_present={} mode={}",
                    cfg.enabled,
                    cfg.bot_token_env,
                    token_present,
                    if cfg.enabled {
                        "config-gated-live"
                    } else {
                        "mock-or-disabled"
                    }
                );
                println!(
                    "allowed_chats={} note=no public listen port; local gateway is authority",
                    cfg.allowed_chat_ids.len()
                );
                Ok(())
            }
        },
        Commands::Eval { cmd } => match cmd {
            EvalCmd::Run { json } => {
                let dir = cli.home.join("eval-runs").join(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs().to_string())
                        .unwrap_or_else(|_| "run".into()),
                );
                std::fs::create_dir_all(&dir)?;
                let report = run_offline_trajectory_suite(&dir);
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    for c in &report.cases {
                        println!(
                            "{}  {}  {}",
                            if c.ok { "PASS" } else { "FAIL" },
                            c.id,
                            c.detail
                        );
                    }
                    println!(
                        "passed={} failed={} all_ok={}",
                        report.passed,
                        report.failed,
                        report.all_ok()
                    );
                }
                if !report.all_ok() {
                    return Err("eval suite failed".into());
                }
                Ok(())
            }
            EvalCmd::Report {
                binding,
                measurements,
                thresholds,
            } => {
                let binding: CandidateBinding = read_bounded_json(&binding, "binding")?;
                let measurements: Vec<EvaluationResourceMeasurement> =
                    read_bounded_json(&measurements, "measurements")?;
                let thresholds: Vec<MetricThreshold> = match thresholds {
                    Some(path) => read_bounded_json(&path, "thresholds")?,
                    None => Vec::new(),
                };
                let report = run_priority2_offline_evaluation(
                    &cli.home,
                    binding,
                    &measurements,
                    &thresholds,
                )?;
                println!("{}", serde_json::to_string_pretty(&report)?);
                if !report.passed {
                    return Err("evaluation thresholds failed".into());
                }
                Ok(())
            }
            EvalCmd::Compare { .. } => unreachable!("read-only evaluation dispatch is preflighted"),
        },
        Commands::Campaign { cmd } => {
            let store = CampaignStore::open(&cli.home)?;
            match cmd {
                CampaignCmd::List => {
                    let rows = store.list()?;
                    if rows.is_empty() {
                        println!("no campaigns");
                    }
                    for c in rows {
                        println!("{}  {:?}  {}", c.id, c.status, c.name);
                    }
                }
                CampaignCmd::Create {
                    name,
                    writes,
                    cmd,
                    cmd_arg,
                } => {
                    let mut steps = Vec::new();
                    for w in writes {
                        let (path, contents) = w
                            .split_once('=')
                            .ok_or_else(|| format!("--write expects path=contents, got {w}"))?;
                        steps.push(CampaignStepSpec {
                            label: path.to_string(),
                            kind: StepKind::WriteFile {
                                relative_path: path.into(),
                                contents: contents.into(),
                            },
                        });
                    }
                    if let Some(program) = cmd {
                        steps.push(CampaignStepSpec {
                            label: format!("cmd:{program}"),
                            kind: StepKind::RunCommand {
                                program,
                                args: cmd_arg,
                            },
                        });
                    }
                    let view = store.create(&name, steps)?;
                    println!("{} created steps={}", view.campaign.id, view.steps.len());
                }
                CampaignCmd::Status { id } => {
                    let id = uuid::Uuid::parse_str(&id)?;
                    let view = store
                        .get(id)?
                        .ok_or_else(|| format!("campaign {id} not found"))?;
                    println!(
                        "{}  {:?}  {}",
                        view.campaign.id, view.campaign.status, view.campaign.name
                    );
                    for s in view.steps {
                        println!(
                            "  [{}] {:?}  {}  job={:?}  {}",
                            s.idx, s.status, s.label, s.job_id, s.detail
                        );
                    }
                }
                CampaignCmd::Run { id } => {
                    let id = uuid::Uuid::parse_str(&id)?;
                    let view = store.run(id)?;
                    println!(
                        "{} => {:?} ({} steps)",
                        view.campaign.id,
                        view.campaign.status,
                        view.steps.len()
                    );
                    for s in view.steps {
                        println!("  [{}] {:?} {}", s.idx, s.status, s.label);
                    }
                }
                CampaignCmd::Diagnose { json } => {
                    let report = store.diagnose()?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&report)?);
                    } else {
                        println!(
                            "campaign schema={} diagnostics={}",
                            report.schema_version,
                            report.diagnostics.len()
                        );
                        for diagnostic in report.diagnostics {
                            println!(
                                "  campaign={:?} step={:?} field={} repairable={} {}",
                                diagnostic.campaign_id,
                                diagnostic.step_id,
                                diagnostic.field,
                                diagnostic.repairable,
                                diagnostic.detail
                            );
                        }
                    }
                }
                CampaignCmd::Repair { json } => {
                    let report = store.repair()?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&report)?);
                    } else {
                        println!(
                            "campaign repaired={} remaining={}",
                            report.repaired,
                            report.remaining.len()
                        );
                        for diagnostic in report.remaining {
                            println!(
                                "  campaign={:?} step={:?} field={} repairable={} {}",
                                diagnostic.campaign_id,
                                diagnostic.step_id,
                                diagnostic.field,
                                diagnostic.repairable,
                                diagnostic.detail
                            );
                        }
                    }
                }
            }
            Ok(())
        }
        Commands::Trace { cmd } => match cmd {
            TraceCmd::Show { id, json } => {
                let query = parse_causal_query(&id)?;
                let report = load_causal_turn(&cli.home, query)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    println!(
                        "manifest={} status={:?} session={} turn={}",
                        report.manifest.id,
                        report.manifest.status,
                        report.manifest.session_id,
                        report.manifest.turn_id
                    );
                    if let Some(trace) = &report.trace_context {
                        println!(
                            "trace={} span={}",
                            trace.trace_id, trace.span_id
                        );
                    }
                    println!(
                        "provider={} model={} model_calls={} tool_calls={} replay={:?}",
                        report.manifest.provider,
                        report.manifest.model,
                        report.model_calls.len(),
                        report.tool_calls.len(),
                        report.replay.classification
                    );
                    println!(
                        "timings total_ms={} first_response_ms={:?} model_ms={} tool_ms={}",
                        report.timings.total_ms,
                        report.timings.first_response_ms,
                        report.timings.model_ms,
                        report.timings.tool_ms
                    );
                    println!(
                        "effect_transcript_consistent={}",
                        report.effect_transcript_consistent
                    );
                    for call in &report.tool_calls {
                        println!(
                            "  tool {} {} suppressed={} effect={}",
                            call.call_id,
                            call.tool_id,
                            call.suppressed,
                            call.effect_sha256.as_deref().unwrap_or("-")
                        );
                    }
                }
                Ok(())
            }
            TraceCmd::Recent { limit, json } => {
                let rows = list_recent_causal_turns(&cli.home, limit)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&rows)?);
                } else if rows.is_empty() {
                    println!("no execution manifests");
                } else {
                    for row in rows {
                        println!(
                            "{}  {:?}  session={} turn={} {}/{}",
                            row.id,
                            row.status,
                            row.session_id,
                            row.turn_id,
                            row.provider,
                            row.model
                        );
                    }
                }
                Ok(())
            }
            TraceCmd::Export { id, out } => {
                let query = parse_causal_query(&id)?;
                let path = write_causal_export(&cli.home, query, &out)?;
                println!("wrote {}", path.display());
                Ok(())
            }
        },
        Commands::Vertical { cmd } => {
            match cmd {
                VerticalCmd::List => {
                    let agents =
                        open_seeded_agent_registry(cli.home.join("agent-registry.db"))?;
                    let workflows =
                        open_seeded_workflow_registry(cli.home.join("workflow-registry.db"))?;
                    println!("agents:");
                    for agent in agents.list()? {
                        println!(
                            "  {}@{} — {}",
                            agent.id.as_str(),
                            agent.version.as_str(),
                            agent.responsibility
                        );
                    }
                    println!("workflows:");
                    for workflow in workflows.list()? {
                        println!(
                            "  {}@{} — {}",
                            workflow.id.as_str(),
                            workflow.version.as_str(),
                            workflow.description
                        );
                    }
                }
                VerticalCmd::WriteFile {
                    path,
                    contents,
                    auto_grant,
                    policy,
                    json,
                } => {
                    let policy = parse_policy_mode(&policy)?;
                    let report = run_write_file_handoff(
                        &cli.home,
                        WriteFileHandoffRequest {
                            relative_path: path,
                            contents,
                            auto_grant,
                            policy,
                        },
                    )?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&report)?);
                    } else {
                        println!(
                            "workflow={}@{} terminal={:?} agent={}@{} invocation={}",
                            report.workflow_id,
                            report.workflow_version,
                            report.workflow_terminal,
                            report.agent_id,
                            report.agent_version,
                            report.invocation_id
                        );
                        if let Some(run_id) = report.run_id {
                            println!("run={run_id}");
                        }
                        if let Some(job) = report.job_id {
                            println!("job={job}");
                        }
                        println!("summary={}", report.agent_result.summary);
                        if let Some(artifact) = report.artifact {
                            println!(
                                "artifact={} bytes={}",
                                artifact.sha256, artifact.size_bytes
                            );
                        }
                    }
                }
                VerticalCmd::ReadFile { path, json } => {
                    let report = run_read_file_handoff(
                        &cli.home,
                        ReadFileHandoffRequest {
                            relative_path: path,
                        },
                    )?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&report)?);
                    } else {
                        println!(
                            "workflow={}@{} status={:?} run={}",
                            report.workflow_id,
                            report.workflow_version,
                            report.status,
                            report.run_id
                        );
                        println!("summary={}", report.summary);
                        for artifact in report.artifacts {
                            println!(
                                "artifact={} bytes={}",
                                artifact.sha256, artifact.size_bytes
                            );
                        }
                    }
                }
                VerticalCmd::WriteThenRead {
                    path,
                    contents,
                    auto_grant,
                    policy,
                    json,
                } => {
                    let policy = parse_policy_mode(&policy)?;
                    let report = run_write_then_read_handoff(
                        &cli.home,
                        WriteFileHandoffRequest {
                            relative_path: path,
                            contents,
                            auto_grant,
                            policy,
                        },
                    )?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&report)?);
                    } else {
                        println!(
                            "workflow={}@{} status={:?} run={} nodes={} children={}",
                            report.workflow_id,
                            report.workflow_version,
                            report.status,
                            report.run_id,
                            report.nodes.len(),
                            report.children.len()
                        );
                        for node in &report.nodes {
                            println!(
                                "  node={} status={} artifact={:?}",
                                node.node_id,
                                node.status.as_str(),
                                node.artifact_sha256
                            );
                        }
                        println!("summary={}", report.summary);
                    }
                }
            }
            Ok(())
        }
    }
}

fn drain_gateway_once(
    home: &std::path::Path,
) -> Result<Option<optimus_kernel::DrainResult>, Box<dyn std::error::Error>> {
    let home_buf = home.to_path_buf();
    let out = drain_one(home, |msg| {
        let mut kernel = match parse_session(msg.session_id.clone()) {
            Ok(sid) => match Kernel::open_session(&home_buf, KernelConfig::default(), sid) {
                Ok(k) => k,
                Err(e) => return Err(e.to_string()),
            },
            Err(e) => return Err(e.to_string()),
        };
        let route = match resolve_route(
            &home_buf,
            &RouteRequest::standard(RouteSurface::Gateway, &msg.provider, None),
        ) {
            Ok(route) => route,
            Err(error) => return Err(error.to_string()),
        };
        let result = match route.provider {
            ProviderId::Offline => {
                let mut model = ScriptedModel::new(vec![CompletionResponse {
                    text: Some(format!("[gateway:{}] {}", msg.channel, msg.text)),
                    tool_calls: vec![],
                }]);
                model.stream_chunks = false;
                kernel.turn(&mut model, &msg.text)
            }
            ProviderId::Codex => {
                let mut cfg = CodexOAuthConfig::from_env(&home_buf);
                cfg.model = route.model.as_str().into();
                let mut model = match CodexOAuthModel::new(cfg) {
                    Ok(model) => model,
                    Err(error) => return Err(error.to_string()),
                };
                kernel.turn(&mut model, &msg.text)
            }
            ProviderId::OpenAiCompat => {
                let cfg = match OpenAiCompatConfig::from_env() {
                    Ok(config) => config,
                    Err(error) => return Err(error.to_string()),
                };
                let mut model = OpenAiCompatModel::new(cfg);
                kernel.turn(&mut model, &msg.text)
            }
        };
        match result {
            Ok(r) => Ok((r.assistant_text, Some(kernel.session_id().to_string()))),
            Err(e) => Err(e.to_string()),
        }
    })?;
    Ok(out)
}

fn parse_session(
    session: Option<String>,
) -> Result<Option<uuid::Uuid>, Box<dyn std::error::Error>> {
    match session {
        None => Ok(None),
        Some(s) => Ok(Some(uuid::Uuid::parse_str(&s)?)),
    }
}
