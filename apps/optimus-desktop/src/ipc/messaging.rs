//! Program P28 messaging: gateway inbox/outbox, delivery receipts, ambiguous recovery.
//!
//! Does not mint project roots or auto-grant SmartDeny. External exactly-once is
//! never claimed — doctor surfaces ambiguous local receipts only.

use std::path::PathBuf;

use optimus_kernel::{
    acknowledge_delivery, enqueue, gateway_status, list_ambiguous_sends, list_inbox,
    list_outbox_receipts, load_telegram_config,
};
use serde_json::json;

#[cfg(test)]
pub(super) fn owns(method: &str) -> bool {
    matches!(
        method,
        "gateway_status"
            | "gateway_inbox"
            | "gateway_outbox"
            | "gateway_enqueue"
            | "gateway_ambiguous"
            | "gateway_ack_delivery"
            | "gateway_telegram_status"
    )
}

pub(super) fn handle(
    home: &PathBuf,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match method {
        "gateway_status" => {
            let status = gateway_status(home).map_err(|e| e.to_string())?;
            Ok(json!({
                "status": status,
                "note": "Local SQLite is delivery authority. External exactly-once is not claimed.",
            }))
        }
        "gateway_inbox" => {
            let rows = list_inbox(home).map_err(|e| e.to_string())?;
            Ok(json!({ "messages": rows, "count": rows.len() }))
        }
        "gateway_outbox" => {
            let limit = params
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(50)
                .clamp(1, 200) as usize;
            let rows = list_outbox_receipts(home, limit).map_err(|e| e.to_string())?;
            Ok(json!({
                "messages": rows,
                "count": rows.len(),
                "note": "delivered_unix is a local adapter receipt, not external EO.",
            }))
        }
        "gateway_enqueue" => {
            let text = params
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "text required".to_string())?
                .trim();
            if text.is_empty() {
                return Err("text required".into());
            }
            if text.chars().count() > 4000 {
                return Err("text too long (max 4000)".into());
            }
            let channel = params
                .get("channel")
                .and_then(|v| v.as_str())
                .unwrap_or("local");
            if channel != "local" && channel != "telegram" {
                return Err("channel must be local or telegram".into());
            }
            let provider = params
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("offline");
            let session = params.get("session_id").and_then(|v| v.as_str());
            let message =
                enqueue(home, channel, text, provider, session).map_err(|e| e.to_string())?;
            Ok(json!({ "message": message }))
        }
        "gateway_ambiguous" => {
            let limit = params
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(50)
                .clamp(1, 200) as usize;
            let rows = list_ambiguous_sends(home, limit).map_err(|e| e.to_string())?;
            Ok(json!({
                "messages": rows,
                "count": rows.len(),
                "note": "Succeeded turns without local delivery receipt. Ack after operator confirms external handoff.",
            }))
        }
        "gateway_ack_delivery" => {
            let message_id = params
                .get("message_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "message_id required".to_string())?;
            let outbound_id = params
                .get("outbound_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "outbound_id required".to_string())?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let ok = acknowledge_delivery(home, message_id, outbound_id, now)
                .map_err(|e| e.to_string())?;
            if !ok {
                return Err("ack refused (id mismatch or not terminal)".into());
            }
            Ok(json!({
                "acked": true,
                "message_id": message_id,
                "outbound_id": outbound_id,
                "delivered_unix": now,
            }))
        }
        "gateway_telegram_status" => {
            let config = load_telegram_config(home).map_err(|e| e.to_string())?;
            let token_env = config.bot_token_env.clone();
            let token_present = std::env::var(&token_env)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            Ok(json!({
                "enabled": config.enabled,
                "bot_token_env": token_env,
                "token_present": token_present,
                "allowed_chat_ids": config.allowed_chat_ids,
                "mode": if config.enabled { "config-gated-live" } else { "mock-or-disabled" },
                "note": "Default path is mock/long-poll client. No public listen port. External EO not claimed.",
            }))
        }
        _ => Err(format!("unknown method: {method}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn enqueue_outbox_ambiguous_ack_roundtrip() {
        let dir = tempdir().unwrap();
        let home = dir.path().to_path_buf();
        let enq = handle(
            &home,
            "gateway_enqueue",
            json!({"text": "hi", "channel": "local"}),
        )
        .unwrap();
        let message_id = enq["message"]["id"].as_str().unwrap().to_string();

        // Complete via library drain so outbox exists.
        optimus_kernel::drain_one(&home, |inbound| {
            Ok((format!("echo:{}", inbound.text), None))
        })
        .unwrap()
        .unwrap();

        let amb = handle(&home, "gateway_ambiguous", json!({})).unwrap();
        assert_eq!(amb["count"], 1);
        let outbound_id = amb["messages"][0]["outbound"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(amb["messages"][0]["message_id"], message_id);

        handle(
            &home,
            "gateway_ack_delivery",
            json!({"message_id": message_id, "outbound_id": outbound_id}),
        )
        .unwrap();
        let amb2 = handle(&home, "gateway_ambiguous", json!({})).unwrap();
        assert_eq!(amb2["count"], 0);

        let status = handle(&home, "gateway_status", json!({})).unwrap();
        assert_eq!(status["status"]["ambiguous_sends"], 0);
        assert_eq!(status["status"]["outbox_total"], 1);
    }

    #[test]
    fn telegram_status_is_disabled_by_default() {
        let dir = tempdir().unwrap();
        let home = dir.path().to_path_buf();
        let st = handle(&home, "gateway_telegram_status", json!({})).unwrap();
        assert_eq!(st["enabled"], false);
        assert_eq!(st["token_present"], false);
    }
}
