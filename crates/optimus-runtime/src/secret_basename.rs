//! Secret and credential basename policy (ADR-0049 module-split companion).
//!
//! `is_secret_basename` is the single policy authority shared by runtime
//! effects and kernel filesystem-root callers. It lives in its own module
//! so `crates/optimus-runtime/src/lib.rs` stays under its baselined size.

/// True for secret or credential basenames denied by filesystem tools.
///
/// This is the single policy authority shared by runtime effects and kernel
/// filesystem-root callers.
pub fn is_secret_basename(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    // `.env`, `.env.local`, `.env.production`, ... — environment-scoped env
    // files carry the same credentials as the bare `.env`. A basename is an
    // env file only when `.env` is followed by a scope separator (`.`/`-`) or
    // ends right there; merely sharing the `.env` prefix (`.environment`,
    // `.envelopes`, `.envrc`) is not a credential file and must not be denied.
    is_env_scoped_basename(&lower)
        || matches!(
            lower.as_str(),
            // Config credentials and SSH private keys. The `id_*` set covers
            // the OpenSSH key types beyond `id_rsa`, including the
            // FIDO2-backed security-key types (`id_*_sk`), whose private keys
            // must be protected just like any other.
            "auth.json"
                | "id_rsa"
                | "id_ed25519"
                | "id_ed448"
                | "id_ecdsa"
                | "id_dsa"
                | "id_ed25519_sk"
                | "id_ecdsa_sk"
                | ".netrc"
        )
        || lower.ends_with(".pem")
}

/// True only for the env-file basenames that carry credentials: the bare
/// `.env` and the scoped forms (`.env.local`, `.env-production`, ...). A name
/// that merely starts with `.env` (`.environment`, `.envelopes`) is a regular
/// file, not a secret.
fn is_env_scoped_basename(lower: &str) -> bool {
    lower == ".env"
        || lower
            .strip_prefix(".env")
            .is_some_and(|rest| rest.starts_with('.') || rest.starts_with('-'))
}

#[cfg(test)]
mod tests {
    use super::is_secret_basename;

    #[test]
    fn denies_exact_credential_filenames() {
        for name in [".env", "auth.json", "id_rsa", ".netrc"] {
            assert!(is_secret_basename(name), "{name} should be denied");
        }
    }

    #[test]
    fn denies_ssh_private_key_types() {
        for name in ["id_ed25519", "id_ed448", "id_ecdsa", "id_dsa"] {
            assert!(is_secret_basename(name), "{name} should be denied");
        }
    }

    #[test]
    fn denies_ssh_security_key_private_key_types() {
        for name in ["id_ed25519_sk", "id_ecdsa_sk"] {
            assert!(is_secret_basename(name), "{name} should be denied");
        }
    }

    #[test]
    fn denies_environment_scoped_env_files() {
        for name in [".env.local", ".env.production", ".env.test", ".ENV.PROD"] {
            assert!(is_secret_basename(name), "{name} should be denied");
        }
    }

    #[test]
    fn denies_pem_suffix_case_insensitively() {
        assert!(is_secret_basename("key.pem"));
        assert!(is_secret_basename("CERT.PEM"));
    }

    #[test]
    fn env_prefix_requires_a_scope_separator() {
        // Regression: the old `starts_with(".env")` check denied any basename
        // that merely shared the `.env` prefix, so ordinary files like
        // `.environment.yaml`, `.envelopes`, and the direnv marker `.envrc`
        // were treated as secrets. Only the bare `.env` and its scoped forms
        // (`.`/`-` separator) are credential files; these must be allowed.
        for name in [".environment.yaml", ".envelopes", ".envrc", ".envtest"] {
            assert!(
                !is_secret_basename(name),
                "{name} is not a credential file and should be allowed"
            );
        }
        // The scoped env forms stay denied, including the hyphenated spelling
        // and case variants.
        for name in [
            ".env.local",
            ".env.production",
            ".ENV.PROD",
            ".env-production",
            ".env-test",
        ] {
            assert!(
                is_secret_basename(name),
                "{name} is an env-scoped credential file and should be denied"
            );
        }
    }

    #[test]
    fn allows_ordinary_source_files() {
        for name in ["main.rs", "package.json", "notes.md", ".gitignore"] {
            assert!(!is_secret_basename(name), "{name} should be allowed");
        }
    }
}
