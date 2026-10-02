//! Persistent, proof-of-possession client identity for direct multiplayer.
//!
//! The caller chooses the per-user state path. On Unix-like systems the key
//! file is owner-only (0600). On Windows the PKCS#8 key is protected with
//! DPAPI for the current user before it reaches disk. A key is a local
//! pseudonym, not an externally verified identity; deleting it creates a new
//! identity.

use anyhow::{Context, Result, ensure};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use std::{
    fmt,
    fs::{self, OpenOptions},
    io::Read,
    path::Path,
};

const PLAIN_MAGIC: &[u8; 8] = b"BRIID001";
const WINDOWS_MAGIC: &[u8; 8] = b"BRIDP001";
const MAX_IDENTITY_FILE: u64 = 4096;

/// A local private Ed25519 key. Debug output never includes key material.
pub struct ClientIdentity {
    pkcs8: Vec<u8>,
    public_key: [u8; 32],
}

impl fmt::Debug for ClientIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientIdentity")
            .field("public_key", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl ClientIdentity {
    /// Load an existing identity or create it atomically at the caller's path.
    /// Existing symlinks, non-files, oversized files, and malformed keys fail closed.
    pub fn load_or_create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let parent = path.parent().context("Identity path has no parent")?;
        fs::create_dir_all(parent).context("Create identity directory")?;

        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                ensure!(
                    metadata.file_type().is_file(),
                    "Identity path is not a regular file"
                );
                ensure!(
                    metadata.len() <= MAX_IDENTITY_FILE,
                    "Identity file exceeds limit"
                );
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    ensure!(
                        metadata.permissions().mode() & 0o077 == 0,
                        "Identity file permissions are not private"
                    );
                }
                let file = OpenOptions::new().read(true).open(path)?;
                let mut stored = Vec::with_capacity(metadata.len() as usize);
                file.take(MAX_IDENTITY_FILE + 1).read_to_end(&mut stored)?;
                ensure!(
                    stored.len() as u64 <= MAX_IDENTITY_FILE,
                    "Identity file exceeds limit"
                );
                Self::decode_stored(&stored)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let rng = SystemRandom::new();
                let generated = Ed25519KeyPair::generate_pkcs8(&rng)
                    .map_err(|_| anyhow::anyhow!("Operating system key generation failed"))?;
                let identity = Self::from_pkcs8(generated.as_ref().to_vec())?;
                let stored = Self::encode_stored(identity.pkcs8())?;
                match bri_files::create_new_private(path, &stored) {
                    Ok(()) => Ok(identity),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        Self::load_or_create(path)
                    }
                    Err(error) => Err(error).context("Persist new client identity"),
                }
            }
            Err(error) => Err(error).context("Inspect client identity"),
        }
    }

    /// Stable public key used to derive the server-visible pseudonym.
    pub fn public_key(&self) -> &[u8; 32] {
        &self.public_key
    }

    /// Sign the caller's domain-separated handshake transcript.
    pub fn sign(&self, transcript: &[u8]) -> Result<[u8; 64]> {
        let key = Ed25519KeyPair::from_pkcs8(&self.pkcs8)
            .map_err(|_| anyhow::anyhow!("Stored client identity is invalid"))?;
        let signature = key.sign(transcript);
        let bytes: [u8; 64] = signature
            .as_ref()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Unexpected Ed25519 signature length"))?;
        Ok(bytes)
    }

    fn pkcs8(&self) -> &[u8] {
        &self.pkcs8
    }

    fn from_pkcs8(pkcs8: Vec<u8>) -> Result<Self> {
        ensure!(
            !pkcs8.is_empty() && pkcs8.len() <= 1024,
            "Invalid identity key length"
        );
        let pair = Ed25519KeyPair::from_pkcs8(&pkcs8)
            .map_err(|_| anyhow::anyhow!("Stored client identity is invalid"))?;
        let public_key: [u8; 32] = pair
            .public_key()
            .as_ref()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Unexpected Ed25519 public key length"))?;
        Ok(Self { pkcs8, public_key })
    }

    fn decode_stored(stored: &[u8]) -> Result<Self> {
        ensure!(stored.len() >= 9, "Identity file is truncated");
        let pkcs8 = if stored.starts_with(PLAIN_MAGIC) {
            #[cfg(unix)]
            {
                stored[PLAIN_MAGIC.len()..].to_vec()
            }
            #[cfg(not(unix))]
            {
                anyhow::bail!("Unprotected identity file is not supported on this platform")
            }
        } else if stored.starts_with(WINDOWS_MAGIC) {
            #[cfg(windows)]
            {
                dpapi_unprotect(&stored[WINDOWS_MAGIC.len()..])?
            }
            #[cfg(not(windows))]
            {
                anyhow::bail!("Windows-protected identity cannot be opened on this platform")
            }
        } else {
            anyhow::bail!("Unknown identity file format")
        };
        Self::from_pkcs8(pkcs8)
    }

    fn encode_stored(pkcs8: &[u8]) -> Result<Vec<u8>> {
        #[cfg(unix)]
        {
            let mut stored = PLAIN_MAGIC.to_vec();
            stored.extend_from_slice(pkcs8);
            Ok(stored)
        }
        #[cfg(windows)]
        {
            let mut stored = WINDOWS_MAGIC.to_vec();
            stored.extend_from_slice(&dpapi_protect(pkcs8)?);
            ensure!(
                stored.len() as u64 <= MAX_IDENTITY_FILE,
                "Protected identity exceeds limit"
            );
            Ok(stored)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = pkcs8;
            anyhow::bail!("Persistent identity storage is unsupported on this platform")
        }
    }
}

#[cfg(windows)]
fn dpapi_protect(input: &[u8]) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData},
    };
    let source = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(input.len())?,
        pbData: input.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // DPAPI protects the key for the current Windows user. UI is forbidden in headless startup.
    let ok = unsafe {
        CryptProtectData(
            &source,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    ensure!(
        ok != 0 && !output.pbData.is_null(),
        "Windows user data protection failed"
    );
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe { LocalFree(output.pbData.cast()) };
    Ok(bytes)
}

#[cfg(windows)]
fn dpapi_unprotect(input: &[u8]) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
        },
    };
    ensure!(!input.is_empty(), "Protected identity payload is empty");
    let source = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(input.len())?,
        pbData: input.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptUnprotectData(
            &source,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    ensure!(
        ok != 0 && !output.pbData.is_null(),
        "Windows user data unprotection failed"
    );
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe { LocalFree(output.pbData.cast()) };
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_round_trips_and_only_public_key_is_exposed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("identity.key");
        let first = ClientIdentity::load_or_create(&path).unwrap();
        let second = ClientIdentity::load_or_create(&path).unwrap();
        assert_eq!(first.public_key(), second.public_key());
        assert_ne!(first.sign(b"transcript").unwrap(), [0; 64]);
        assert!(!format!("{first:?}").contains("PRIVATE"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn malformed_and_symlink_identity_files_fail_closed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("identity.key");
        fs::write(&path, b"not an identity").unwrap();
        assert!(ClientIdentity::load_or_create(&path).is_err());
        #[cfg(unix)]
        {
            let target = directory.path().join("target");
            fs::write(&target, b"x").unwrap();
            let link = directory.path().join("link");
            std::os::unix::fs::symlink(&target, &link).unwrap();
            assert!(ClientIdentity::load_or_create(&link).is_err());
        }
    }
}
