//! Deterministic SHA-256 digest computation and validated hex identities.
//!
//! One place owns "what a SHA-256 digest looks like": exactly 64 ASCII hex
//! digits. Validation accepts either case; computed digests are lowercase
//! hex. Every crate validates or computes digests through this seam instead
//! of re-deriving the rule (previously ~14 hand-written copies across 10
//! crates, one of which had already drifted in its error text).

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
/// canonical form. Serializes transparently as the plain hex string, and
/// deserialization re-runs [`Sha256Digest::parse`] so an invalid digest can
/// never be constructed through serde (the `#[serde(transparent)]` derive
/// would otherwise copy any string straight into the inner `String`, bypassing
/// validation).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

/// Compare a digest against a hex string literal (`digest == "abc…"`).
///
/// Digests are case-insensitive by the crate's documented rule ("Validation
/// accepts either case"), and the canonical form is always lowercase. Comparing
/// with `eq_ignore_ascii_case` means an upper-case hex literal compares equal to
/// a lower-case digest instead of silently returning `false` (which contradicted
/// `is_sha256_hex`'s acceptance of either case).
impl PartialEq<str> for Sha256Digest {
    fn eq(&self, other: &str) -> bool {
        self.0.eq_ignore_ascii_case(other)
    }
}

impl PartialEq<&str> for Sha256Digest {
    fn eq(&self, other: &&str) -> bool {
        // Auto-deref resolves the double reference; an explicit deref would
        // be redundant (clippy::explicit-auto-deref under -D warnings).
        self.0.eq_ignore_ascii_case(other)
    }
}

impl PartialEq<String> for Sha256Digest {
    fn eq(&self, other: &String) -> bool {
        self.0.eq_ignore_ascii_case(other.as_str())
    }
}

// Symmetric comparisons (`literal == digest`). `PartialEq` is not
// automatically reversed for foreign types like `str`/`String`, so without
// these impls a caller could write `digest == literal` but not
// `literal == digest` — a one-sided rule that also silently differs from the
// documented case-insensitivity. Mirroring the forward impls keeps both
// directions case-insensitive and equivalent.

impl PartialEq<Sha256Digest> for str {
    fn eq(&self, other: &Sha256Digest) -> bool {
        self.eq_ignore_ascii_case(&other.0)
    }
}

impl PartialEq<Sha256Digest> for &str {
    fn eq(&self, other: &Sha256Digest) -> bool {
        self.eq_ignore_ascii_case(&other.0)
    }
}

impl PartialEq<Sha256Digest> for String {
    fn eq(&self, other: &Sha256Digest) -> bool {
        self.as_str().eq_ignore_ascii_case(&other.0)
    }
}

impl std::str::FromStr for Sha256Digest {
    type Err = &'static str;

    /// Parse via `"<hex>".parse::<Sha256Digest>()`, matching [`Sha256Digest::parse`].
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Sha256Digest::parse(value).ok_or("expected exactly 64 hex digits")
    }
}

impl TryFrom<&str> for Sha256Digest {
    type Error = &'static str;

    /// Parse via `Sha256Digest::try_from("<hex>")?`, matching [`Sha256Digest::parse`].
    ///
    /// Provides the standard fallible construction path (`?`-compatible with
    /// `Result<Sha256Digest, &'static str>`), symmetric with the existing
    /// `FromStr` impl so callers don't have to destructure an `Option`.
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Sha256Digest::parse(value).ok_or("expected exactly 64 hex digits")
    }
}

