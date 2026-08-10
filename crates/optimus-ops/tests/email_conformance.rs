//! spec-017 A7 conformance: the Email adapter end-to-end through the shared
//! claim→turn→settle spine, plus the A7 specifics — threading headers
//! surviving the durable ledger, and attachments landing in the artifact
//! store as content-addressed paths.
//
//! The transport under test is the scripted mock; the spine it rides is the
//! real contract (`optimus_ops::transport`).

use std::path::Path;
use std::sync::{Arc, Mutex};

use optimus_ops::adapters::email::{
    load_email_config, save_email_config, EmailAdapter, EmailConfig, MailAttachment, MailInbound,
    MockMailTransport,
};
use optimus_ops::{adapter_cycle, list_transport_events, TransportAdapter, TransportId};

fn enabled_config(allowed: &[&str]) -> EmailConfig {
    EmailConfig {
        enabled: true,
        imap_host: "imap.example".into(),
        imap_port: 993,
        imap_user: "optimus@example".into(),
        imap_pass_env: "OPTIMUS_EMAIL_IMAP_PASS".into(),
        smtp_host: "smtp.example".into(),
        smtp_port: 587,
        smtp_user: "optimus@example".into(),
        smtp_pass_env: "OPTIMUS_EMAIL_SMTP_PASS".into(),
        allowed_senders: allowed.iter().map(|s| s.to_string()).collect(),
    }
}

fn mail(from: &str, text: &str, message_id: Option<&str>) -> MailInbound {
    MailInbound {
        from: from.into(),
        text: text.into(),
        message_id: message_id.map(|s| s.to_string()),
        attachments: Vec::new(),
    }
}

#[test]
fn a7_full_cycle_delivers_threaded_reply_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let config = enabled_config(&["alice@example"]);
    save_email_config(dir.path(), &config).unwrap();
    let sent: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let mut adapter = EmailAdapter::with_transport(
        config,
        Box::new(MockMailTransport {
            inbound: vec![mail(
                "alice@example",
                "please fix the gateway",
                Some("<orig-thread@example>"),
            )],
            sent: Arc::clone(&sent),
            ..Default::default()
        }),
    );
    let reply = |message: &optimus_ops::InboundMessage| {
        assert_eq!(
            message.session_id.as_deref(),
            Some("email:alice@example#<orig-thread@example>")
        );
        Ok((
            format!("reply:{}", message.text),
            message.session_id.clone(),
        ))
    };
    let result = adapter_cycle(dir.path(), &mut adapter, reply).unwrap();
    assert_eq!(result.enqueued.len(), 1);
    assert_eq!(result.drained.len(), 1);
    assert_eq!(result.receipts.len(), 1);

    // The thread id survived the durable ledger: the send target still
    // carries `addr#<message-id>`, which the live SMTP send decodes into
    // In-Reply-To/References (pinned in email.rs's decode_target).
    let recorded = sent.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].0, "alice@example#<orig-thread@example>");
    assert_eq!(recorded[0].1, "reply:please fix the gateway");
}

#[test]
fn a7_attachment_lands_in_the_artifact_store_as_a_path() {
    let dir = tempfile::tempdir().unwrap();
    let config = enabled_config(&["bob@example"]);
    save_email_config(dir.path(), &config).unwrap();
    let mut adapter = EmailAdapter::with_transport(
        config,
        Box::new(MockMailTransport {
            inbound: vec![MailInbound {
                from: "bob@example".into(),
                text: "see the plan".into(),
                message_id: Some("<m2@example>".into()),
                attachments: vec![MailAttachment {
                    filename: "plan.txt".into(),
                    mime: "text/plain".into(),
                    bytes: b"the plan".to_vec(),
                }],
            }],
            ..Default::default()
        }),
    );
    let raw = adapter.poll_inbound(dir.path()).unwrap();
    assert_eq!(raw.len(), 1);
    assert_eq!(raw[0].attachments.len(), 1);
    // Content-addressed path under the home's artifact store (blobs/{2}/{sha}),
    // not the bytes themselves (A7).
    let sha = &raw[0].attachments[0];
    let path = Path::new(dir.path())
        .join("artifacts")
        .join("blobs")
        .join(&sha[..2])
        .join(sha);
    assert!(
        path.exists(),
        "attachment must be materialized in the artifact store"
    );
    let stored = std::fs::read(&path).unwrap();
    assert_eq!(String::from_utf8_lossy(&stored), "the plan");
}

#[test]
fn a7_unallowed_sender_is_refused_with_named_diagnostic_event() {
    let dir = tempfile::tempdir().unwrap();
    let config = enabled_config(&["alice@example"]);
    save_email_config(dir.path(), &config).unwrap();
    let mut adapter = EmailAdapter::with_transport(
        config,
        Box::new(MockMailTransport {
            inbound: vec![mail("mallory@example", "exfiltrate", None)],
            ..Default::default()
        }),
    );
    let result = adapter_cycle(dir.path(), &mut adapter, |_| {
        panic!("a refused sender must never reach a turn")
    })
    .unwrap();
    assert_eq!(result.enqueued.len(), 0);
    assert_eq!(result.refused.len(), 1);
    let events = list_transport_events(dir.path(), Some("email"), 10).unwrap();
    assert!(events.iter().any(|e| e.kind == "inbound_refused"));
    assert!(events.iter().any(|e| e.detail.contains("mallory@example")));
}

#[test]
fn config_round_trips_and_defaults_apply() {
    let config = enabled_config(&["a@b.c"]);
    let raw = serde_json::to_string(&config).unwrap();
    let back: EmailConfig = serde_json::from_str(&raw).unwrap();
    assert_eq!(config, back);

    // Ports default; hosts are required (a config without hosts is a typo).
    let partial: EmailConfig = serde_json::from_str(
        r#"{"enabled": true, "imap_host": "imap", "imap_user": "u", "imap_pass_env": "P", "smtp_host": "smtp", "smtp_user": "u", "smtp_pass_env": "P"}"#,
    )
    .unwrap();
    assert_eq!(partial.imap_port, 993);
    assert_eq!(partial.smtp_port, 587);
    assert!(partial.allowed_senders.is_empty());
}

#[test]
fn disabled_adapter_polls_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = enabled_config(&["a@b.c"]);
    config.enabled = false;
    let mut adapter = EmailAdapter::with_transport(
        config,
        Box::new(MockMailTransport {
            inbound: vec![mail("a@b.c", "x", None)],
            ..Default::default()
        }),
    );
    assert!(!adapter.is_enabled(dir.path()));
    let raw = adapter.poll_inbound(dir.path()).unwrap();
    assert!(raw.is_empty());
}

#[test]
fn load_email_config_missing_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(load_email_config(dir.path()).is_err());
    assert_eq!(TransportId::Email.as_str(), "email");
}
