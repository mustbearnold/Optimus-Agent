//! Credential protection and atomic, user-only file persistence.

use std::path::Path;

use crate::{KernelError, Result};

pub trait CredentialProtector: Send + Sync {
    fn label(&self) -> &'static str;
    fn protect(&self, plaintext: &[u8]) -> Result<Vec<u8>>;
    fn unprotect(&self, ciphertext: &[u8]) -> Result<Vec<u8>>;
}

#[derive(Debug, Default)]
pub struct SystemCredentialProtector;

impl CredentialProtector for SystemCredentialProtector {
    fn label(&self) -> &'static str {
        if cfg!(windows) {
            "dpapi-current-user"
        } else if cfg!(target_os = "linux") {
            "secret-service-aes256gcm-v1"
        } else {
            "unavailable"
        }
    }

    fn protect(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        platform::protect(plaintext)
    }

    fn unprotect(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        platform::unprotect(ciphertext)
    }
}

pub fn atomic_write_user_only(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| KernelError::Model("credential path has no parent".into()))?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("credential"),
        uuid::Uuid::new_v4()
    ));
    let result = (|| -> Result<()> {
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        platform::harden_user_only(&temporary)?;
        platform::atomic_replace(&temporary, path)?;
        platform::harden_user_only(path)?;
        verify_user_only(path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub fn verify_user_only(path: &Path) -> Result<()> {
    platform::verify_user_only(path)
}

pub fn harden_user_only(path: &Path) -> Result<()> {
    platform::harden_user_only(path)
}

#[cfg(windows)]
mod platform {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    use windows_sys::Win32::Security::{
        GetFileSecurityW, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
        SetFileSecurityW, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
        SE_DACL_PROTECTED,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    use crate::{KernelError, Result};

    const ENTROPY: &[u8] = b"Optimus Agent/CodexAuth/v2";

    pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>> {
        crypt(plaintext, true)
    }

    pub fn unprotect(ciphertext: &[u8]) -> Result<Vec<u8>> {
        crypt(ciphertext, false)
    }

    fn crypt(input: &[u8], protect: bool) -> Result<Vec<u8>> {
        let input_len = u32::try_from(input.len())
            .map_err(|_| KernelError::Model("credential payload is too large".into()))?;
        let entropy_len = u32::try_from(ENTROPY.len())
            .map_err(|_| KernelError::Model("credential entropy is too large".into()))?;
        let input_blob = CRYPT_INTEGER_BLOB {
            cbData: input_len,
            pbData: input.as_ptr() as *mut u8,
        };
        let entropy_blob = CRYPT_INTEGER_BLOB {
            cbData: entropy_len,
            pbData: ENTROPY.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: null_mut(),
        };
        let success = unsafe {
            if protect {
                CryptProtectData(
                    &input_blob,
                    null(),
                    &entropy_blob,
                    null(),
                    null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &input_blob,
                    null_mut(),
                    &entropy_blob,
                    null(),
                    null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            }
        };
        if success == 0 {
            return Err(KernelError::Io(std::io::Error::last_os_error()));
        }
        let bytes =
            unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
        unsafe {
            LocalFree(output.pbData.cast());
        }
        Ok(bytes)
    }

    pub fn harden_user_only(path: &Path) -> Result<()> {
        let sddl: Vec<u16> = OsStr::new("D:P(A;;FA;;;OW)")
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut descriptor = null_mut();
        let converted = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        };
        if converted == 0 {
            return Err(KernelError::Io(std::io::Error::last_os_error()));
        }
        let wide = wide_path(path);
        let applied = unsafe {
            SetFileSecurityW(
                wide.as_ptr(),
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                descriptor,
            )
        };
        unsafe {
            LocalFree(descriptor);
        }
        if applied == 0 {
            return Err(KernelError::Io(std::io::Error::last_os_error()));
        }
        Ok(())
    }

    pub fn verify_user_only(path: &Path) -> Result<()> {
        let wide = wide_path(path);
        let requested = DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION;
        let mut needed = 0u32;
        unsafe {
            GetFileSecurityW(wide.as_ptr(), requested, null_mut(), 0, &mut needed);
        }
        if needed == 0 {
            return Err(KernelError::Io(std::io::Error::last_os_error()));
        }
        let mut descriptor = vec![0u8; needed as usize];
        let loaded = unsafe {
            GetFileSecurityW(
                wide.as_ptr(),
                requested,
                descriptor.as_mut_ptr() as *mut _,
                needed,
                &mut needed,
            )
        };
        if loaded == 0 {
            return Err(KernelError::Io(std::io::Error::last_os_error()));
        }
        let mut control = 0u16;
        let mut revision = 0u32;
        let control_ok = unsafe {
            GetSecurityDescriptorControl(descriptor.as_ptr() as *mut _, &mut control, &mut revision)
        };
        let mut present = 0;
        let mut defaulted = 0;
        let mut dacl = null_mut();
        let dacl_ok = unsafe {
            GetSecurityDescriptorDacl(
                descriptor.as_ptr() as *mut _,
                &mut present,
                &mut dacl,
                &mut defaulted,
            )
        };
        if control_ok == 0
            || dacl_ok == 0
            || control & SE_DACL_PROTECTED == 0
            || present == 0
            || dacl.is_null()
        {
            return Err(KernelError::Model(
                "credential file does not have a protected explicit DACL".into(),
            ));
        }
        Ok(())
    }

    pub fn atomic_replace(from: &Path, to: &Path) -> Result<()> {
        let from = wide_path(from);
        let to = wide_path(to);
        let moved = unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if moved == 0 {
            return Err(KernelError::Io(std::io::Error::last_os_error()));
        }
        Ok(())
    }

    fn wide_path(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::{Path, PathBuf};

    use crate::{KernelError, Result};

    #[cfg(target_os = "linux")]
    const KEYRING_SERVICE: &str = "optimus-agent";
    #[cfg(target_os = "linux")]
    const KEYRING_USER: &str = "codex-auth-v2-master-key";
    #[cfg(target_os = "linux")]
    const NONCE_LEN: usize = 12;
    #[cfg(target_os = "linux")]
    const KEY_LEN: usize = 32;

    #[cfg(target_os = "linux")]
    pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>> {
        let key = load_or_create_master_key()?;
        protect_with_key(&key, plaintext)
    }

    #[cfg(target_os = "linux")]
    pub fn unprotect(ciphertext: &[u8]) -> Result<Vec<u8>> {
        let key = load_or_create_master_key()?;
        unprotect_with_key(&key, ciphertext)
    }

    #[cfg(not(target_os = "linux"))]
    pub fn protect(_plaintext: &[u8]) -> Result<Vec<u8>> {
        Err(KernelError::Model(
            "credential encryption backend is unavailable on this platform".into(),
        ))
    }

    #[cfg(not(target_os = "linux"))]
    pub fn unprotect(_ciphertext: &[u8]) -> Result<Vec<u8>> {
        Err(KernelError::Model(
            "credential encryption backend is unavailable on this platform".into(),
        ))
    }

    #[cfg(target_os = "linux")]
    pub(super) fn protect_with_key(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
        use aes_gcm::Aes256Gcm;

        let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| {
            KernelError::Model("credential encryption key has invalid length".into())
        })?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| KernelError::Model("credential encryption failed".into()))?;
        let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        out.extend_from_slice(nonce.as_slice());
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    #[cfg(target_os = "linux")]
    pub(super) fn unprotect_with_key(key: &[u8; KEY_LEN], blob: &[u8]) -> Result<Vec<u8>> {
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        if blob.len() <= NONCE_LEN {
            return Err(KernelError::Model(
                "credential ciphertext is truncated or malformed".into(),
            ));
        }
        let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| {
            KernelError::Model("credential encryption key has invalid length".into())
        })?;
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| KernelError::Model("credential decryption failed".into()))
    }

    #[cfg(target_os = "linux")]
    fn load_or_create_master_key() -> Result<[u8; KEY_LEN]> {
        use keyring::{Entry, Error as KeyringError};

        let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(map_keyring_error)?;
        load_or_create_master_key_locked(
            &global_master_key_lock_path(),
            || match entry.get_secret() {
                Ok(secret) => Ok(Some(secret)),
                Err(KeyringError::NoEntry) => Ok(None),
                Err(error) => Err(map_keyring_error(error)),
            },
            |key| {
                entry
                    .set_password(&encode_master_key(key))
                    .map_err(map_keyring_error)
            },
            || {
                use aes_gcm::aead::rand_core::RngCore;
                use aes_gcm::aead::OsRng;

                let mut key = [0u8; KEY_LEN];
                OsRng.fill_bytes(&mut key);
                key
            },
        )
    }

    #[cfg(target_os = "linux")]
    pub(super) fn encode_master_key(key: &[u8]) -> String {
        use base64::Engine as _;

        base64::engine::general_purpose::STANDARD.encode(key)
    }

    #[cfg(target_os = "linux")]
    fn global_master_key_lock_path() -> PathBuf {
        PathBuf::from("/run/user")
            .join(unsafe { libc::geteuid() }.to_string())
            .join("optimus-agent-master-key.lock")
    }

    #[cfg(target_os = "linux")]
    pub(super) fn load_or_create_master_key_locked<Get, Set, Generate>(
        lock_path: &Path,
        mut get_secret: Get,
        mut set_secret: Set,
        mut generate_key: Generate,
    ) -> Result<[u8; KEY_LEN]>
    where
        Get: FnMut() -> Result<Option<Vec<u8>>>,
        Set: FnMut(&[u8]) -> Result<()>,
        Generate: FnMut() -> [u8; KEY_LEN],
    {
        let _lock = lock_master_key_initialization(lock_path)?;
        if let Some(secret) = get_secret()? {
            return decode_master_key(&secret);
        }

        let candidate = generate_key();
        set_secret(&candidate)?;
        let persisted = get_secret()?.ok_or_else(|| {
            KernelError::Model("Secret Service did not persist the credential key".into())
        })?;
        let persisted = decode_master_key(&persisted)?;
        if persisted != candidate {
            return Err(KernelError::Model(
                "Secret Service credential key changed during initialization".into(),
            ));
        }
        Ok(persisted)
    }

    #[cfg(target_os = "linux")]
    pub(super) fn decode_master_key(secret: &[u8]) -> Result<[u8; KEY_LEN]> {
        use base64::Engine as _;

        let decoded;
        let secret = if secret.len() == KEY_LEN {
            secret
        } else {
            decoded = base64::engine::general_purpose::STANDARD
                .decode(secret)
                .map_err(|_| {
                    KernelError::Model("Secret Service returned a malformed credential key".into())
                })?;
            decoded.as_slice()
        };
        if secret.len() != KEY_LEN {
            return Err(KernelError::Model(
                "Secret Service returned a credential key with unexpected length".into(),
            ));
        }
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(secret);
        Ok(key)
    }

    #[cfg(target_os = "linux")]
    fn lock_master_key_initialization(path: &Path) -> Result<std::fs::File> {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

        use fs2::FileExt;

        let parent = path
            .parent()
            .ok_or_else(|| KernelError::Model("credential master-key lock has no parent".into()))?;
        let parent_metadata = std::fs::symlink_metadata(parent)?;
        let euid = unsafe { libc::geteuid() };
        if parent_metadata.file_type().is_symlink()
            || !parent_metadata.is_dir()
            || parent_metadata.uid() != euid
            || parent_metadata.permissions().mode() & 0o077 != 0
        {
            return Err(KernelError::Model(
                "credential master-key lock directory is not private to the current user".into(),
            ));
        }

        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.uid() != euid
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(KernelError::Model(
                "credential master-key lock is not an owner-only regular file".into(),
            ));
        }
        FileExt::lock_exclusive(&file)?;
        Ok(file)
    }

    #[cfg(target_os = "linux")]
    fn map_keyring_error(error: keyring::Error) -> KernelError {
        KernelError::Model(format!(
            "Secret Service credential backend unavailable: {error}. Unlock GNOME Keyring / org.freedesktop.secrets and retry."
        ))
    }

    #[cfg(unix)]
    pub fn harden_user_only(path: &Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.file_type().is_file() {
            return Err(KernelError::Model(
                "credential path is not a regular file".into(),
            ));
        }
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(path, permissions)?;
        Ok(())
    }

    #[cfg(unix)]
    pub fn verify_user_only(path: &Path) -> Result<()> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.file_type().is_file() {
            return Err(KernelError::Model(
                "credential path is not a regular file".into(),
            ));
        }
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(KernelError::Model(format!(
                "credential file mode {mode:04o} is not owner-only"
            )));
        }
        let euid = unsafe { libc::geteuid() };
        if metadata.uid() != euid {
            return Err(KernelError::Model(
                "credential file is not owned by the current user".into(),
            ));
        }
        Ok(())
    }

    #[cfg(unix)]
    pub fn atomic_replace(from: &Path, to: &Path) -> Result<()> {
        std::fs::rename(from, to)?;
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn harden_user_only(_path: &Path) -> Result<()> {
        Err(KernelError::Model(
            "credential ACL backend is unavailable on this platform".into(),
        ))
    }

    #[cfg(not(unix))]
    pub fn verify_user_only(_path: &Path) -> Result<()> {
        Err(KernelError::Model(
            "credential ACL backend is unavailable on this platform".into(),
        ))
    }

    #[cfg(not(unix))]
    pub fn atomic_replace(_from: &Path, _to: &Path) -> Result<()> {
        Err(KernelError::Model(
            "atomic credential replacement is unavailable on this platform".into(),
        ))
    }
}

