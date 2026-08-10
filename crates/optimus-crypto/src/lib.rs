//! Deterministic SHA-256 digest computation and validated hex identities.
//!
//! One place owns "what a SHA-256 digest looks like": exactly 64 ASCII hex
//! digits. Validation accepts either case; computed digests are lowercase
//! hex. Every crate validates or computes digests through this seam instead
//! of re-deriving the rule (previously ~14 hand-written copies across 10
//! crates, one of which had already drifted in its error text).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Compute the lowercase hex SHA-256 digest of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Whether `value` is a valid SHA-256 hex digest (64 hex digits, any case).
pub fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// A validated SHA-256 hex digest.
///
/// Construction normalizes to lowercase; `Display`/`as_str` return the
/// canonical form. Serializes transparently as the plain hex string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    /// Parse a digest string; `None` unless it is exactly 64 hex digits.
    pub fn parse(value: &str) -> Option<Self> {
        is_sha256_hex(value).then(|| Self(value.to_ascii_lowercase()))
    }

    /// Compute the digest of `bytes`.
    pub fn digest(bytes: &[u8]) -> Self {
        Self(sha256_hex(bytes))
    }

    /// The canonical lowercase hex form.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for Sha256Digest {
    type Err = &'static str;

    /// Parse via `"<hex>".parse::<Sha256Digest>()`, matching [`Sha256Digest::parse`].
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Sha256Digest::parse(value).ok_or("expected exactly 64 hex digits")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_matches_sha2_direct() {
        assert_eq!(
            sha256_hex(b"optimus"),
            format!("{:x}", Sha256::digest(b"optimus"))
        );
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_hex_matches_known_fips_vector() {
        // FIPS 180-4 test vector for the ASCII string "abc"; pins the output
        // to a hardcoded constant so a regression in the sha2 crate is caught
        // even if the self-matching check above drifts.
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn is_sha256_hex_accepts_only_64_hex_digits() {
        let ok = "a".repeat(64);
        assert!(is_sha256_hex(&ok));
        assert!(is_sha256_hex(&ok.to_uppercase()));
        assert!(!is_sha256_hex("short"));
        assert!(!is_sha256_hex(&"a".repeat(63)));
        assert!(!is_sha256_hex(&"a".repeat(65)));
        assert!(!is_sha256_hex(&"g".repeat(64)));
        assert!(!is_sha256_hex(""));
    }

    #[test]
    fn sha256_digest_parse_round_trips() {
        let digest = Sha256Digest::digest(b"optimus");
        assert_eq!(digest.as_str().len(), 64);
        let parsed = Sha256Digest::parse(digest.as_str()).expect("canonical form parses");
        assert_eq!(parsed, digest);
        assert_eq!(Sha256Digest::parse(&"g".repeat(64)), None);
        assert_eq!(Sha256Digest::parse(&"a".repeat(63)), None);
    }

    #[test]
    fn sha256_digest_normalizes_case() {
        let upper = Sha256Digest::parse(&"A".repeat(64)).expect("upper-case hex parses");
        assert_eq!(upper.as_str(), &"a".repeat(64));
    }

    #[test]
    fn sha256_digest_from_str_parses_and_rejects() {
        let digest = Sha256Digest::digest(b"optimus");
        let parsed: Sha256Digest = digest.as_str().parse().expect("canonical form parses");
        assert_eq!(parsed, digest);
        // FromStr must reject the same inputs as parse().
        assert!(&"g".repeat(64).parse::<Sha256Digest>().is_err());
        assert!(&"a".repeat(63).parse::<Sha256Digest>().is_err());
        assert!(&"".parse::<Sha256Digest>().is_err());
    }

    #[test]
    fn sha256_digest_serializes_transparently() {
        let digest = Sha256Digest::digest(b"optimus");
        let json = serde_json::to_string(&digest).expect("serializes");
        assert_eq!(json, format!("\"{}\"", digest.as_str()));
        let back: Sha256Digest = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back, digest);
    }
}
