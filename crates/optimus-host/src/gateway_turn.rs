//! One inbound gateway message, run through the host kernel and answered.
//!
//! The field this module exists for is `InboundMessage::session_id`. ADR-0071
//! decided it means exactly one thing — the **routing address** of the
//! conversation, in the `<channel>:<address>` shape every adapter mints — and
//! that a turn:
//!
//! - derives its kernel session from that address rather than parsing the
//!   address as a session (`telegram:42` is not a UUID and never was), and
//! - hands the address back unchanged, because the value the turn returns is
//!   written by the ledger as the obligation's delivery target. Returning the
//!   kernel's own session id there addressed every reply to a bare UUID no
//!   adapter could route.
//!
//! The other decision here is what a paused effect means to someone who is not
//! at this machine. SmartDeny is a durable pause, not a failure: `turn_loop`
//! propagates `NeedsApproval` so the accepted turn and its execution manifest
//! stay alive until the exact bound call resolves. `drain_one` sees only
//! `Err`, so an uncaught pause is retried — a second and third paused manifest
//! for one message — and then dead-lettered. Catching it here is what makes the
//! message terminal (law 10) while the job stays open for the operator, and the
//! reply names no job id: approval is a local capability, and the remote sender
//! is not given a handle to it.

use std::path::PathBuf;

use optimus_kernel::{
    drain_one, CodexOAuthConfig, CodexOAuthModel, CompletionResponse, DrainResult, InboundMessage,
    Kernel, KernelConfig, KernelError, OpenAiCompatConfig, OpenAiCompatModel, ProviderId,
    RouteRequest, RouteSurface, ScriptedModel, SessionStore, TurnResult,
};
use optimus_runtime::RuntimeError;
use uuid::Uuid;

use crate::chat::apply_resolved_openai_model;

/// Namespace for sessions derived from a gateway routing address.
///
/// Fixed forever: change it and every remote conversation restarts. It is
/// itself a v5 UUID over a name Optimus owns, so it is reproducible rather than
/// a magic constant — `the_namespace_is_derived_not_invented` re-derives it.
const GATEWAY_SESSION_NAMESPACE: Uuid = Uuid::from_u128(0xeac1_bdcf_0fb7_5a69_a226_a00e_79d5_4ccb);

/// What a remote sender is told when their turn stops at an approval gate.
///
/// Honest about the state and useless as a lever: it names no job, no node, and
/// no way to resolve either.
const PAUSED_REPLY: &str = "That needs approval from the operator of this machine before it can \
                            run, so I've stopped and left it waiting for them. Nothing has been \
                            done yet.";

/// The kernel session a routing address belongs to.
///
/// Deterministic on purpose (ADR-0071): the same chat resumes the same session
/// across restarts with no table to migrate and no lookup that could disagree
/// with itself. The derived id is guessable from a public address and grants
/// nothing — reading that session still needs the local store, and every effect
/// inside it still passes SmartDeny.
pub fn session_for_address(address: &str) -> Uuid {
    Uuid::new_v5(&GATEWAY_SESSION_NAMESPACE, address.as_bytes())
}

/// Run one gateway message and return `(reply, reply address)`.
///
/// The second element is the address the message arrived at, returned
/// unchanged. An absent address is an anonymous turn: a fresh session, and no
/// obligation to deliver anything (ADR-0070).
pub fn gateway_turn(
    home: &PathBuf,
    message: &InboundMessage,
) -> Result<(String, Option<String>), String> {
    let address = message.session_id.clone();
    let mut kernel = open_addressed_session(home, address.as_deref())?;
    let route = optimus_kernel::resolve_route(
        home,
        &RouteRequest::standard(RouteSurface::Gateway, &message.provider, None),
    )
    .map_err(|error| error.to_string())?;
    let turn = match route.provider {
        ProviderId::Offline => {
            let mut model = ScriptedModel::new(vec![CompletionResponse {
                text: Some(format!("[gateway:{}] {}", message.channel, message.text)),
                tool_calls: vec![],
            }]);
            model.stream_chunks = false;
            kernel.turn(&mut model, &message.text)
        }
        ProviderId::Codex => {
            let mut config = CodexOAuthConfig::from_env(home);
            config.model = route.model.as_str().into();
            let mut model = CodexOAuthModel::new(config).map_err(|error| error.to_string())?;
            kernel.turn(&mut model, &message.text)
        }
        ProviderId::OpenAiCompat => {
            let config = apply_resolved_openai_model(
                OpenAiCompatConfig::from_env().map_err(|error| error.to_string())?,
                route.model.as_str(),
            );
            let mut model = OpenAiCompatModel::new(config);
            kernel.turn(&mut model, &message.text)
        }
    };
    Ok((reply_text(turn)?, address))
}

