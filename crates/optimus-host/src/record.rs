//! One core per home (criterion C3, docs/architecture/north-star-2026-07.md).
//!
//! The host advertises itself in a user-only record inside the home it
//! serves; surfaces probe the record and attach instead of spawning, and a
//! second host start against a healthily served home refuses to run. The
//! record is written only after a successful bind, and every reader
//! health-checks before trusting it, so a crash-stale record simply falls
//! through to a fresh spawn. A record whose token no longer authenticates is
//! treated as unserved: nothing could attach with it anyway.
//!
//! Version 2 (spec-015 A1, ADR-0083): the record carries a `transport` field
//! naming the holder's carrier — `"ws"` written by `optimus serve`, `"http"`
//! by the surviving `--host-only` writer. `read_record` is
//! known-version-tolerant: version-1 records (which only the HTTP holder
//! ever wrote) read back with `transport: None` and are treated as HTTP-mode
//! holders everywhere a diagnostic needs to name the transport.
//!
//! This module lives in optimus-host (not the desktop bin crate) because the
//! serve process writes the record itself (spec-015 R8) and Phase-B surfaces
//! read it; every surface shares one implementation.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const RUNTIME_RECORD_FILE: &str = "host-runtime.json";
pub const RECORD_VERSION_V1: u32 = 1;
pub const RECORD_VERSION_V2: u32 = 2;
pub const TRANSPORT_HTTP: &str = "http";
pub const TRANSPORT_WS: &str = "ws";
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostRuntimeRecord {
    pub version: u32,
    pub port: u16,
    pub pid: u32,
    pub token: String,
    /// Carrier the holder serves: `"http"` (`--host-only`) or `"ws"`
    /// (`optimus serve`). Absent on version-1 records, which only the HTTP
    /// holder ever wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
}

impl HostRuntimeRecord {
    /// The holder's carrier as a stable label: `"ws"` for v2/ws records,
    /// `"http"` for v1 records and v2/http records.
    pub fn transport_label(&self) -> &str {
        match self.transport.as_deref() {
            Some(TRANSPORT_WS) => TRANSPORT_WS,
            _ => TRANSPORT_HTTP,
        }
    }
}

pub fn record_path(home: &Path) -> PathBuf {
    home.join(RUNTIME_RECORD_FILE)
}

/// Advertise a bound host. Called only after the bind succeeds, so the
/// record never points at a port nobody serves yet (spec-015 R8: a
/// post-bind record-write failure is FATAL for serve — the dial ticket lives
/// only in the record).
pub fn write_record(home: &Path, port: u16, token: &str, transport: &str) -> Result<(), String> {
    let record = HostRuntimeRecord {
        version: RECORD_VERSION_V2,
        port,
        pid: std::process::id(),
        token: token.to_string(),
        transport: Some(transport.to_string()),
    };
    let bytes = serde_json::to_vec_pretty(&record).map_err(|error| error.to_string())?;
    optimus_kernel::atomic_write_user_only(&record_path(home), &bytes)
        .map_err(|error| error.to_string())
}

/// Known-version-tolerant read: accepts v1 (transport defaults to `None`,
/// i.e. an HTTP-mode holder) and v2. Any other version, a zero port, or an
/// empty token reads as unserved.
pub fn read_record(home: &Path) -> Option<HostRuntimeRecord> {
    let bytes = std::fs::read(record_path(home)).ok()?;
    let record: HostRuntimeRecord = serde_json::from_slice(&bytes).ok()?;
    if !matches!(record.version, RECORD_VERSION_V1 | RECORD_VERSION_V2)
        || record.port == 0
        || record.token.is_empty()
    {
        return None;
    }
    Some(record)
}

/// The port of a healthy host already serving `home`, if any.
pub fn healthy_serving_port(home: &Path) -> Option<u16> {
    healthy_record(home).map(|record| record.port)
}

/// The record of a healthy host already serving `home`, if any — of ANY
/// record version/transport (spec-015 R8: refusal is against a healthy
/// holder of any version/transport; one core per home).
pub fn healthy_record(home: &Path) -> Option<HostRuntimeRecord> {
    let record = read_record(home)?;
    record_is_healthy(&record).then_some(record)
}

/// Probe one record's port + token (TCP connect + 200 + `ok:true`).
pub fn record_is_healthy(record: &HostRuntimeRecord) -> bool {
    probe_health(record.port, &record.token)
}

/// The serve-side refusal diagnostic naming the holder's transport
/// (spec-015 R1/R8, ADR-0083): distinct from the desktop shell's existing
/// C3 string. The spawner parses both.
pub fn holder_refusal_diagnostic(record: &HostRuntimeRecord) -> String {
    if record.transport_label() == TRANSPORT_WS {
        "a host is already serving this home in ws mode".to_string()
    } else {
        "a host is already serving this home in HTTP mode".to_string()
    }
}

