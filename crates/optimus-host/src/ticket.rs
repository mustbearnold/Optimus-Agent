//! Dial-ticket and process-secret minting/delivery for `optimus serve`
//! (spec-015 R7, ADR-0084).
//!
//! Two credentials, two classes:
//!
//! - The DIAL TICKET authenticates renderer/tui/cli-kind connections. The
//!   spawning shell mints it per launch (CSPRNG, >= 32 chars) and delivers it
//!   via environment — never argv, which is ps-visible. A serve started
//!   manually (no env ticket) mints its own per-launch ticket. In BOTH mint
//!   paths serve writes the ticket to the user-only `host-runtime.json`
//!   record; the record token IS the accepted WS dial ticket.
//! - The PROCESS SECRET authenticates the shell kind (`client_kind:"shell"`).
//!   The spawning shell mints it per launch and delivers it via environment;
//!   `os.rs` compares it constant-time per call (`os.rs:105-118`). A manual
//!   serve (no env secret) rejects all shell-kind connections.
//!
//! Serve never prints either credential to stderr (divergence from the
//! HTTP-token stderr pairing, ADR-0084).

/// Env name the spawning shell uses to deliver the per-launch dial ticket.
pub const TICKET_ENV: &str = "OPTIMUS_HOST_TOKEN";
/// Env name the spawning shell uses to deliver the per-launch process
/// secret (the constant `os.rs` already reads for the staging relay).
pub const PROCESS_SECRET_ENV: &str = "OPTIMUS_NATIVE_SELECTION_TOKEN";
/// Minimum credential length (spec-015 R7; `os.rs:109`).
pub const TICKET_MIN_CHARS: usize = 32;

/// The dial ticket for this launch: the env-delivered ticket when the
/// spawning shell provided one (>= 32 chars), otherwise a manual-start mint
/// fallback (`apps/optimus-desktop/src/main.rs:144-155` pattern). Never
/// printed, never in argv, never in a URL.
pub fn dial_ticket() -> String {
    std::env::var(TICKET_ENV)
        .ok()
        .filter(|ticket| ticket.len() >= TICKET_MIN_CHARS)
        .unwrap_or_else(|| {
            format!(
                "optimus-host-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4().simple()
            )
        })
}

/// The process secret the spawning shell delivered, if any. `None` means a
/// manual serve: shell-kind connections are rejected (the staging relay is
/// unavailable outside the spawn path, spec-015 R7).
pub fn process_secret() -> Option<String> {
    std::env::var(PROCESS_SECRET_ENV)
        .ok()
        .filter(|secret| secret.len() >= TICKET_MIN_CHARS)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::sync::Mutex;

    use super::*;

    /// Env vars are process-global and other tests (chat.rs) manipulate them,
    /// so env-mutating tests serialize on one lock.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Restores the original value of `key` on drop.
    struct EnvGuard {
        key: &'static str,
        original: Option<OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let original = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, original }
        }

        fn clear(key: &'static str) -> Self {
            let original = std::env::var_os(key);
            std::env::remove_var(key);
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn manual_mint_fallback_is_long_and_unique() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvGuard::clear(TICKET_ENV);
        let first = dial_ticket();
        let second = dial_ticket();
        assert!(first.len() >= TICKET_MIN_CHARS, "ticket too short");
        assert_ne!(first, second, "per-launch tickets must differ");
    }

    #[test]
    fn env_delivered_ticket_wins_when_long_enough() {
        let _lock = ENV_LOCK.lock().unwrap();
        let ticket = "x".repeat(48);
        let _guard = EnvGuard::set(TICKET_ENV, &ticket);
        assert_eq!(dial_ticket(), ticket);
    }

    #[test]
    fn undersized_env_ticket_falls_back_to_mint() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvGuard::set(TICKET_ENV, "too-short");
        assert!(dial_ticket().len() >= TICKET_MIN_CHARS);
    }

    #[test]
    fn process_secret_is_absent_without_env() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvGuard::clear(PROCESS_SECRET_ENV);
        assert!(process_secret().is_none());
        let secret = "y".repeat(40);
        let _guard = EnvGuard::set(PROCESS_SECRET_ENV, &secret);
        assert_eq!(process_secret().as_deref(), Some(secret.as_str()));
    }
}
