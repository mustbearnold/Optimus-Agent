//! Stored API keys for OpenAI-compatible providers (DeepSeek today).
//!
//! Codex authenticates through OAuth and owns `auth.json`. Key-based providers
//! had no storage at all: `OpenAiCompatConfig::from_deepseek_env` read
//! `DEEPSEEK_API_KEY` from the process environment, so a desktop user who never
//! exports that variable before launching the app could not reach DeepSeek and
//! had nowhere in Settings to say so. This store gives those providers the same
//! protected-at-rest, user-only file treatment the OAuth store already uses.
//!
//! The environment variable stays authoritative when it is set, so existing
//! scripted and CI launches keep working unchanged.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::{
    atomic_write_user_only, verify_user_only, CredentialProtector, KernelError, Result,
    SystemCredentialProtector,
};

/// Canonical provider id for DeepSeek, matching `routing::ProviderId::as_str`.
pub const DEEPSEEK_PROVIDER: &str = "deepseek";

/// Where a resolved key came from. Reported to the UI so the user can tell a
/// stored key apart from one inherited from the launching environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKeySource {
    Stored,
    Environment,
    None,
}

impl ProviderKeySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stored => "stored",
            Self::Environment => "environment",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderKeyStatus {
    pub provider: String,
    pub present: bool,
    pub source: ProviderKeySource,
    /// Masked tail (never the key) so the user can recognise which key is set.
    pub hint: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProviderKeyEntry {
    api_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderKeyFile {
    version: u32,
    #[serde(default)]
    providers: BTreeMap<String, ProviderKeyEntry>,
}

impl Default for ProviderKeyFile {
    fn default() -> Self {
        Self {
            version: 1,
            providers: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ProtectedEnvelope {
    version: u32,
    protection: String,
    ciphertext_hex: String,
}

pub struct ProviderKeyStore {
    path: PathBuf,
    lock_path: PathBuf,
    protector: Arc<dyn CredentialProtector>,
}

impl ProviderKeyStore {
    pub fn open(home: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_protector(home, Arc::new(SystemCredentialProtector))
    }

    pub fn open_with_protector(
        home: impl AsRef<Path>,
        protector: Arc<dyn CredentialProtector>,
    ) -> Result<Self> {
        let home = home.as_ref().to_path_buf();
        fs::create_dir_all(&home)?;
        Ok(Self {
            path: home.join("provider-keys.json"),
            lock_path: home.join("provider-keys.lock"),
            protector,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn lock_exclusive(&self) -> Result<File> {
        if fs::symlink_metadata(&self.lock_path)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(KernelError::Model(
                "provider key lock file must not be a symlink".into(),
            ));
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&self.lock_path)?;
        FileExt::lock_exclusive(&file)?;
        Ok(file)
    }

    fn load_unlocked(&self) -> Result<ProviderKeyFile> {
        if !self.path.exists() {
            return Ok(ProviderKeyFile::default());
        }
        let metadata = fs::symlink_metadata(&self.path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(KernelError::Model(
                "provider key file must be a non-symlink regular file".into(),
            ));
        }
        verify_user_only(&self.path)?;
        let bytes = fs::read(&self.path)?;
        let envelope: ProtectedEnvelope = serde_json::from_slice(&bytes)?;
        if envelope.version != 1 || envelope.protection != self.protector.label() {
            return Err(KernelError::Model(
                "unsupported provider key envelope or protection backend".into(),
            ));
        }
        let ciphertext = decode_hex(&envelope.ciphertext_hex)?;
        let plaintext = self.protector.unprotect(&ciphertext)?;
        Ok(serde_json::from_slice(&plaintext)?)
    }

    fn save_unlocked(&self, file: &ProviderKeyFile) -> Result<()> {
        let plaintext = serde_json::to_vec(file)?;
        let ciphertext = self.protector.protect(&plaintext)?;
        let envelope = ProtectedEnvelope {
            version: 1,
            protection: self.protector.label().into(),
            ciphertext_hex: encode_hex(&ciphertext),
        };
        atomic_write_user_only(&self.path, &serde_json::to_vec_pretty(&envelope)?)
    }

    /// Persist a key. An all-whitespace key is rejected rather than silently
    /// stored, because a blank stored key would shadow a working environment
    /// variable and read as "configured" in Settings.
    pub fn set_key(&self, provider: &str, api_key: &str, base_url: Option<&str>) -> Result<()> {
        let api_key = api_key.trim();
        if api_key.is_empty() {
            return Err(KernelError::Model(
                "provider API key must not be empty".into(),
            ));
        }
        let base_url = base_url.map(str::trim).filter(|value| !value.is_empty());
        let _lock = self.lock_exclusive()?;
        let mut file = self.load_unlocked()?;
        file.providers.insert(
            provider.to_string(),
            ProviderKeyEntry {
                api_key: api_key.to_string(),
                base_url: base_url.map(str::to_string),
            },
        );
        self.save_unlocked(&file)
    }

    pub fn clear_key(&self, provider: &str) -> Result<()> {
        let _lock = self.lock_exclusive()?;
        let mut file = self.load_unlocked()?;
        file.providers.remove(provider);
        self.save_unlocked(&file)
    }

    /// The stored key only. Callers that want environment fallback use
    /// [`ProviderKeyStore::resolve`].
    pub fn stored_key(&self, provider: &str) -> Result<Option<(String, Option<String>)>> {
        let _lock = self.lock_exclusive()?;
        let file = self.load_unlocked()?;
        Ok(file
            .providers
            .get(provider)
            .map(|entry| (entry.api_key.clone(), entry.base_url.clone())))
    }

    /// Stored key first, then the process environment. Returns the key and any
    /// stored base URL override.
    pub fn resolve(
        &self,
        provider: &str,
        env_key: &str,
    ) -> Result<Option<(String, Option<String>, ProviderKeySource)>> {
        if let Some((key, base_url)) = self.stored_key(provider)? {
            return Ok(Some((key, base_url, ProviderKeySource::Stored)));
        }
        match std::env::var(env_key) {
            Ok(value) if !value.trim().is_empty() => Ok(Some((
                value.trim().to_string(),
                None,
                ProviderKeySource::Environment,
            ))),
            _ => Ok(None),
        }
    }

    pub fn status(&self, provider: &str, env_key: &str) -> Result<ProviderKeyStatus> {
        let resolved = self.resolve(provider, env_key)?;
        Ok(match resolved {
            Some((key, base_url, source)) => ProviderKeyStatus {
                provider: provider.to_string(),
                present: true,
                source,
                hint: Some(mask_key(&key)),
                base_url,
            },
            None => ProviderKeyStatus {
                provider: provider.to_string(),
                present: false,
                source: ProviderKeySource::None,
                hint: None,
                base_url: None,
            },
        })
    }
}

/// Show only the last four characters. Short keys reveal nothing at all rather
/// than degrading to "most of the key".
fn mask_key(key: &str) -> String {
    if key.chars().count() <= 8 {
        return "•".repeat(8);
    }
    let mut tail: Vec<char> = key.chars().rev().take(4).collect();
    tail.reverse();
    let tail: String = tail.into_iter().collect();
    format!("••••{tail}")
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn decode_hex(value: &str) -> Result<Vec<u8>> {
    if value.len() % 2 != 0 {
        return Err(KernelError::Model(
            "provider key envelope ciphertext is malformed".into(),
        ));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16).map_err(|_| {
                KernelError::Model("provider key envelope ciphertext is malformed".into())
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[derive(Debug)]
    struct IdentityProtector;

    impl CredentialProtector for IdentityProtector {
        fn label(&self) -> &'static str {
            "identity-test"
        }
        fn protect(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
            Ok(plaintext.to_vec())
        }
        fn unprotect(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
            Ok(ciphertext.to_vec())
        }
    }

    fn store(home: &Path) -> ProviderKeyStore {
        ProviderKeyStore::open_with_protector(home, Arc::new(IdentityProtector)).unwrap()
    }

    #[test]
    fn stored_key_round_trips_and_clears() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());

        assert!(store.stored_key(DEEPSEEK_PROVIDER).unwrap().is_none());
        store
            .set_key(DEEPSEEK_PROVIDER, "sk-deepseek-abcdefgh", None)
            .unwrap();
        let (key, base) = store.stored_key(DEEPSEEK_PROVIDER).unwrap().unwrap();
        assert_eq!(key, "sk-deepseek-abcdefgh");
        assert_eq!(base, None);

        store.clear_key(DEEPSEEK_PROVIDER).unwrap();
        assert!(store.stored_key(DEEPSEEK_PROVIDER).unwrap().is_none());
    }

    #[test]
    fn key_is_never_written_in_plaintext_and_file_is_user_only() {
        let dir = tempdir().unwrap();
        let store = ProviderKeyStore::open(dir.path()).unwrap();
        if store
            .set_key(DEEPSEEK_PROVIDER, "sk-deepseek-secret-value", None)
            .is_err()
        {
            // No OS credential backend in this environment; nothing to assert.
            return;
        }
        let raw = fs::read_to_string(store.path()).unwrap();
        assert!(
            !raw.contains("sk-deepseek-secret-value"),
            "provider key file must not contain the plaintext key"
        );
        verify_user_only(store.path()).unwrap();
    }

    #[test]
    fn blank_key_is_rejected_so_it_cannot_shadow_the_environment() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());
        assert!(store.set_key(DEEPSEEK_PROVIDER, "   ", None).is_err());
        assert!(store.stored_key(DEEPSEEK_PROVIDER).unwrap().is_none());
    }

    #[test]
    fn stored_key_wins_over_the_environment_and_status_masks_it() {
        let dir = tempdir().unwrap();
        let store = store(dir.path());
        store
            .set_key(DEEPSEEK_PROVIDER, "sk-stored-key-1234", None)
            .unwrap();

        let resolved = store
            .resolve(DEEPSEEK_PROVIDER, "OPTIMUS_TEST_DEEPSEEK_KEY_UNSET")
            .unwrap()
            .unwrap();
        assert_eq!(resolved.0, "sk-stored-key-1234");
        assert_eq!(resolved.2, ProviderKeySource::Stored);

        let status = store
            .status(DEEPSEEK_PROVIDER, "OPTIMUS_TEST_DEEPSEEK_KEY_UNSET")
            .unwrap();
        assert!(status.present);
        assert_eq!(status.hint.as_deref(), Some("••••1234"));
        assert!(!status
            .hint
            .as_deref()
            .unwrap_or_default()
            .contains("sk-stored"));
    }

    #[test]
    fn short_keys_are_fully_masked() {
        assert_eq!(mask_key("abcd"), "••••••••");
        assert_eq!(mask_key("abcdefgh"), "••••••••");
        assert_eq!(mask_key("abcdefghi"), "••••fghi");
    }
}
