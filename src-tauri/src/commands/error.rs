use serde::{Serialize, Serializer};
use specta::{datatype::DataType, Type, Types};
use thiserror::Error;

use crate::storage::StorageError;

/// What every command rejects with. It crosses IPC as its message string, so
/// the frontend sees exactly what a plain `Result<_, String>` would give it.
#[derive(Debug, Error)]
pub enum CommandError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("{}", with_causes(.0))]
    Http(reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    InvalidHeaderName(#[from] reqwest::header::InvalidHeaderName),
    #[error(transparent)]
    InvalidHeaderValue(#[from] reqwest::header::InvalidHeaderValue),
    #[error("{0}")]
    Message(String),
}

impl CommandError {
    /// For foreign errors this enum has no variant for (`http` and `url`
    /// parse errors are not nameable without extra dependencies).
    pub fn msg(error: impl std::fmt::Display) -> Self {
        Self::Message(error.to_string())
    }
}

// The URL can hold a resolved secret (`?token={{token}}`), and these messages
// reach the error banner, so it is dropped. The user knows what they sent.
impl From<reqwest::Error> for CommandError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error.without_url())
    }
}

// reqwest's own message is only the top level ("error sending request"); the
// reason the user needs (connection refused, DNS, TLS) is in the source chain.
fn with_causes(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        let cause_message = cause.to_string();
        // Wrappers often repeat their inner error's message verbatim.
        if !message.ends_with(&cause_message) {
            message.push_str(": ");
            message.push_str(&cause_message);
        }
        source = cause.source();
    }
    message
}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<&str> for CommandError {
    fn from(message: &str) -> Self {
        Self::Message(message.to_string())
    }
}

impl Serialize for CommandError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

// `ErrorHandlingMode::Throw` leaves the error out of the generated
// TypeScript, but specta still needs a type to describe it: a string.
impl Type for CommandError {
    fn definition(types: &mut Types) -> DataType {
        String::definition(types)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_reach_the_frontend_as_their_message_string() {
        let error = CommandError::from(StorageError::InvalidTreeMove);

        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#""cannot move a folder into itself or one of its descendants""#
        );
    }

    #[test]
    fn send_failures_name_their_cause_but_not_the_url() {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let url = format!("http://127.0.0.1:{port}/items?token=secret");
        let error = tauri::async_runtime::block_on(async {
            let client = crate::build_http_client().unwrap();
            client.get(url).send().await.unwrap_err()
        });

        let message = CommandError::from(error).to_string();

        assert!(message.starts_with("error sending request: "), "{message}");
        assert!(message.contains("Connection refused"), "{message}");
        assert!(!message.contains("secret"), "{message}");
    }
}
