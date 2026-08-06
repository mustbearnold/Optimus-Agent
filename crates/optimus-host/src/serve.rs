//! `optimus serve` — the headless agent backend (spec-015, ADR-0083/0084).
//!
//! One core per home: serve owns the SQLite home, sessions, approvals,
//! filesystem scopes, and every durable effect, and a second serve against a
//! healthily served home refuses to start (exit 3, named diagnostic). The
//! wire contract is JSON-RPC 2.0 over two carriers sharing one dispatch —
//! loopback WebSocket (desktop renderer, attached clients) and stdio
//! (spawned children). HTTP `GET /api/health` stays on the record port,
//! Bearer-gated exactly as the HTTP mode gates it today: the record token IS
//! the Bearer.
//!
//! Lifecycle pins (R1/R8): exit 2 = bind, security-validation, or
//! record-write failure (bind-failure exit 2 is a CHANGE from the HTTP
//! mode's exit 1, ADR-0083); exit 3 = refusal (home already served). The
//! record is written only after a successful bind and a post-bind
//! record-write failure is FATAL (the dial ticket lives only in the record).
//! `--stdio` opens the record + listener additively and is the ONLY mode
//! that reads stdin; plain serve never reads stdin at all (a GUI-spawned
//! child's stdin is typically /dev/null and an immediate EOF must not be
//! treated as a carrier disconnect — R4/R9).

use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::record::{self, TRANSPORT_WS};
use crate::ticket;

/// Default loopback port (spec-015 R8; `DEFAULT_HOST_PORT` precedent,
/// `apps/optimus-desktop/src/main.rs:34`).
pub const DEFAULT_HOST_PORT: u16 = 17865;

/// Exit 2: bind, security-validation, or record-write failure (ADR-0083).
pub const EXIT_BIND_OR_SECURITY: i32 = 2;
/// Exit 3: refusal — a healthy host already serves this home.
pub const EXIT_REFUSED: i32 = 3;

/// Run the headless backend. Never returns: exits 2/3 on lifecycle failure
/// per the pinned codes, otherwise serves until the process is terminated.
///
/// `_stdio` (the `--stdio` flag) is accepted from Phase A1; the stdio carrier
/// itself lands with the wire layer (Phase A2) — until then the flag changes
/// nothing, and no mode reads stdin.
pub fn run(home: &Path, port: u16, _stdio: bool) -> ! {
    // One core per home (C3): refuse on a HEALTHY holder of ANY record
    // version/transport, naming the holder's transport (R1/R8, ADR-0083).
    // A stale record (dead port) falls through to a fresh bind.
    if let Some(holder) = record::healthy_record(home) {
        eprintln!("error: {}", record::holder_refusal_diagnostic(&holder));
        std::process::exit(EXIT_REFUSED);
    }

    let addr = format!("127.0.0.1:{port}");
    let server = match Server::http(&addr) {
        Ok(server) => server,
        Err(error) => {
            // Bind failure after a negative probe = spawn-race loser; fail
            // closed, never a second port (ADR-0045:137-139, R8).
            eprintln!("error: cannot bind {addr}: {error}");
            std::process::exit(EXIT_BIND_OR_SECURITY);
        }
    };

    // The dial ticket: env-delivered by the spawning shell, or a
    // manual-start mint. The record token IS the accepted WS dial ticket
    // (R7, ADR-0084). Never printed to stderr.
    let ticket = ticket::dial_ticket();
    if let Err(error) = record::write_record(home, port, &ticket, TRANSPORT_WS) {
        // FATAL (R8): a running serve without a record would be unreachable
        // by design and would produce a false "check port" diagnostic in
        // every client. Not inherited from the HTTP mode's best-effort write.
        eprintln!("error: cannot write host-runtime record: {error}");
        std::process::exit(EXIT_BIND_OR_SECURITY);
    }
    eprintln!(
        "[optimus serve] home={} on 127.0.0.1:{port}",
        home.display()
    );

    for request in server.incoming_requests() {
        let method = request.method().clone();
        let path = request
            .url()
            .split('?')
            .next()
            .unwrap_or_default()
            .to_string();
        match (method, path.as_str()) {
            (Method::Get, "/api/health") => {
                if !bearer_matches(&request, &ticket) {
                    let _ = request.respond(json_response(401, "{\"ok\":false}"));
                    continue;
                }
                let _ = request.respond(json_response(
                    200,
                    "{\"ok\":true,\"streaming\":true,\"transport\":\"ws\"}",
                ));
            }
            // Phase A2 accepts WebSocket upgrades here (tiny_http
            // `Request::upgrade`) and serves the JSON-RPC wire layer.
            _ => {
                let _ = request.respond(json_response(404, "{\"ok\":false}"));
            }
        }
    }
    std::process::exit(0);
}

/// Constant-time-ish bearer check against the dial ticket. The health
/// endpoint is protected by the same credential as the WS handshake (R8).
fn bearer_matches(request: &Request, ticket: &str) -> bool {
    let expected = format!("Bearer {ticket}");
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Authorization"))
        .is_some_and(|header| {
            header.value.as_str().len() == expected.len()
                && header
                    .value
                    .as_str()
                    .as_bytes()
                    .iter()
                    .zip(expected.as_bytes())
                    .fold(0u8, |difference, (left, right)| difference | (left ^ right))
                    == 0
        })
}

/// Append an accepted-connection line to `<home>/logs/connections.log`
/// (R8): fires post-hello — after the credential handshake COMPLETED, so a
/// rejected handshake never logs and a line proves dial AND handshake. The
/// line carries the origin (`"null"`/`"missing"` or the origin value) and a
/// timestamp, never the ticket; format pinned in the protocol schema.
pub fn append_connection_log(home: &Path, origin: &str) {
    let dir = home.join("logs");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let line = format!("{} origin={origin}\n", iso8601_utc_now());
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("connections.log"))
        .and_then(|mut file| file.write_all(line.as_bytes()));
}

/// ISO-8601 UTC timestamp (`2026-08-06T13:03:00Z`) without a chrono
/// dependency: civil-from-days conversion (Hinnant's algorithm).
fn iso8601_utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn json_response(status: u16, body: &'static str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut response =
        Response::from_data(body.as_bytes().to_vec()).with_status_code(StatusCode(status));
    if let Ok(header) = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]) {
        response.add_header(header);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::{civil_from_days, iso8601_utc_now};

    #[test]
    fn civil_from_days_roundtrips_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_298), (2025, 7, 29));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(20_625), (2026, 6, 21));
    }

    #[test]
    fn iso8601_now_is_utc_shaped() {
        let stamp = iso8601_utc_now();
        assert!(
            stamp.len() == 20 && stamp.ends_with('Z'),
            "unexpected stamp: {stamp}"
        );
        assert!(stamp.starts_with("20"), "sanity: current era: {stamp}");
    }
}
