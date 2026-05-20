use std::collections::HashMap;

use reqwest::header::{HeaderName, HeaderValue};
use rusqlite::{params, OptionalExtension};

use crate::commands::models::{AuthConfig, RequestDetail};
use crate::storage::StorageError;

use super::variable_resolver::resolve_text;

pub(super) fn inherited_auth_for_request(
    connection: &rusqlite::Connection,
    request: &RequestDetail,
) -> Result<Option<AuthConfig>, StorageError> {
    let mut parent_id: Option<String> = connection
        .query_row(
            "SELECT parent_id FROM collection_nodes WHERE request_id = ?",
            params![request.id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();

    while let Some(id) = parent_id {
        let row: Option<(Option<String>, Option<String>)> = connection
            .query_row(
                "SELECT auth_json, parent_id FROM collection_nodes WHERE id = ?",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((auth_json, next_parent_id)) = row else {
            break;
        };
        if let Some(auth) = auth_json.as_deref().and_then(parse_json_optional) {
            return Ok(Some(auth));
        }
        parent_id = next_parent_id;
    }

    let collection_auth: Option<Option<String>> = connection
        .query_row(
            "SELECT auth_json FROM collections WHERE id = ?",
            params![request.collection_id],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(auth_json) = collection_auth {
        if let Some(auth) = auth_json.as_deref().and_then(parse_json_optional) {
            return Ok(Some(auth));
        }
    }

    Ok(None)
}
pub(super) fn effective_auth(
    request_auth: Option<&AuthConfig>,
    inherited_auth: Option<&AuthConfig>,
) -> Option<AuthConfig> {
    match request_auth {
        Some(auth) if auth.auth_type == "noauth" => Some(auth.clone()),
        Some(auth) => Some(auth.clone()),
        None => inherited_auth.cloned(),
    }
}
pub(super) fn apply_auth(
    mut builder: reqwest::RequestBuilder,
    auth: Option<&AuthConfig>,
    variables: &HashMap<String, String>,
) -> Result<reqwest::RequestBuilder, String> {
    let Some(auth) = auth else {
        return Ok(builder);
    };

    match auth.auth_type.as_str() {
        "bearer" => {
            if let Some(token) = auth.token.as_ref() {
                builder = builder.bearer_auth(resolve_text(token, variables).0);
            }
        }
        "basic" => {
            let username = auth
                .username
                .as_deref()
                .map(|value| resolve_text(value, variables).0)
                .unwrap_or_default();
            let password = auth
                .password
                .as_deref()
                .map(|value| resolve_text(value, variables).0);
            builder = builder.basic_auth(username, password);
        }
        "apikey" => {
            let key = auth
                .key
                .clone()
                .unwrap_or_else(|| "Authorization".to_string());
            let value = auth
                .value
                .as_deref()
                .map(|value| resolve_text(value, variables).0)
                .unwrap_or_default();
            if auth.add_to.as_deref() == Some("query") {
                builder = builder.query(&[(key, value)]);
            } else if !key.is_empty() {
                let name = HeaderName::from_bytes(key.as_bytes()).map_err(|e| e.to_string())?;
                let value = HeaderValue::from_str(&value).map_err(|e| e.to_string())?;
                builder = builder.header(name, value);
            }
        }
        _ => {}
    }

    Ok(builder)
}
fn parse_json_optional<T: serde::de::DeserializeOwned>(value: &str) -> Option<T> {
    serde_json::from_str(value).ok()
}
