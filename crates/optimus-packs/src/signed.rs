//! Signed pack manifests (program P27.e).
//!
//! Default posture: **reject unsigned**. Trust root is a named key id + secret
//! used for HMAC-SHA256 over a canonical manifest body. Permission ceilings are
//! sets of allowed [`ToolPolicy`] values — tools outside the ceiling fail closed
//! and cannot escalate past SmartDeny classes.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{PackError, ToolPolicy};

/// Named trust root for pack signature verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustRoot {
    pub key_id: String,
    /// HMAC secret material (operator-managed; never embedded in packs).
    pub secret_hex: String,
}

impl TrustRoot {
    pub fn secret_bytes(&self) -> Result<Vec<u8>, PackError> {
        hex_decode(&self.secret_hex).map_err(|e| PackError::Msg(e))
    }
}

/// Unsigned pack body that becomes the signature subject.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackManifestBody {
    pub pack_id: String,
    pub version: String,
    pub tool_ids: Vec<String>,
    /// Policies this pack may advertise (intersection with host SmartDeny).
    pub max_policies: Vec<ToolPolicy>,
}

/// Signed envelope. Unsigned loads are rejected by default.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedPackManifest {
    pub body: PackManifestBody,
    pub key_id: String,
    pub signature_hex: String,
}

impl PackManifestBody {
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, PackError> {
        serde_json::to_vec(self).map_err(|e| PackError::Msg(e.to_string()))
    }
}

/// HMAC-SHA256(secret, body_json) as lowercase hex.
pub fn sign_manifest(root: &TrustRoot, body: &PackManifestBody) -> Result<SignedPackManifest, PackError> {
    let secret = root.secret_bytes()?;
    let bytes = body.canonical_bytes()?;
    let signature_hex = hmac_sha256_hex(&secret, &bytes);
    Ok(SignedPackManifest {
        body: body.clone(),
        key_id: root.key_id.clone(),
        signature_hex,
    })
}

/// Verify signature against trust root. Fail closed on key mismatch or bad MAC.
pub fn verify_manifest(
    root: &TrustRoot,
    signed: &SignedPackManifest,
) -> Result<PackManifestBody, PackError> {
    if signed.key_id != root.key_id {
        return Err(PackError::Msg(format!(
            "pack trust key mismatch: got {}, want {}",
            signed.key_id, root.key_id
        )));
    }
    let secret = root.secret_bytes()?;
    let bytes = signed.body.canonical_bytes()?;
    let expected = hmac_sha256_hex(&secret, &bytes);
    if !constant_time_eq(expected.as_bytes(), signed.signature_hex.as_bytes()) {
        return Err(PackError::Msg("pack signature verification failed".into()));
    }
    Ok(signed.body.clone())
}

/// Default reject: missing signature file or empty signature.
pub fn load_signed_manifest_file(
    path: impl AsRef<Path>,
    root: &TrustRoot,
) -> Result<PackManifestBody, PackError> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(PackError::Msg(format!(
            "unsigned pack rejected: missing manifest {}",
            path.display()
        )));
    }
    let raw = std::fs::read_to_string(path).map_err(|e| PackError::Msg(e.to_string()))?;
    let signed: SignedPackManifest =
        serde_json::from_str(&raw).map_err(|e| PackError::Msg(e.to_string()))?;
    if signed.signature_hex.trim().is_empty() {
        return Err(PackError::Msg("unsigned pack rejected: empty signature".into()));
    }
    verify_manifest(root, &signed)
}

/// Fail closed if tool policy is outside the pack ceiling.
pub fn assert_policy_within_ceiling(
    policy: ToolPolicy,
    ceiling: &[ToolPolicy],
) -> Result<(), PackError> {
    if ceiling.contains(&policy) {
        Ok(())
    } else {
        Err(PackError::Msg(format!(
            "pack permission ceiling denied policy {policy:?}"
        )))
    }
}

/// SmartDeny-aligned default ceiling for third-party packs (no Process/NetworkWrite/Desktop).
pub fn default_third_party_ceiling() -> Vec<ToolPolicy> {
    vec![
        ToolPolicy::WorkspaceRead,
        ToolPolicy::WorkspaceWrite,
        ToolPolicy::NetworkRead,
        ToolPolicy::MemoryRead,
        ToolPolicy::SkillRead,
        ToolPolicy::Browser,
        ToolPolicy::UserInteraction,
        ToolPolicy::Capability,
    ]
}

fn hmac_sha256_hex(secret: &[u8], message: &[u8]) -> String {
    // HMAC-SHA256 without extra crates: ipad/opad construction.
    const BLOCK: usize = 64;
    let mut key = secret.to_vec();
    if key.len() > BLOCK {
        key = Sha256::digest(&key).to_vec();
    }
    if key.len() < BLOCK {
        key.resize(BLOCK, 0);
    }
    let mut ipad = vec![0x36u8; BLOCK];
    let mut opad = vec![0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= key[i];
        opad[i] ^= key[i];
    }
    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(message);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(inner_hash);
    hex_encode(&outer.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn hex_decode(input: &str) -> Result<Vec<u8>, String> {
    let input = input.trim();
    if input.len() % 2 != 0 {
        return Err("odd hex length".into());
    }
    let mut out = Vec::with_capacity(input.len() / 2);
    let bytes = input.as_bytes();
    for chunk in bytes.chunks(2) {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err("invalid hex".into()),
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root() -> TrustRoot {
        TrustRoot {
            key_id: "dev-root-1".into(),
            secret_hex: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".into(),
        }
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let root = test_root();
        let body = PackManifestBody {
            pack_id: "example.mcp".into(),
            version: "1.0.0".into(),
            tool_ids: vec!["mcp_echo".into()],
            max_policies: default_third_party_ceiling(),
        };
        let signed = sign_manifest(&root, &body).unwrap();
        let verified = verify_manifest(&root, &signed).unwrap();
        assert_eq!(verified.pack_id, "example.mcp");
    }

    #[test]
    fn unsigned_and_tampered_fail_closed() {
        let root = test_root();
        let body = PackManifestBody {
            pack_id: "x".into(),
            version: "1".into(),
            tool_ids: vec![],
            max_policies: default_third_party_ceiling(),
        };
        let mut signed = sign_manifest(&root, &body).unwrap();
        signed.signature_hex = String::new();
        assert!(verify_manifest(&root, &signed).is_err());
        signed.signature_hex = "deadbeef".into();
        assert!(verify_manifest(&root, &signed).is_err());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.json");
        assert!(load_signed_manifest_file(&path, &root).is_err());
    }

    #[test]
    fn permission_ceiling_blocks_escalation() {
        let ceiling = default_third_party_ceiling();
        assert!(assert_policy_within_ceiling(ToolPolicy::WorkspaceRead, &ceiling).is_ok());
        assert!(assert_policy_within_ceiling(ToolPolicy::Process, &ceiling).is_err());
        assert!(assert_policy_within_ceiling(ToolPolicy::NetworkWrite, &ceiling).is_err());
        assert!(assert_policy_within_ceiling(ToolPolicy::Desktop, &ceiling).is_err());
    }
}
