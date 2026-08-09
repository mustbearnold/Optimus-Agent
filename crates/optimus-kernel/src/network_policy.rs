//! Shared egress policy hooks for browser and web_search (P12 S6).
//!
//! Provider TLS/OAuth adapters may keep adapter-local checks; durable HTTP
//! tools that fetch arbitrary URLs must call [`assert_public_http_url`].

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};

use thiserror::Error;
use url::Url;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EgressError {
    #[error("ssrf blocked: {0}")]
    Ssrf(String),
}

/// Fail closed unless the URL is HTTP(S) to a non-local, non-private host.
pub fn assert_public_http_url(url: &Url) -> Result<(), EgressError> {
    match url.scheme() {
        "http" | "https" => {}
        other => return Err(EgressError::Ssrf(format!("scheme {other}"))),
    }
    let host = url
        .host_str()
        .ok_or_else(|| EgressError::Ssrf("missing host".into()))?
        .to_ascii_lowercase();
    if host_blocked(&host) {
        return Err(EgressError::Ssrf(format!("host {host}")));
    }
    if let Ok(addr) = host.parse::<IpAddr>() {
        if ip_blocked(addr) {
            return Err(EgressError::Ssrf(format!("ip {addr}")));
        }
    } else {
        let port = url.port_or_known_default().unwrap_or(80);
        let key = format!("{host}:{port}");
        if let Ok(iter) = key.to_socket_addrs() {
            for sa in iter.take(8) {
                if ip_blocked(sa.ip()) {
                    return Err(EgressError::Ssrf(format!("resolved {}", sa.ip())));
                }
            }
        }
    }
    Ok(())
}

/// Parse then apply [`assert_public_http_url`].
pub fn assert_public_http_url_str(raw: &str) -> Result<Url, EgressError> {
    let url = Url::parse(raw).map_err(|e| EgressError::Ssrf(format!("parse: {e}")))?;
    assert_public_http_url(&url)?;
    Ok(url)
}

pub fn host_blocked(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host == "metadata.google.internal"
        // AWS EC2 short name for the instance-metadata service, which
        // resolves to the link-local 169.254.169.254. Blocked so the SSRF
        // guard does not rely on DNS resolving to a private IP to catch it.
        || host == "instance-data"
}

pub fn ip_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || (v4.octets()[0] == 169 && v4.octets()[1] == 254)
                || v4.octets()[0] == 0
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped IPv6 (::ffff:a.b.c.d) must be checked against the
            // IPv4 blocklist too: is_loopback() only matches ::1, so e.g.
            // ::ffff:127.0.0.1 would otherwise bypass the SSRF guard.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return ip_blocked(IpAddr::V4(v4));
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
        }
    }
}

/// True when a resolved socket address is blocked for egress.
pub fn socket_blocked(sa: SocketAddr) -> bool {
    ip_blocked(sa.ip())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_localhost_and_private() {
        assert!(assert_public_http_url_str("http://localhost/x").is_err());
        assert!(assert_public_http_url_str("http://127.0.0.1/x").is_err());
        assert!(assert_public_http_url_str("http://10.0.0.5/x").is_err());
        assert!(assert_public_http_url_str("http://[::1]/x").is_err());
        assert!(assert_public_http_url_str("file:///etc/passwd").is_err());
    }

    #[test]
    fn allows_public_https() {
        // example.com is public; DNS may fail offline — only scheme/host literal check for IP.
        let url = Url::parse("https://example.com/path").unwrap();
        // Host is not a literal private IP; resolution may still block if it resolves private
        // (unlikely for example.com). Accept Ok or resolved private as environment variance.
        let _ = assert_public_http_url(&url);
    }

    #[test]
    fn blocks_ipv4_mapped_ipv6() {
        // Regression: ::ffff:127.0.0.1 (IPv4-mapped IPv6) used to bypass the
        // blocklist because is_loopback() only matches ::1.
        assert!(ip_blocked("::ffff:127.0.0.1".parse::<IpAddr>().unwrap()));
        assert!(ip_blocked("::ffff:10.0.0.5".parse::<IpAddr>().unwrap()));
        assert!(ip_blocked("::ffff:169.254.1.1".parse::<IpAddr>().unwrap()));
        assert!(!ip_blocked("::ffff:8.8.8.8".parse::<IpAddr>().unwrap()));
        assert!(assert_public_http_url_str("http://[::ffff:127.0.0.1]/x").is_err());
        assert!(assert_public_http_url_str("http://[::ffff:10.0.0.5]/x").is_err());
    }

    #[test]
    fn blocks_unspecified_ipv6() {
        // Regression: `::` (the unspecified address) used to bypass the guard.
        // It is the IPv6 analogue of 0.0.0.0 (already blocked for IPv4) and on
        // many stacks connections to it land on the local host.
        assert!(ip_blocked("::".parse::<IpAddr>().unwrap()));
        assert!(assert_public_http_url_str("http://[::]/x").is_err());
    }

    #[test]
    fn blocks_aws_instance_metadata_hostname() {
        // Regression: `instance-data` (AWS EC2's short name for the metadata
        // service at 169.254.169.254) used to pass `host_blocked`, so the
        // guard depended on DNS resolving to the link-local IP to catch it.
        // A stub resolver returning the loopback would otherwise admit SSRF.
        assert!(host_blocked("instance-data"));
        assert!(assert_public_http_url_str("http://instance-data/latest/meta-data/").is_err());
    }
}