/// Append an accepted-connection line to `<home>/logs/connections.log`
/// (spec-015 R8): fires post-hello — after the credential handshake
/// COMPLETED, so a rejected handshake never logs and a line proves dial AND
/// handshake. The line carries the origin (`"null"`/`"missing"` or the
/// origin value) and a timestamp, never the ticket; format pinned in the
/// protocol schema (`docs/architecture/surface-protocol.schema.json`).
pub fn log_connection(home: &Path, origin: &str) {
    use std::io::Write;
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
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
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

/// Minimal loopback `GET /api/health` with the record's bearer token. Kept on
/// std TcpStream so probing costs no HTTP-client dependency.
fn probe_health(port: u16, token: &str) -> bool {
    let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    if stream.set_read_timeout(Some(PROBE_TIMEOUT)).is_err()
        || stream.set_write_timeout(Some(PROBE_TIMEOUT)).is_err()
    {
        return false;
    }
    let mut stream = stream;
    let request = format!(
        "GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
         Authorization: Bearer {token}\r\nOrigin: http://127.0.0.1:{port}\r\n\
         Connection: close\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut response = String::new();
    if stream.read_to_string(&mut response).is_err() {
        return false;
    }
    response.starts_with("HTTP/1.1 200") && response.contains("\"ok\":true")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;

    /// A one-request loopback server speaking just enough HTTP for the probe:
    /// 200 + health body when the bearer token matches, 401 otherwise.
    fn scripted_health_server(expected_token: &str) -> (u16, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let expected = format!("Authorization: Bearer {expected_token}");
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut authorized = false;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if line.trim() == expected {
                    authorized = true;
                }
                if line == "\r\n" {
                    break;
                }
            }
            let mut stream = stream;
            let body = if authorized {
                "HTTP/1.1 200 OK\r\nContent-Length: 27\r\nConnection: close\r\n\r\n{\"ok\":true,\"streaming\":true}"
            } else {
                "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            };
            let _ = stream.write_all(body.as_bytes());
        });
        (port, handle)
    }

    #[test]
    fn record_roundtrips_and_is_user_only() {
        let home = tempfile::tempdir().unwrap();
        write_record(home.path(), 43111, "token-roundtrip", TRANSPORT_WS).unwrap();
        let record = read_record(home.path()).expect("record must read back");
        assert_eq!(record.port, 43111);
        assert_eq!(record.token, "token-roundtrip");
        assert_eq!(record.pid, std::process::id());
        assert_eq!(record.version, RECORD_VERSION_V2);
        assert_eq!(record.transport_label(), TRANSPORT_WS);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(record_path(home.path()))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "the record carries the host token");
        }
    }

    #[test]
    fn v1_records_read_back_as_http_holders() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            record_path(home.path()),
            br#"{"version":1,"port":43112,"pid":1,"token":"v1-token"}"#,
        )
        .unwrap();
        let record = read_record(home.path()).expect("v1 records are known");
        assert_eq!(record.version, RECORD_VERSION_V1);
        assert_eq!(record.transport_label(), TRANSPORT_HTTP);
        assert_eq!(
            holder_refusal_diagnostic(&record),
            "a host is already serving this home in HTTP mode"
        );
    }

    #[test]
    fn missing_or_malformed_records_read_as_unserved() {
        let home = tempfile::tempdir().unwrap();
        assert!(read_record(home.path()).is_none(), "missing file");
        std::fs::write(record_path(home.path()), b"not json").unwrap();
        assert!(read_record(home.path()).is_none(), "malformed file");
        std::fs::write(
            record_path(home.path()),
            br#"{"version":99,"port":1,"pid":1,"token":"x"}"#,
        )
        .unwrap();
        assert!(read_record(home.path()).is_none(), "unknown version");
        std::fs::write(
            record_path(home.path()),
            br#"{"version":2,"port":0,"pid":1,"token":"x","transport":"ws"}"#,
        )
        .unwrap();
        assert!(read_record(home.path()).is_none(), "zero port");
    }

    #[test]
    fn healthy_host_is_discovered_through_its_record() {
        let home = tempfile::tempdir().unwrap();
        let (port, server) = scripted_health_server("token-live");
        write_record(home.path(), port, "token-live", TRANSPORT_HTTP).unwrap();
        // Single probe: the scripted server answers exactly one request.
        let holder = healthy_record(home.path()).expect("healthy holder");
        assert_eq!(holder.port, port);
        assert_eq!(holder.transport_label(), TRANSPORT_HTTP);
        server.join().unwrap();
    }

    #[test]
    fn refusal_diagnostic_names_the_ws_holder() {
        let holder = HostRuntimeRecord {
            version: RECORD_VERSION_V2,
            port: 43113,
            pid: 7,
            token: "t".into(),
            transport: Some(TRANSPORT_WS.into()),
        };
        assert_eq!(
            holder_refusal_diagnostic(&holder),
            "a host is already serving this home in ws mode"
        );
    }

    #[test]
    fn wrong_token_record_reads_as_unserved() {
        let home = tempfile::tempdir().unwrap();
        let (port, server) = scripted_health_server("token-real");
        write_record(home.path(), port, "token-stale", TRANSPORT_WS).unwrap();
        assert_eq!(healthy_serving_port(home.path()), None);
        server.join().unwrap();
    }

    #[test]
    fn dead_port_record_reads_as_unserved() {
        let home = tempfile::tempdir().unwrap();
        let placeholder = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead_port = placeholder.local_addr().unwrap().port();
        drop(placeholder);
        write_record(home.path(), dead_port, "token-dead", TRANSPORT_WS).unwrap();
        assert_eq!(healthy_serving_port(home.path()), None);
    }
}