/// Drain one gateway message through the host-owned kernel and canonical route.
pub fn drain_gateway_once(home: &PathBuf) -> Result<Option<DrainResult>, String> {
    let home_buf = home.clone();
    drain_one(home, |message| gateway_turn(&home_buf, message)).map_err(|error| error.to_string())
}

/// A finished turn's text, or the paused answer when it stopped at approval.
fn reply_text(turn: optimus_kernel::Result<TurnResult>) -> Result<String, String> {
    match turn {
        Ok(result) => Ok(result.assistant_text),
        Err(KernelError::Runtime(RuntimeError::NeedsApproval { job_id, node_index })) => {
            // The operator's console is the only place these ids appear.
            eprintln!(
                "gateway: job {job_id} node {node_index} awaiting approval — \
                 grant with: optimus approvals grant {job_id}"
            );
            Ok(PAUSED_REPLY.into())
        }
        Err(error) => Err(error.to_string()),
    }
}

/// Open the session `address` names, creating it on first contact.
///
/// A derived id exists before its row does, so the first message from a chat
/// asks for a session the store has never heard of. It is seeded from a fresh
/// kernel session rather than an empty row, because a session's opening
/// transcript is the kernel's business: the system prompt and the loaded pack
/// set come from the same code path a locally-created session uses, and the id
/// the store minted for that seed is discarded rather than left as a stray
/// conversation in the session list.
fn open_addressed_session(home: &PathBuf, address: Option<&str>) -> Result<Kernel, String> {
    let Some(address) = address else {
        return Kernel::open_session(home, KernelConfig::default(), None)
            .map_err(|error| error.to_string());
    };
    let session = session_for_address(address);
    let store = SessionStore::open(home.join("sessions.db")).map_err(|error| error.to_string())?;
    if !store.exists(session).map_err(|error| error.to_string())? {
        let seed = {
            let kernel = Kernel::open_session(home, KernelConfig::default(), None)
                .map_err(|error| error.to_string())?;
            kernel.session_id()
        };
        let (packs, messages, title) = store.load(seed).map_err(|error| error.to_string())?;
        store
            .save(session, &title, &packs, &messages)
            .map_err(|error| error.to_string())?;
        store.delete(seed).map_err(|error| error.to_string())?;
    }
    Kernel::open_session(home, KernelConfig::default(), Some(session))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_namespace_is_derived_not_invented() {
        assert_eq!(
            GATEWAY_SESSION_NAMESPACE,
            Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                b"https://optimus-agent.local/gateway/session"
            ),
            "the namespace is v5 over a name Optimus owns; it must stay reproducible"
        );
    }

    #[test]
    fn an_address_always_lands_on_the_same_session() {
        assert_eq!(
            session_for_address("telegram:42"),
            session_for_address("telegram:42")
        );
        assert_ne!(
            session_for_address("telegram:42"),
            session_for_address("telegram:43")
        );
        assert_ne!(
            session_for_address("telegram:42"),
            session_for_address("slack:42")
        );
    }

    #[test]
    fn a_derived_session_is_a_v5_uuid() {
        let session = session_for_address("telegram:42");
        assert_eq!(session.get_version_num(), 5);
        assert_ne!(session, Uuid::nil());
    }

    #[test]
    fn the_paused_reply_hands_the_sender_nothing_to_act_on() {
        let reply = PAUSED_REPLY.to_ascii_lowercase();
        for leak in ["job", "grant", "approvals ", "node", "http"] {
            assert!(
                !reply.contains(leak),
                "the paused reply must not mention {leak:?}: {PAUSED_REPLY}"
            );
        }
        assert!(reply.contains("approval"), "it must still be honest");
    }
}
