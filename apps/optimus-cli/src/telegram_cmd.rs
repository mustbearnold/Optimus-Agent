//! `optimus gateway telegram` — the adapter's status face and its live run loop.
//!
//! This module is where B-CAP-02 stops being a fixture. The mock transport
//! proved the cycle (enqueue → turn → obligation → send → settle); `run` is that
//! same cycle driven by `LiveTelegramTransport` against the real Bot API, and it
//! is the first place an Optimus turn is started by someone who is not sitting
//! at this machine.
//!
//! Two things that look like details are the whole design:
//!
//! - **The turn is `optimus_host::gateway_turn`**, not a local copy. A remote
//!   sender gets the same route, the same session derivation, and the same
//!   SmartDeny spine a local session gets. A high-risk effect pauses and waits
//!   for the operator here exactly as it does in the desktop app (ADR-0071).
//! - **The poll cursor is durable.** `poll_once` takes an offset and hands back
//!   the next one, leaving the cursor to its caller — and in production that
//!   caller is this file. An in-memory cursor would restart at zero, and
//!   Telegram would redeliver every update it has not seen confirmed, so a
//!   restart would answer the same messages a second time.

use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Subcommand;
use optimus_kernel::{load_telegram_config, telegram_poll_once, TelegramConfig};

/// Seconds Telegram is asked to hold an empty poll open before answering.
///
/// The Bot API documents `timeout=0` as short polling and asks production
/// callers not to use it; 30s sits in the band it recommends instead.
const POLL_HOLD_SECS: u64 = 30;

/// Pause after a cycle that found nothing, so a transport answering instantly —
/// a proxy, an error page — cannot turn this loop into a spin.
const IDLE_PAUSE: Duration = Duration::from_secs(1);

/// Consecutive failed cycles tolerated before the loop gives up.
///
/// A dropped connection should not end a long-lived poller, but a loop that
/// retries forever hides a broken token or a revoked bot from whoever started
/// it. Failing out after a few tries reports the real error to the operator.
const MAX_CONSECUTIVE_FAILURES: u32 = 5;

#[derive(Subcommand, Debug)]
pub enum TelegramCmd {
    /// Show adapter configuration and readiness (no secrets printed)
    Status,
    /// Long-poll Telegram and answer, until --max-cycles or interrupted
    Run {
        /// Stop after N cycles (0 = run until interrupted)
        #[arg(long, default_value_t = 0)]
        max_cycles: u64,
        /// Seconds Telegram holds an empty poll open
        #[arg(long, default_value_t = POLL_HOLD_SECS)]
        poll_seconds: u64,
    },
}

/// Dispatch `optimus gateway telegram [status|run]`; bare invocation is status.
pub fn run(home: &Path, cmd: Option<&TelegramCmd>) -> Result<(), Box<dyn std::error::Error>> {
    let config = load_telegram_config(home)?;
    match cmd.unwrap_or(&TelegramCmd::Status) {
        TelegramCmd::Status => {
            print_status(&config);
            Ok(())
        }
        TelegramCmd::Run {
            max_cycles,
            poll_seconds,
        } => run_live(home, &config, *max_cycles, *poll_seconds),
    }
}

fn print_status(config: &TelegramConfig) {
    let token_present = std::env::var(&config.bot_token_env)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    println!(
        "telegram enabled={} token_env={} token_present={} mode={}",
        config.enabled,
        config.bot_token_env,
        token_present,
        if config.enabled {
            "config-gated-live"
        } else {
            "mock-or-disabled"
        }
    );
    println!(
        "allowed_chats={} note=no public listen port; local gateway is authority",
        config.allowed_chat_ids.len()
    );
}

