use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::storage::{SecretsError, StorageError};

/// Typed error returned from Tauri command handlers.
///
/// Serializes as a plain string so the JavaScript side keeps the same shape it
/// had when commands returned `Result<T, String>`.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Storage(#[from] StorageError),
    #[error("{0}")]
    Secrets(#[from] SecretsError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("request cancelled")]
    Cancelled,
    #[error("{0}")]
    Message(String),
}

impl AppError {
    pub fn msg(message: impl Into<String>) -> Self {
        AppError::Message(message.into())
    }
}

impl From<String> for AppError {
    fn from(value: String) -> Self {
        AppError::Message(value)
    }
}

impl From<&str> for AppError {
    fn from(value: &str) -> Self {
        AppError::Message(value.to_string())
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = std::result::Result<T, AppError>;
