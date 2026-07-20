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
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
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
    use std::path::Path;

    use crate::{KernelError, Result};

    pub fn protect(_plaintext: &[u8]) -> Result<Vec<u8>> {
        Err(KernelError::Model(
            "credential encryption backend is unavailable on this platform".into(),
        ))
    }

    pub fn unprotect(_ciphertext: &[u8]) -> Result<Vec<u8>> {
        Err(KernelError::Model(
            "credential encryption backend is unavailable on this platform".into(),
        ))
    }

    pub fn harden_user_only(_path: &Path) -> Result<()> {
        Err(KernelError::Model(
            "credential ACL backend is unavailable on this platform".into(),
        ))
    }

    pub fn verify_user_only(_path: &Path) -> Result<()> {
        Err(KernelError::Model(
            "credential ACL backend is unavailable on this platform".into(),
        ))
    }

    pub fn atomic_replace(_from: &Path, _to: &Path) -> Result<()> {
        Err(KernelError::Model(
            "atomic credential replacement is unavailable on this platform".into(),
        ))
    }
}
