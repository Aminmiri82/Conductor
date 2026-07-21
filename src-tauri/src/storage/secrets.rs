use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use aes_gcm::{
    aead::{Aead, KeyInit, Nonce},
    Aes256Gcm, Key,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rand::RngCore;
use thiserror::Error;

const KEYRING_SERVICE: &str = "com.yaramiri.conductor";
const KEYRING_ACCOUNT: &str = "db-master-key";
const KEY_FILE_NAME: &str = ".master.key";
const CIPHERTEXT_PREFIX: &str = "enc:v1:";
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

#[derive(Debug, Error)]
pub enum SecretsError {
    #[error("failed to access secrets storage: {0}")]
    Backend(String),
    #[error("failed to encrypt value")]
    Encrypt,
    #[error("failed to decrypt value")]
    Decrypt,
    #[error("invalid encoded ciphertext")]
    InvalidCiphertext,
    #[error("filesystem error at {path}: {source}")]
    FileOperation {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Manages the master key used to encrypt sensitive data at rest.
///
/// The key is stored in the OS keyring (preferred) with a fallback to a
/// permissions-restricted file inside the app data directory.
#[derive(Clone)]
pub struct Secrets {
    inner: Arc<SecretsInner>,
}

struct SecretsInner {
    cipher: Aes256Gcm,
}

impl Secrets {
    /// Initialize the secrets service, resolving or creating a master key.
    pub fn init(app_data_dir: impl AsRef<Path>) -> Result<Self, SecretsError> {
        let key_bytes = resolve_master_key(app_data_dir.as_ref())?;
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        Ok(Self {
            inner: Arc::new(SecretsInner { cipher }),
        })
    }

    /// Encrypt a plaintext value and return the tagged ciphertext string.
    pub fn encrypt(&self, plaintext: &str) -> Result<String, SecretsError> {
        if is_encrypted(plaintext) {
            return Ok(plaintext.to_string());
        }
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::<Aes256Gcm>::from_slice(&nonce_bytes);
        let ciphertext = self
            .inner
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| SecretsError::Encrypt)?;
        let mut combined = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ciphertext);
        Ok(format!("{CIPHERTEXT_PREFIX}{}", B64.encode(&combined)))
    }

    /// Decrypt an `enc:v1:...` value. Non-encrypted inputs are returned as-is.
    pub fn decrypt(&self, value: &str) -> Result<String, SecretsError> {
        let Some(body) = value.strip_prefix(CIPHERTEXT_PREFIX) else {
            return Ok(value.to_string());
        };
        let raw = B64
            .decode(body.as_bytes())
            .map_err(|_| SecretsError::InvalidCiphertext)?;
        if raw.len() <= NONCE_LEN {
            return Err(SecretsError::InvalidCiphertext);
        }
        let (nonce_bytes, ciphertext) = raw.split_at(NONCE_LEN);
        let nonce = Nonce::<Aes256Gcm>::from_slice(nonce_bytes);
        let plaintext = self
            .inner
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| SecretsError::Decrypt)?;
        String::from_utf8(plaintext).map_err(|_| SecretsError::Decrypt)
    }

    /// Whether the input looks like an already-encrypted payload.
    pub fn is_encrypted(value: &str) -> bool {
        is_encrypted(value)
    }
}

pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(CIPHERTEXT_PREFIX)
}

fn resolve_master_key(app_data_dir: &Path) -> Result<[u8; KEY_LEN], SecretsError> {
    // Try the OS keyring first.
    match load_from_keyring() {
        Ok(Some(bytes)) => return Ok(bytes),
        Ok(None) => {
            let generated = generate_key();
            if store_in_keyring(&generated).is_ok() {
                return Ok(generated);
            }
            // Fall through to file-based storage if we cannot persist to keyring.
        }
        Err(_) => {
            // Keyring unavailable — fall back to file-based storage.
        }
    }

    load_or_create_key_file(app_data_dir)
}

fn load_from_keyring() -> Result<Option<[u8; KEY_LEN]>, SecretsError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|error| SecretsError::Backend(error.to_string()))?;
    match entry.get_password() {
        Ok(value) => {
            let bytes = B64
                .decode(value.as_bytes())
                .map_err(|_| SecretsError::InvalidCiphertext)?;
            if bytes.len() != KEY_LEN {
                return Err(SecretsError::InvalidCiphertext);
            }
            let mut out = [0u8; KEY_LEN];
            out.copy_from_slice(&bytes);
            Ok(Some(out))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(SecretsError::Backend(error.to_string())),
    }
}

fn store_in_keyring(key: &[u8; KEY_LEN]) -> Result<(), SecretsError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|error| SecretsError::Backend(error.to_string()))?;
    entry
        .set_password(&B64.encode(key))
        .map_err(|error| SecretsError::Backend(error.to_string()))
}

fn load_or_create_key_file(app_data_dir: &Path) -> Result<[u8; KEY_LEN], SecretsError> {
    fs::create_dir_all(app_data_dir).map_err(|source| SecretsError::FileOperation {
        path: app_data_dir.to_path_buf(),
        source,
    })?;
    let path = app_data_dir.join(KEY_FILE_NAME);
    if let Ok(contents) = fs::read_to_string(&path) {
        let bytes = B64
            .decode(contents.trim().as_bytes())
            .map_err(|_| SecretsError::InvalidCiphertext)?;
        if bytes.len() != KEY_LEN {
            return Err(SecretsError::InvalidCiphertext);
        }
        let mut out = [0u8; KEY_LEN];
        out.copy_from_slice(&bytes);
        return Ok(out);
    }

    let generated = generate_key();
    write_key_file(&path, &generated)?;
    Ok(generated)
}

fn generate_key() -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

fn write_key_file(path: &Path, key: &[u8; KEY_LEN]) -> Result<(), SecretsError> {
    fs::write(path, B64.encode(key)).map_err(|source| SecretsError::FileOperation {
        path: path.to_path_buf(),
        source,
    })?;
    restrict_file_permissions(path)?;
    Ok(())
}

#[cfg(unix)]
fn restrict_file_permissions(path: &Path) -> Result<(), SecretsError> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|source| SecretsError::FileOperation {
            path: path.to_path_buf(),
            source,
        })?
        .permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms).map_err(|source| SecretsError::FileOperation {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(not(unix))]
fn restrict_file_permissions(_path: &Path) -> Result<(), SecretsError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_secrets() -> Secrets {
        let key = [0x42u8; KEY_LEN];
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        Secrets {
            inner: Arc::new(SecretsInner { cipher }),
        }
    }

    #[test]
    fn round_trip_encrypt_decrypt() {
        let secrets = test_secrets();
        let ciphertext = secrets.encrypt("hello world").unwrap();
        assert!(ciphertext.starts_with(CIPHERTEXT_PREFIX));
        assert_eq!(secrets.decrypt(&ciphertext).unwrap(), "hello world");
    }

    #[test]
    fn encrypt_is_idempotent_on_ciphertext() {
        let secrets = test_secrets();
        let first = secrets.encrypt("secret").unwrap();
        let second = secrets.encrypt(&first).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn decrypt_passthrough_for_plaintext() {
        let secrets = test_secrets();
        assert_eq!(secrets.decrypt("plain-text").unwrap(), "plain-text");
    }
}
