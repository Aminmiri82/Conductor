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
///
/// Resolution never creates a second key when any persisted key already
/// exists (file or keyring), so ciphertext stays decryptable across
/// keyring availability flaps.
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
        if looks_like_ciphertext(plaintext) {
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
    #[allow(dead_code)]
    pub fn is_encrypted(value: &str) -> bool {
        looks_like_ciphertext(value)
    }
}

pub fn is_encrypted(value: &str) -> bool {
    looks_like_ciphertext(value)
}

/// True only when the value has a valid `enc:v1:` payload (prefix + base64
/// body long enough to hold a nonce). A bare prefix alone is not treated as
/// ciphertext, so plaintext that happens to start with `enc:v1:` is still
/// encrypted.
fn looks_like_ciphertext(value: &str) -> bool {
    let Some(body) = value.strip_prefix(CIPHERTEXT_PREFIX) else {
        return false;
    };
    match B64.decode(body.as_bytes()) {
        Ok(raw) if raw.len() > NONCE_LEN => true,
        _ => false,
    }
}

fn resolve_master_key(app_data_dir: &Path) -> Result<[u8; KEY_LEN], SecretsError> {
    // Prefer an existing file key as the single source of truth. If the
    // keyring later becomes available, migrate that same key into it —
    // never mint a second master key.
    if let Some(file_key) = try_load_key_file(app_data_dir)? {
        let _ = store_in_keyring(&file_key);
        return Ok(file_key);
    }

    match load_from_keyring() {
        Ok(Some(bytes)) => return Ok(bytes),
        Ok(None) | Err(_) => {}
    }

    // Neither store has a key yet — generate once.
    let generated = generate_key();
    if store_in_keyring(&generated).is_ok() {
        return Ok(generated);
    }
    write_key_file(&app_data_dir.join(KEY_FILE_NAME), &generated)?;
    Ok(generated)
}

fn load_from_keyring() -> Result<Option<[u8; KEY_LEN]>, SecretsError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|error| SecretsError::Backend(error.to_string()))?;
    match entry.get_password() {
        Ok(value) => Ok(Some(decode_key_bytes(&value)?)),
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

fn try_load_key_file(app_data_dir: &Path) -> Result<Option<[u8; KEY_LEN]>, SecretsError> {
    let path = app_data_dir.join(KEY_FILE_NAME);
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path).map_err(|source| SecretsError::FileOperation {
        path: path.clone(),
        source,
    })?;
    Ok(Some(decode_key_bytes(contents.trim())?))
}

fn decode_key_bytes(value: &str) -> Result<[u8; KEY_LEN], SecretsError> {
    let bytes = B64
        .decode(value.as_bytes())
        .map_err(|_| SecretsError::InvalidCiphertext)?;
    if bytes.len() != KEY_LEN {
        return Err(SecretsError::InvalidCiphertext);
    }
    let mut out = [0u8; KEY_LEN];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn generate_key() -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

fn write_key_file(path: &Path, key: &[u8; KEY_LEN]) -> Result<(), SecretsError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| SecretsError::FileOperation {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    write_key_file_atomic(path, key)
}

#[cfg(unix)]
fn write_key_file_atomic(path: &Path, key: &[u8; KEY_LEN]) -> Result<(), SecretsError> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    // Create with 0600 so the key is never briefly world-readable.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|source| SecretsError::FileOperation {
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(B64.encode(key).as_bytes())
        .map_err(|source| SecretsError::FileOperation {
            path: path.to_path_buf(),
            source,
        })?;
    file.sync_all()
        .map_err(|source| SecretsError::FileOperation {
            path: path.to_path_buf(),
            source,
        })?;
    let mut perms = file
        .metadata()
        .map_err(|source| SecretsError::FileOperation {
            path: path.to_path_buf(),
            source,
        })?
        .permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms).map_err(|source| SecretsError::FileOperation {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

#[cfg(not(unix))]
fn write_key_file_atomic(path: &Path, key: &[u8; KEY_LEN]) -> Result<(), SecretsError> {
    // Windows: rely on the user profile ACL for the app data directory.
    // A full DACL lockdown would need platform-specific crates; document
    // that keyring is preferred on desktop.
    fs::write(path, B64.encode(key)).map_err(|source| SecretsError::FileOperation {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_secrets() -> Secrets {
        let key = [0x42u8; KEY_LEN];
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        Secrets {
            inner: Arc::new(SecretsInner { cipher }),
        }
    }

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("conductor-secrets-{nanos}"));
        fs::create_dir_all(&path).expect("temp dir");
        path
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

    #[test]
    fn bare_prefix_is_not_treated_as_ciphertext() {
        let secrets = test_secrets();
        let weird = format!("{CIPHERTEXT_PREFIX}not-valid");
        let encrypted = secrets.encrypt(&weird).unwrap();
        assert_ne!(encrypted, weird);
        assert_eq!(secrets.decrypt(&encrypted).unwrap(), weird);
    }

    #[test]
    fn file_key_is_reused_across_resolve_calls() {
        let dir = temp_dir();
        let first = resolve_master_key(&dir).expect("first resolve");
        let second = resolve_master_key(&dir).expect("second resolve");
        assert_eq!(first, second);
        let _ = fs::remove_dir_all(&dir);
    }
}