#[cfg(all(test, unix))]
mod unix_file_tests {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    use tempfile::tempdir;

    use super::{atomic_write_user_only, verify_user_only};

    #[test]
    fn atomic_credential_replace_is_owner_only_and_replaces_bytes() {
        let root = tempdir().expect("tempdir");
        let path = root.path().join("auth.json");

        atomic_write_user_only(&path, b"first").expect("first write");
        atomic_write_user_only(&path, b"second").expect("replacement write");

        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        assert!(metadata.file_type().is_file());
        assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        verify_user_only(&path).expect("verified owner-only file");
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_crypto_tests {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;
    use std::time::Duration;

    use tempfile::tempdir;

    use super::platform::{
        decode_master_key, encode_master_key, load_or_create_master_key_locked, protect_with_key,
        unprotect_with_key,
    };

    #[test]
    fn secret_service_key_encrypts_with_authentication_and_random_nonce() {
        let key = [0x5au8; 32];
        let first = protect_with_key(&key, b"credential-sentinel").expect("protect");
        let second = protect_with_key(&key, b"credential-sentinel").expect("protect again");

        assert_ne!(first, second);
        assert!(!first
            .windows(b"credential-sentinel".len())
            .any(|window| window == b"credential-sentinel"));
        assert_eq!(
            unprotect_with_key(&key, &first).expect("unprotect"),
            b"credential-sentinel"
        );

        let mut tampered = first;
        let last = tampered.last_mut().expect("ciphertext byte");
        *last ^= 1;
        assert!(unprotect_with_key(&key, &tampered).is_err());
    }

    #[test]
    fn secret_service_master_key_is_stored_as_utf8_text() {
        let key = [0xff_u8; 32];
        let encoded = encode_master_key(&key);

        assert!(encoded.is_ascii());
        assert_eq!(decode_master_key(encoded.as_bytes()).unwrap(), key);
        assert_eq!(decode_master_key(&key).unwrap(), key);
    }

    #[test]
    fn global_lock_serializes_first_use_master_key_creation() {
        let root = tempdir().expect("tempdir");
        let mut permissions = std::fs::metadata(root.path()).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(root.path(), permissions).unwrap();
        let lock_path = Arc::new(root.path().join("master-key.lock"));
        let backend = Arc::new(Mutex::new(None::<Vec<u8>>));
        let writes = Arc::new(AtomicUsize::new(0));
        let start = Arc::new(Barrier::new(2));
        let mut workers = Vec::new();

        for marker in [0x11_u8, 0x22_u8] {
            let lock_path = Arc::clone(&lock_path);
            let backend = Arc::clone(&backend);
            let writes = Arc::clone(&writes);
            let start = Arc::clone(&start);
            workers.push(thread::spawn(move || {
                start.wait();
                load_or_create_master_key_locked(
                    &lock_path,
                    || {
                        let value = backend.lock().unwrap().clone();
                        if value.is_none() {
                            thread::sleep(Duration::from_millis(40));
                        }
                        Ok(value)
                    },
                    |key| {
                        writes.fetch_add(1, Ordering::SeqCst);
                        *backend.lock().unwrap() = Some(key.to_vec());
                        Ok(())
                    },
                    || [marker; 32],
                )
                .expect("load or create key")
            }));
        }

        let first = workers.remove(0).join().unwrap();
        let second = workers.remove(0).join().unwrap();
        assert_eq!(first, second);
        assert_eq!(writes.load(Ordering::SeqCst), 1);
    }
}
