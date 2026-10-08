mod auth;
mod body;
mod postman_scripts;
mod resolver;
mod response_files;
mod sender;
mod store;

pub use response_files::{save_response_body, save_text_file, BinaryBodies};
pub use sender::{resolve_request, send_request};
pub use store::{create_request, duplicate_request, get_request, save_request};

/// Stored JSON columns are read leniently: one that no longer parses reads as
/// absent instead of making the request unopenable.
fn parse_json_optional<T: serde::de::DeserializeOwned>(value: &str) -> Option<T> {
    serde_json::from_str(value).ok()
}
