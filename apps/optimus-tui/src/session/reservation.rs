//! Durable identity reservation for a newly submitted terminal conversation.
//!
//! This runs on the turn worker so creating the session never blocks screen
//! input or animation. Its update is queued before provider or tool work, which
//! gives even a failed first turn an identity the next prompt can continue.

use std::sync::mpsc::Sender;

use optimus_host::client::HostClient;
use serde_json::{json, Value};

use super::TurnUpdate;

/// Ensure the outgoing turn and screen share one durable session identity.
///
/// `false` means the terminal update channel is closed or reservation failed;
/// in either case the caller must stop before contacting the provider.
pub(super) fn ensure(
    client: &HostClient,
    params: &mut Value,
    updates: &Sender<TurnUpdate>,
) -> bool {
    if params.get("session").is_some() {
        return true;
    }

    let reserved = client
        .call("new_session", json!({}))
        .map_err(|error| format!("could not start a durable session: {error}"))
        .and_then(|value| {
            value
                .get("id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| "could not start a durable session: host returned no id".to_string())
        });
    let id = match reserved {
        Ok(id) => id,
        Err(error) => {
            let _ = updates.send(TurnUpdate::Failed(error));
            return false;
        }
    };

    params["session"] = json!(id);
    updates.send(TurnUpdate::SessionReserved(id)).is_ok()
}