/// Poll, answer, and deliver until the cycle budget runs out or the loop dies.
///
/// Cancellation is per cycle (law 9). There is no half-finished unit to
/// interrupt: a cycle either completes and advances the durable cursor, or it
/// does not and the same updates arrive again. Killing the process between
/// cycles loses nothing, and killing it mid-cycle leaves an obligation claimed
/// until its lease expires, which the ledger already handles.
fn run_live(
    home: &Path,
    config: &TelegramConfig,
    max_cycles: u64,
    poll_seconds: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut transport = config.live_transport(poll_seconds)?;
    let home_buf = home.to_path_buf();
    let mut offset = load_cursor(home);
    let mut cycles = 0u64;
    let mut failures = 0u32;
    eprintln!(
        "[optimus-telegram] polling from offset={offset} allowed_chats={} home={}",
        config.allowed_chat_ids.len(),
        home.display()
    );
    loop {
        match telegram_poll_once(home, &mut transport, offset, |message| {
            optimus_host::gateway_turn(&home_buf, message)
        }) {
            Ok(result) => {
                failures = 0;
                offset = result.next_offset;
                save_cursor(home, offset);
                if !result.enqueued.is_empty() {
                    println!(
                        "cycle enqueued={} drained={} receipts={} ambiguous={} failed_sends={}",
                        result.enqueued.len(),
                        result.drained.len(),
                        result.receipts.len(),
                        result.ambiguous.len(),
                        result.failed_sends.len()
                    );
                }
                for id in &result.ambiguous {
                    println!("  ambiguous send, recover with: optimus gateway ambiguous  ({id})");
                }
                if result.enqueued.is_empty() {
                    std::thread::sleep(IDLE_PAUSE);
                }
            }
            Err(error) => {
                failures += 1;
                eprintln!("[optimus-telegram] cycle failed ({failures}/{MAX_CONSECUTIVE_FAILURES}): {error}");
                if failures >= MAX_CONSECUTIVE_FAILURES {
                    return Err(error.into());
                }
                std::thread::sleep(IDLE_PAUSE * failures);
            }
        }
        cycles += 1;
        if max_cycles > 0 && cycles >= max_cycles {
            break;
        }
    }
    Ok(())
}

fn cursor_path(home: &Path) -> PathBuf {
    home.join("gateway").join("telegram-offset")
}

/// The stored cursor, or zero when there is none or it is unreadable.
///
/// Zero is the safe direction: Telegram replays what it has not seen confirmed,
/// so a lost cursor costs duplicate answers rather than silently skipping a
/// message that was never answered at all.
fn load_cursor(home: &Path) -> u64 {
    std::fs::read_to_string(cursor_path(home))
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
        .unwrap_or(0)
}

/// Persist the cursor, or say why it could not be persisted.
///
/// A failure here is not fatal — the cycle already happened and the replies
/// already went out — but it is never silent, because the next restart will
/// repeat work the operator has no other way to predict.
fn save_cursor(home: &Path, offset: u64) {
    let path = cursor_path(home);
    let temporary = path.with_extension("tmp");
    let written = std::fs::write(&temporary, offset.to_string())
        .and_then(|()| std::fs::rename(&temporary, &path));
    if let Err(error) = written {
        eprintln!("[optimus-telegram] could not persist poll cursor {offset}: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_corrupt_cursor_replays_rather_than_skips() {
        let home = tempfile::tempdir().expect("home");
        assert_eq!(load_cursor(home.path()), 0, "no cursor yet");
        std::fs::create_dir_all(home.path().join("gateway")).expect("gateway dir");
        std::fs::write(cursor_path(home.path()), "not-a-number").expect("corrupt cursor");
        assert_eq!(
            load_cursor(home.path()),
            0,
            "an unreadable cursor must replay, never skip ahead"
        );
    }

    #[test]
    fn the_cursor_survives_the_process_that_wrote_it() {
        let home = tempfile::tempdir().expect("home");
        std::fs::create_dir_all(home.path().join("gateway")).expect("gateway dir");
        save_cursor(home.path(), 4171);
        assert_eq!(load_cursor(home.path()), 4171);
        save_cursor(home.path(), 4172);
        assert_eq!(
            load_cursor(home.path()),
            4172,
            "the cursor only moves forward"
        );
        assert!(
            !cursor_path(home.path()).with_extension("tmp").exists(),
            "the atomic rename must not leave a temporary behind"
        );
    }

    #[test]
    fn a_live_run_refuses_a_disabled_adapter_before_it_reaches_the_network() {
        let home = tempfile::tempdir().expect("home");
        let config = TelegramConfig::default();
        assert!(!config.enabled, "the adapter ships disabled");
        let error = run_live(home.path(), &config, 1, 1).expect_err("disabled must refuse");
        assert!(
            error.to_string().contains("disabled"),
            "the refusal must name the gate: {error}"
        );
    }
}
