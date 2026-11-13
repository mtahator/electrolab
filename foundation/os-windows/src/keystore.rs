//! Secrets in Windows Credential Manager (generic credentials, per user).

use std::ffi::OsStr;

use ep_os::{KeyStore, OsError, Result};
use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
use windows_sys::Win32::Security::Credentials::{
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW,
    CredWriteW,
};

use crate::wide;

/// Credential Manager, with every name prefixed by the app's name.
#[derive(Clone, Debug)]
pub struct CredentialStore {
    prefix: String,
}

impl CredentialStore {
    /// Secrets stored as `<app>/<name>`.
    #[must_use]
    pub fn new(app: &str) -> Self {
        Self {
            prefix: format!("{app}/"),
        }
    }

    fn target(&self, name: &str) -> Vec<u16> {
        wide(OsStr::new(&format!("{}{name}", self.prefix)))
    }
}

fn last_error() -> std::io::Error {
    std::io::Error::last_os_error()
}

fn is_not_found(e: &std::io::Error) -> bool {
    e.raw_os_error() == i32::try_from(ERROR_NOT_FOUND).ok()
}

impl KeyStore for CredentialStore {
    #[allow(unsafe_code)]
    fn get(&self, name: &str) -> Result<Option<String>> {
        let target = self.target(name);
        let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `target` is a NUL-terminated UTF-16 string that outlives the call; `cred`
        // receives a buffer owned by the system, freed below with `CredFree`.
        let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &raw mut cred) };
        if ok == 0 {
            let e = last_error();
            return if is_not_found(&e) {
                Ok(None)
            } else {
                Err(e.into())
            };
        }
        // SAFETY: on success `cred` points to a valid CREDENTIALW whose blob has
        // `CredentialBlobSize` bytes; we copy them before freeing.
        let bytes = unsafe {
            let c = &*cred;
            let v = if c.CredentialBlob.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec()
            };
            CredFree(cred.cast());
            v
        };
        String::from_utf8(bytes).map(Some).map_err(|_| {
            OsError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "stored secret is not UTF-8",
            ))
        })
    }

    #[allow(unsafe_code)]
    fn set(&self, name: &str, secret: &str) -> Result<()> {
        let mut target = self.target(name);
        let mut blob = secret.as_bytes().to_vec();
        let size =
            u32::try_from(blob.len()).map_err(|_| OsError::Unsupported("secret too large"))?;
        // SAFETY: an all-zero CREDENTIALW is a valid "empty" value (null pointers, zero sizes).
        let mut cred: CREDENTIALW = unsafe { std::mem::zeroed() };
        cred.Type = CRED_TYPE_GENERIC;
        cred.TargetName = target.as_mut_ptr();
        cred.CredentialBlobSize = size;
        cred.CredentialBlob = blob.as_mut_ptr();
        cred.Persist = CRED_PERSIST_LOCAL_MACHINE;
        // SAFETY: `cred` and the buffers it points to (`target`, `blob`) live until the call
        // returns; the system copies them.
        let ok = unsafe { CredWriteW(&raw const cred, 0) };
        if ok == 0 {
            Err(last_error().into())
        } else {
            Ok(())
        }
    }

    #[allow(unsafe_code)]
    fn delete(&self, name: &str) -> Result<()> {
        let target = self.target(name);
        // SAFETY: `target` is a NUL-terminated UTF-16 string that outlives the call.
        let ok = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
        if ok == 0 {
            let e = last_error();
            if !is_not_found(&e) {
                return Err(e.into());
            }
        }
        Ok(())
    }
}
