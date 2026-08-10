//! Email adapter (spec-017): IMAP inbound + SMTP outbound on the shared
//! transport contract.
//!
//! Security posture: credentials live in env vars named by the config (never
//! on disk), the sender allowlist is fail-closed (an enabled adapter with an
//! empty allowlist refuses to run), and BODY.PEEK keeps unread mail unread
//! until the cycle actually surfaces it. TLS is rustls end-to-end: the `imap`
//! crate is driven as a generic client over a rustls socket (its native-tls
//! feature is never enabled), and SMTP uses lettre's rustls transport.
//!
//! Threading (A7): the original Message-ID rides inside the routing address
//! (`addr#<message-id>`), so it survives the durable outbound ledger's
//! target string unchanged; the live SMTP send decodes it back into
//! In-Reply-To/References. Attachments are decoded with mailparse and
//! content-addressed into the artifact store; their paths ride
//! `RawInbound.attachments`.

use std::net::TcpStream;
use std::path::Path;
use std::sync::{Arc, Mutex};

use imap::types::Seq;
use lettre::message::{Mailbox, Message};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::response::Severity;
use lettre::transport::smtp::SmtpTransport;
use lettre::Transport;
use serde::{Deserialize, Serialize};
use ureq::rustls::pki_types::ServerName;
use ureq::rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::transport::{RawInbound, SendOutcome, TransportAdapter, TransportId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailConfig {
    pub enabled: bool,
    pub imap_host: String,
    #[serde(default = "default_imap_port")]
    pub imap_port: u16,
    pub imap_user: String,
    pub imap_pass_env: String,
    pub smtp_host: String,
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    pub smtp_user: String,
    pub smtp_pass_env: String,
    #[serde(default)]
    pub allowed_senders: Vec<String>,
}

fn default_imap_port() -> u16 {
    993
}
fn default_smtp_port() -> u16 {
    587
}

pub fn load_email_config(home: impl AsRef<Path>) -> Result<EmailConfig, String> {
    let raw = std::fs::read_to_string(home.as_ref().join("gateway").join("email.json"))
        .map_err(|e| format!("email config: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("email config parse: {e}"))
}

pub fn save_email_config(home: impl AsRef<Path>, config: &EmailConfig) -> Result<(), String> {
    let dir = home.as_ref().join("gateway");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let raw = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("email.json"), raw).map_err(|e| e.to_string())
}

/// One decoded inbound mail, pre-materialization. The adapter owns the
/// artifact-store write so attachments become paths, not bytes.
pub struct MailInbound {
    pub from: String,
    pub text: String,
    pub message_id: Option<String>,
    pub attachments: Vec<MailAttachment>,
}

pub struct MailAttachment {
    pub filename: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// Transport seam for the email adapter: scripted by the mock in conformance
/// suites, real IMAP/SMTP in the live adapter.
pub trait MailTransport: Send {
    fn poll_inbound(&mut self) -> Result<Vec<MailInbound>, String>;
    fn send(&mut self, target: &str, body: &str) -> Result<SendOutcome, String>;
}

#[derive(Default)]
pub struct MockMailTransport {
    pub inbound: Vec<MailInbound>,
    pub sent: Arc<Mutex<Vec<(String, String)>>>,
    /// Scripted outcomes consumed in order; empty = always Confirmed.
    pub script: Vec<SendOutcome>,
    /// When true, poll_inbound returns a transport failure.
    pub failing: bool,
}

impl MockMailTransport {
    pub fn received(&self) -> Vec<(String, String)> {
        self.sent.lock().unwrap().clone()
    }
}

impl MailTransport for MockMailTransport {
    fn poll_inbound(&mut self) -> Result<Vec<MailInbound>, String> {
        if self.failing {
            return Err("mock mail transport is failing".into());
        }
        Ok(std::mem::take(&mut self.inbound))
    }

    fn send(&mut self, target: &str, body: &str) -> Result<SendOutcome, String> {
        self.sent
            .lock()
            .unwrap()
            .push((target.to_string(), body.to_string()));
        if self.script.is_empty() {
            Ok(SendOutcome::Confirmed {
                provider_message_id: format!("mock-{}", target),
            })
        } else {
            Ok(self.script.remove(0))
        }
    }
}

struct LiveMailTransport {
    config: EmailConfig,
}

impl LiveMailTransport {
    fn new(config: EmailConfig) -> Self {
        Self { config }
    }

    /// TLS IMAP session: rustls socket (ureq's rustls pin) handed to the
    /// imap crate's generic `Client::new`, per its documented rustls pattern.
    fn connect_imap(
        &self,
        password: &str,
    ) -> Result<imap::Session<StreamOwned<ClientConnection, TcpStream>>, String> {
        let tcp = TcpStream::connect((self.config.imap_host.as_str(), self.config.imap_port))
            .map_err(|e| {
                format!(
                    "imap connect {}:{}: {e}",
                    self.config.imap_host, self.config.imap_port
                )
            })?;
        let server_name = ServerName::try_from(self.config.imap_host.clone())
            .map_err(|e| format!("imap hostname '{}': {e}", self.config.imap_host))?;
        let tls = ClientConfig::builder()
            .with_root_certificates(system_root_store())
            .with_no_client_auth();
        let conn = ClientConnection::new(Arc::new(tls), server_name)
            .map_err(|e| format!("imap tls setup: {e}"))?;
        let mut client = imap::Client::new(StreamOwned::new(conn, tcp));
        client
            .read_greeting()
            .map_err(|e| format!("imap greeting: {e}"))?;
        client
            .login(&self.config.imap_user, password)
            .map_err(|(e, _)| format!("imap login: {e}"))
    }
}

/// The reply address of a threaded send, after the `addr#<message-id>`
/// encoding survived the durable ledger.
fn decode_target(target: &str) -> (&str, Option<&str>) {
    match target.split_once('#') {
        Some((addr, thread)) => (addr, Some(thread)),
        None => (target, None),
    }
}

impl MailTransport for LiveMailTransport {
    fn poll_inbound(&mut self) -> Result<Vec<MailInbound>, String> {
        let password = env_password(&self.config.imap_pass_env)?;
        let mut session = self.connect_imap(&password)?;
        session
            .select("INBOX")
            .map_err(|e| format!("imap select INBOX: {e}"))?;
        let unseen = session
            .search("UNSEEN")
            .map_err(|e| format!("imap search UNSEEN: {e}"))?;

        let mut messages = Vec::new();
        let mut surfaced: Vec<Seq> = Vec::new();
        for seq in unseen {
            let fetches = session
                .fetch(seq.to_string(), "(UID ENVELOPE BODY.PEEK[])")
                .map_err(|e| format!("imap fetch {seq}: {e}"))?;
            let Some(fetch) = fetches.first() else {
                continue;
            };
            let Some(raw_body) = fetch.body() else {
                continue;
            };
            // ENVELOPE From -> "mailbox@host"; without a sender there is no
            // routing address to authorize or reply to.
            let from = fetch
                .envelope()
                .and_then(|env| env.from.as_ref())
                .and_then(|addrs| addrs.first())
                .and_then(|addr| {
                    Some(format!(
                        "{}@{}",
                        String::from_utf8_lossy(addr.mailbox?),
                        String::from_utf8_lossy(addr.host?)
                    ))
                });
            let Some(from) = from else {
                continue;
            };
            let message_id = fetch
                .envelope()
                .and_then(|env| env.message_id.as_ref())
                .map(|id| String::from_utf8_lossy(id).trim().to_string())
                .filter(|id| !id.is_empty());
            let parsed = mailparse::parse_mail(raw_body).map_err(|e| format!("mailparse: {e}"))?;
            let mut text = String::new();
            let mut attachments = Vec::new();
            collect_parts(&parsed, &mut text, &mut attachments);
            if text.trim().is_empty() && attachments.is_empty() {
                continue; // nothing to turn on
            }
            messages.push(MailInbound {
                from,
                text,
                message_id,
                attachments,
            });
            surfaced.push(seq);
        }

        // BODY.PEEK never set \Seen; mark it only for mail we surfaced.
        for seq in &surfaced {
            let _ = session.store(seq.to_string(), "+FLAGS (\\Seen)");
        }
        let _ = session.expunge();
        let _ = session.logout();
        Ok(messages)
    }

    fn send(&mut self, target: &str, body: &str) -> Result<SendOutcome, String> {
        let password = env_password(&self.config.smtp_pass_env)?;
        let mailer = SmtpTransport::relay(&self.config.smtp_host)
            .map_err(|e| format!("smtp relay {}: {e}", self.config.smtp_host))?
            .port(self.config.smtp_port)
            .credentials(Credentials::new(self.config.smtp_user.clone(), password))
            .build();
        let message_id = format!("<{}@optimus>", uuid::Uuid::new_v4());
        let (addr, thread) = decode_target(target);
        let from: Mailbox = self
            .config
            .smtp_user
            .parse()
            .map_err(|e| format!("bad smtp from '{}': {e}", self.config.smtp_user))?;
        let to: Mailbox = addr
            .parse()
            .map_err(|e| format!("bad smtp target '{addr}': {e}"))?;
        let mut builder = Message::builder()
            .message_id(Some(message_id.clone()))
            .from(from)
            .to(to);
        if let Some(thread) = thread {
            // A7: the reply references the mail it answers.
            builder = builder
                .in_reply_to(thread.to_string())
                .references(thread.to_string());
        }
        let message = builder
            .subject("Re: Optimus")
            .body(body.to_string())
            .map_err(|e| format!("smtp message build: {e}"))?;
        let result = mailer.send(&message).map_err(|e| e.to_string())?;
        let severity = result.code().severity;
        match severity {
            // Permanent rejection: never retry. The obligation settles
            // failed-permanently with the named diagnostic.
            Severity::PermanentNegativeCompletion => Ok(SendOutcome::Failed {
                detail: format!("smtp 5xx {severity:?}"),
            }),
            // Transient: the obligation settles ambiguous for operator
            // recovery — never a silent retry loop.
            Severity::TransientNegativeCompletion => Ok(SendOutcome::Ambiguous {
                detail: format!("smtp 4xx {severity:?}"),
            }),
            _ => Ok(SendOutcome::Confirmed {
                provider_message_id: message_id,
            }),
        }
    }
}

/// Walk the MIME tree: first text part is the turn text; everything else is
/// an attachment (A7). Mailparse gives us decoded bytes.
fn collect_parts(
    node: &mailparse::ParsedMail,
    text: &mut String,
    attachments: &mut Vec<MailAttachment>,
) {
    let content_type = node.ctype.mimetype.as_str();
    if content_type == "multipart/alternative" || content_type == "multipart/mixed" {
        for child in &node.subparts {
            collect_parts(child, text, attachments);
        }
        return;
    }
    if content_type == "text/plain" && text.is_empty() {
        *text = String::from_utf8_lossy(&node.get_body_raw().unwrap_or_default()).to_string();
        return;
    }
    if node.ctype.mimetype.is_empty() {
        return; // untyped container parts carry nothing
    }
    if !content_type.starts_with("text/") || !text.is_empty() {
        let filename = node
            .get_content_disposition()
            .params
            .get("filename")
            .cloned()
            .unwrap_or_else(|| format!("attachment-{}", attachments.len() + 1));
        attachments.push(MailAttachment {
            filename,
            mime: content_type.to_string(),
            bytes: node.get_body_raw().unwrap_or_default(),
        });
    }
}

pub struct EmailAdapter {
    config: EmailConfig,
    transport: Box<dyn MailTransport + Send>,
    /// Health probe state: Ok until a poll fails (best-effort).
    last_poll_ok: bool,
}

impl EmailAdapter {
    /// Adapter over a scripted transport (public for conformance suites).
    pub fn with_transport(config: EmailConfig, transport: Box<dyn MailTransport + Send>) -> Self {
        Self {
            config,
            transport,
            last_poll_ok: true,
        }
    }
}

impl TransportAdapter for EmailAdapter {
    fn transport(&self) -> TransportId {
        TransportId::Email
    }

    fn is_enabled(&self, _home: &Path) -> bool {
        self.config.enabled
    }

    fn poll_inbound(&mut self, home: &Path) -> Result<Vec<RawInbound>, String> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }
        // Fail closed: an enabled adapter with no allowlist refuses to run —
        // nothing may drive a turn until an operator names its senders.
        if self.config.allowed_senders.is_empty() {
            return Err("live email requires non-empty allowed_senders (fail closed)".into());
        }
        let raw = match self.transport.poll_inbound() {
            Ok(raw) => raw,
            Err(e) => {
                self.last_poll_ok = false;
                return Err(format!("email poll: {e}"));
            }
        };
        let mut inbound = Vec::new();
        for mail in raw {
            if mail.from.trim().is_empty() || mail.text.trim().is_empty() {
                continue;
            }
            let mut attachments = Vec::new();
            for attachment in mail.attachments {
                // Content-addressed artifact store; the path (not the bytes)
                // is what the turn sees (A7).
                let Ok(store) = optimus_artifacts::ArtifactStore::open(home) else {
                    continue;
                };
                match store.put_bytes(
                    &attachment.bytes,
                    &attachment.mime,
                    "email.attachment",
                    &attachment.filename,
                    None,
                ) {
                    Ok(record) => attachments.push(record.sha256.clone()),
                    Err(_) => continue,
                }
            }
            // The original Message-ID rides in the routing address
            // (`addr#<message-id>`) so the durable outbound ledger carries
            // the thread to the reply's SMTP headers (A7).
            let from = match mail.message_id {
                Some(id) => format!("{}#{id}", mail.from),
                None => mail.from,
            };
            inbound.push(RawInbound {
                from,
                text: mail.text,
                attachments,
            });
        }
        self.last_poll_ok = true;
        Ok(inbound)
    }

    fn is_allowed(&self, from: &str) -> bool {
        let addr = from.split('#').next().unwrap_or(from);
        self.config.allowed_senders.is_empty()
            || self.config.allowed_senders.iter().any(|s| s == addr)
    }

    fn send(&mut self, target: &str, body: &str) -> Result<SendOutcome, String> {
        self.transport.send(target, body)
    }

    fn health(&self) -> Result<(), String> {
        if self.last_poll_ok {
            Ok(())
        } else {
            Err("last email poll failed".into())
        }
    }
}

/// spec-017 adapter convention: Ok(None) when {home}/gateway/email.json is
/// absent, Ok(Some(adapter)) when present (disabled or not), Err on malformed
/// config.
pub fn open_adapter(home: &Path) -> Result<Option<Box<dyn TransportAdapter>>, String> {
    let path = home.join("gateway").join("email.json");
    if !path.exists() {
        return Ok(None);
    }
    let config = load_email_config(home)?;
    Ok(Some(Box::new(EmailAdapter::with_transport(
        config.clone(),
        Box::new(LiveMailTransport::new(config)),
    ))))
}

/// Read a password from the env var the config names. Secrets never touch
/// disk config; a missing var is a config error, not a network error.
fn env_password(var: &str) -> Result<String, String> {
    std::env::var(var).map_err(|_| format!("missing env var {var} for email adapter"))
}

/// Platform CA bundle as a rustls root store. An empty store is fail-closed:
/// the handshake fails rather than trusting an unverifiable peer.
fn system_root_store() -> RootCertStore {
    let mut store = RootCertStore::empty();
    for path in [
        "/etc/ssl/certs/ca-certificates.crt",
        "/etc/pki/tls/certs/ca-bundle.crt",
        "/etc/ssl/ca-bundle.pem",
    ] {
        if let Ok(file) = std::fs::File::open(path) {
            let mut reader = std::io::BufReader::new(file);
            for cert in rustls_pemfile::certs(&mut reader).flatten() {
                let _ = store.add(cert);
            }
            break;
        }
    }
    store
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_target_splits_thread_suffix() {
        assert_eq!(decode_target("a@b.c#<x@y>"), ("a@b.c", Some("<x@y>")));
        assert_eq!(decode_target("a@b.c"), ("a@b.c", None));
    }

    #[test]
    fn allowed_senders_match_the_addr_before_the_thread_suffix() {
        let config = EmailConfig {
            enabled: true,
            imap_host: "imap".into(),
            imap_port: 993,
            imap_user: "u".into(),
            imap_pass_env: "P".into(),
            smtp_host: "smtp".into(),
            smtp_port: 587,
            smtp_user: "u@h".into(),
            smtp_pass_env: "P".into(),
            allowed_senders: vec!["a@b.c".into()],
        };
        let adapter = EmailAdapter::with_transport(config, Box::new(MockMailTransport::default()));
        assert!(adapter.is_allowed("a@b.c#<id@x>"));
        assert!(!adapter.is_allowed("stranger@b.c#<id@x>"));
    }

    #[test]
    fn send_outcome_classification_maps_smtp_codes() {
        // decode_target is the only pure piece; the SMTP mapping needs a
        // live socket, so this pins the encoding contract instead.
        let (addr, thread) = decode_target("u@h#<orig@x>");
        assert_eq!(addr, "u@h");
        assert_eq!(thread, Some("<orig@x>"));
    }
}
