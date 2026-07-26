//! Operator services for Optimus: durable gateway, cron, and surface catalogs.
//!
//! These are intentionally outside the turn-loop waist. Surfaces and the kernel
//! may depend on them; they must not depend on `optimus-kernel`.

mod channel_adapters;
mod cron;
mod gateway;
mod hermes_import;
mod mcp;
mod media_fixtures;
mod pty_session;
mod surface_commands;
mod surfaces;
mod telegram;

pub use channel_adapters::{discord_enqueue, slack_enqueue, AdapterError, ChannelInbound};
pub use cron::{CronAttemptView, CronClaim, CronError, CronJob, CronStore};
pub use gateway::{
    acknowledge_delivery, cancel_claim, claim_one, complete_claim, delivery_state, drain_one,
    enqueue, fail_claim, gateway_status, list_ambiguous_sends, list_inbox, list_outbox,
    list_outbox_receipts, mark_external_send_failed, reconcile, release_claim, renew_claim,
    DrainResult, GatewayClaim, GatewayError, GatewayPaths, GatewayStatus, InboundMessage,
    OutboundMessage, OutboxReceipt,
};
pub use hermes_import::{
    import_memory, import_sessions, import_skills, write_test_fixtures, HermesMemoryFixture,
    HermesSessionFixture, HermesSkillFixture, ImportError, ImportReport,
};
pub use mcp::{
    assert_public_mcp_url, builtin_tool_id_set, default_mock_session, http_mock_bind,
    load_mcp_session, map_mcp_offers, mock_http_list_tools, mock_stdio_list_tools, stdio_mock_bind,
    MappedMcpTool, McpError, McpSessionConfig, McpToolOffer, McpTransportKind,
};
pub use media_fixtures::{
    image_generate_offline, stt_offline, tts_offline, vision_analyze_offline, ImageGenerateRequest,
    ImageGenerateResult, MediaError, SttRequest, SttResult, TtsRequest, TtsResult,
    VisionAnalyzeRequest, VisionAnalyzeResult,
};
pub use pty_session::{PtyError, PtySessionStore, PtyTab, DEFAULT_MAX_TABS};
pub use surface_commands::{
    builtin_surface_commands, commands_for_surface, CommandSurface, SurfaceCommand,
};
pub use surfaces::{
    acp_handle, proxy_chat_offline, tui_smoke_snapshot, AcpRequest, AcpResponse, ProxyChatRequest,
    ProxyChatResponse, ProxyChoice, ProxyMessage, SurfaceError, TuiSnapshot,
};
pub use telegram::{
    load_telegram_config, poll_once as telegram_poll_once, process_inbound_reply_path,
    save_telegram_config, MockTelegramTransport, SendOutcome, TelegramConfig, TelegramError,
    TelegramPollResult, TelegramTransport, TelegramUpdate,
};
