//! One core per home (criterion C3, docs/architecture/north-star-2026-07.md).
//!
//! The host-only server advertises itself in a user-only record inside the
//! home it serves; surfaces probe the record and attach instead of spawning,
//! and a second host-only start against a healthily served home refuses to
//! run. The record is written only after a successful bind, and every reader
//! health-checks before trusting it, so a crash-stale record simply falls
//! through to a fresh spawn. A record whose token no longer authenticates is
//! treated as unserved: nothing could attach with it anyway.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const RUNTIME_RECORD_FILE: &str = "host-runtime.json";
const RUNTIME_RECORD_VERSION: u32 = 1;
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostRuntimeRecord {
    pub version: u32,
    pub port: u16,
    pub pid: u32,
    pub token: String,
}

pub fn record_path(home: &Path) -> PathBuf {
    home.join(RUNTIME_RECORD_FILE)
}

/// Advertise a bound host. Called only after `Server::http` succeeds, so the
/// record never points at a port nobody serves yet.
pub fn write_record(home: &Path, port: u16, token: &str) -> Result<(), String> {
    let record = HostRuntimeRecord {
        version: RUNTIME_RECORD_VERSION,
        port,
        pid: std::process::id(),
        token: token.to_string(),
    };
    let bytes = serde_json::to_vec_pretty(&record).map_err(|error| error.to_string())?;
    optimus_kernel::atomic_write_user_only(&record_path(home), &bytes)
        .map_err(|error| error.to_string())
}

pub fn read_record(home: &Path) -> Option<HostRuntimeRecord> {
    let bytes = std::fs::read(record_path(home)).ok()?;
    let record: HostRuntimeRecord = serde_json::from_slice(&bytes).ok()?;
    if record.version != RUNTIME_RECORD_VERSION || record.port == 0 || record.token.is_empty() {
        return None;
    }
    Some(record)
}

/// The port of a healthy host already serving `home`, if any.
pub fn healthy_serving_port(home: &Path) -> Option<u16> {
    let record = read_record(home)?;
    probe_health(record.port, &record.token).then_some(record.port)
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
        write_record(home.path(), 43111, "token-roundtrip").unwrap();
        let record = read_record(home.path()).expect("record must read back");
        assert_eq!(record.port, 43111);
        assert_eq!(record.token, "token-roundtrip");
        assert_eq!(record.pid, std::process::id());
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
    }

    #[test]
    fn healthy_host_is_discovered_through_its_record() {
        let home = tempfile::tempdir().unwrap();
        let (port, server) = scripted_health_server("token-live");
        write_record(home.path(), port, "token-live").unwrap();
        assert_eq!(healthy_serving_port(home.path()), Some(port));
        server.join().unwrap();
    }

    #[test]
    fn wrong_token_record_reads_as_unserved() {
        let home = tempfile::tempdir().unwrap();
        let (port, server) = scripted_health_server("token-real");
        write_record(home.path(), port, "token-stale").unwrap();
        assert_eq!(healthy_serving_port(home.path()), None);
        server.join().unwrap();
    }

    #[test]
    fn dead_port_record_reads_as_unserved() {
        let home = tempfile::tempdir().unwrap();
        let placeholder = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead_port = placeholder.local_addr().unwrap().port();
        drop(placeholder);
        write_record(home.path(), dead_port, "token-dead").unwrap();
        assert_eq!(healthy_serving_port(home.path()), None);
    }
}
