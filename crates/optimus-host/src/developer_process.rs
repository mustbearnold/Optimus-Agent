//! Child process liveness: readiness, health, and termination.
//!
//! Split out of `developer.rs` under the ADR-0049 module-size ratchet. Every
//! helper here is about the child as an OS process, never about its contents.

use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

pub(crate) fn wait_for_health(port: u16, token: &str, pid: u32) -> Result<(), String> {
    for _ in 0..50 {
        if !pid_alive(pid) {
            return Err("development instance exited before health check".into());
        }
        if health_ok(port, token) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("development instance did not pass the health check within 5 seconds".into())
}

pub(crate) fn wait_for_ready(path: &Path, pid: u32) -> Result<(), String> {
    for _ in 0..50 {
        if !pid_alive(pid) {
            return Err("development desktop exited before readiness".into());
        }
        if ready_ok(path, pid) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("development desktop did not pass readiness within 5 seconds".into())
}

pub(crate) fn ready_ok(path: &Path, pid: u32) -> bool {
    let Ok(body) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) else {
        return false;
    };
    value.get("ready").and_then(|value| value.as_bool()) == Some(true)
        && value.get("pid").and_then(|value| value.as_u64()) == Some(u64::from(pid))
}

pub(crate) fn health_ok(port: u16, token: &str) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(150)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
    let request = format!(
        "GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut response = String::new();
    stream.read_to_string(&mut response).is_ok() && response.starts_with("HTTP/1.1 200")
}

pub(crate) fn stop_pid(pid: Option<u32>) -> Result<(), String> {
    let Some(pid) = pid else {
        return Ok(());
    };
    #[cfg(unix)]
    {
        let pid = pid as libc::pid_t;
        unsafe {
            if libc::kill(pid, libc::SIGTERM) != 0 && *libc::__errno_location() != libc::ESRCH {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        for _ in 0..20 {
            if reap_child(pid)? {
                return Ok(());
            }
            if !pid_alive(pid as u32) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        unsafe {
            if libc::kill(pid, libc::SIGKILL) != 0 && *libc::__errno_location() != libc::ESRCH {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        for _ in 0..20 {
            if reap_child(pid)? || !pid_alive(pid as u32) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err("development instance did not exit after SIGKILL".into())
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Err("process stop is not implemented on this platform".into())
    }
}

#[cfg(unix)]
pub(crate) fn reap_child(pid: libc::pid_t) -> Result<bool, String> {
    let mut status = 0;
    let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    if result == pid {
        return Ok(true);
    }
    if result == 0 {
        return Ok(false);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ECHILD) {
        return Ok(false);
    }
    Err(format!("could not inspect development instance: {error}"))
}

pub(crate) fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        if reap_child(pid as libc::pid_t).unwrap_or(false) {
            return false;
        }
        unsafe {
            libc::kill(pid as libc::pid_t, 0) == 0 || *libc::__errno_location() == libc::EPERM
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}