impl serde::Serialize for Sha256Digest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for Sha256Digest {
    /// Deserialize a digest, re-running validation so an invalid string can
    /// never smuggle itself into a `Sha256Digest` (the previous transparent
    /// derive copied the raw string in without checking).
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = <String as serde::Deserialize>::deserialize(deserializer)?;
        Sha256Digest::parse(&raw).ok_or_else(|| {
            serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(&raw),
                &"a 64-digit hex SHA-256 digest",
            )
        })
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
    fn sha256_hex_handles_arbitrary_binary_bytes() {
        // `sha256_hex` takes `&[u8]` and must digest arbitrary binary inputs,
        // not just printable ASCII strings. Pin a known digest for a byte
        // sequence that is not valid UTF-8 (0x00..=0xFF with no 7-bit ASCII).
        let bytes: Vec<u8> = (0u8..=255).collect();
        assert_eq!(sha256_hex(&bytes), format!("{:x}", Sha256::digest(&bytes)));
        // Deterministic across calls and stable for a mixed binary payload.
        let mixed = [0x00, 0xff, 0x10, 0xfe, 0x7f];
        assert_eq!(sha256_hex(&mixed), sha256_hex(&mixed));
        assert_ne!(sha256_hex(&mixed), sha256_hex(b""));
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
    fn sha256_digest_try_from_parses_and_rejects() {
        let digest = Sha256Digest::digest(b"optimus");
        let parsed = Sha256Digest::try_from(digest.as_str()).expect("canonical form parses");
        assert_eq!(parsed, digest);
        // TryFrom must reject the same inputs as parse()/FromStr.
        assert_eq!(
            Sha256Digest::try_from("g".repeat(64).as_str())
                .map_err(|_| "expected exactly 64 hex digits"),
            Err("expected exactly 64 hex digits")
        );
        assert!(Sha256Digest::try_from("a".repeat(63).as_str()).is_err());
        assert!(Sha256Digest::try_from("").is_err());
    }

    #[test]
    fn sha256_digest_comparison_against_str_literals() {
        let digest = Sha256Digest::digest(b"optimus");
        let hex = digest.as_str().to_string();

        // The canonical lower-case hex form compares equal to an `&str`
        // literal and a `str`.
        assert_eq!(digest, hex.as_str());

        // A digest computed from different bytes must not compare equal.
        let other = Sha256Digest::digest(b"other");
        assert_ne!(other, hex.as_str());
    }

    #[test]
    fn sha256_digest_comparison_is_case_insensitive() {
        // Regression: the documented rule says validation accepts either case,
        // but the old `==` compared the canonical lower-case form against the
        // literal case-sensitively. An upper-case hex literal equal to the same
        // digest used to compare `false`; it must compare equal.
        let digest = Sha256Digest::digest(b"optimus");
        let upper = digest.as_str().to_ascii_uppercase();

        // `&str` overload
        assert_eq!(digest, upper.as_str());
        // `str` overload
        let reference: &str = &upper;
        assert_eq!(digest, reference);
    }

    #[test]
    fn sha256_digest_comparison_against_string_owned() {
        // Regression/coverage: `PartialEq<String>` lets callers compare a
        // digest against an owned `String` (e.g. one read from config or a
        // store) directly, mirroring the existing `&str`/`str` impls. Without
        // it the comparison only compiles after a manual `.as_str()`, and an
        // upper-case owned string must still compare equal (case-insensitive
        // rule).
        let digest = Sha256Digest::digest(b"optimus");
        let owned = digest.as_str().to_string();

        assert_eq!(digest, owned);
        assert_eq!(digest, owned.to_ascii_uppercase());

        let other = Sha256Digest::digest(b"other");
        assert_ne!(other, owned);
    }

    #[test]
    fn sha256_digest_symmetric_comparison_with_str_literals() {
        // Regression: `PartialEq` is not reversed for foreign `str`/`String`
        // types, so `literal == digest` used to fail to compile (and, when it
        // did, was a separate hand-rolled comparison that could drift from the
        // documented case-insensitive rule). The literal-first direction must
        // compare equal for lower- and upper-case hex, and reject a different
        // digest — mirroring the digest-first direction.
        let digest = Sha256Digest::digest(b"optimus");
        let hex = digest.as_str().to_string();
        let upper = hex.to_ascii_uppercase();

        // `&str` literal first
        assert_eq!(hex.as_str(), digest);
        // `str` (unsized) first via a reference
        let reference: &str = &hex;
        assert_eq!(reference, digest);
        // owned `String` first
        assert_eq!(hex, digest);
        assert_eq!(upper, digest);

        let other = Sha256Digest::digest(b"other");
        assert_ne!(hex, other);
        assert_ne!(other.as_str(), digest);
    }

    #[test]
    fn sha256_digest_serializes_transparently() {
        let digest = Sha256Digest::digest(b"optimus");
        let json = serde_json::to_string(&digest).expect("serializes");
        assert_eq!(json, format!("\"{}\"", digest.as_str()));
        let back: Sha256Digest = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back, digest);
    }

    #[test]
    fn sha256_digest_deserialization_rejects_invalid_digests() {
        // The `#[serde(transparent)]` derive used to copy any string straight
        // into the inner `String`, letting a non-hex or wrong-length value
        // construct a `Sha256Digest` that violated the documented invariant.
        // Deserialization must re-run validation and fail closed instead.
        for bad in [
            format!("\"{}\"", "g".repeat(64)), // 64 non-hex chars
            format!("\"{}\"", "a".repeat(63)), // 63 hex chars
            format!("\"{}\"", "a".repeat(65)), // 65 hex chars
            "\"\"".to_string(),                // empty
        ] {
            let result: Result<Sha256Digest, _> = serde_json::from_str(&bad);
            assert!(
                result.is_err(),
                "invalid digest must not deserialize: {bad}"
            );
        }
        // A valid (upper-case) digest still round-trips and normalizes.
        let upper = serde_json::from_str::<Sha256Digest>(&format!("\"{}\"", "A".repeat(64)))
            .expect("upper-case hex parses");
        assert_eq!(upper.as_str(), &"a".repeat(64));
    }
}
