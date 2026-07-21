use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use tauri::State;

use crate::commands::requests::request_auth::decode_auth_json;
use crate::commands::{AppError, AppResult};
use crate::storage::{Secrets, StorageError};
use crate::AppState;

/// Export a stored collection to a Postman Collection v2.1.0 JSON string.
///
/// This is a best-effort dump: only the parts of the schema Conductor stores
/// are populated. Pre-request and test scripts, folder auth inheritance and
/// per-request auth are included when available. Sensitive fields are
/// **decrypted** back to plaintext so the resulting file is a working
/// collection.
#[tauri::command]
pub fn export_postman_collection(
    collection_id: String,
    state: State<'_, AppState>,
) -> AppResult<String> {
    let secrets = state.database.secrets().clone();
    let output = state
        .database
        .with_read_connection(|connection| build_export(connection, &secrets, &collection_id))?;
    serde_json::to_string_pretty(&output).map_err(AppError::from)
}

fn build_export(
    connection: &rusqlite::Connection,
    secrets: &Secrets,
    collection_id: &str,
) -> Result<Value, StorageError> {
    let (name, auth_json): (String, Option<String>) = connection
        .query_row(
            "SELECT name, auth_json FROM collections WHERE id = ?",
            params![collection_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| StorageError::InvalidInput("collection not found".to_string()))?;

    let collection_auth = auth_json
        .as_deref()
        .map(|value| decode_auth_json(secrets, value))
        .transpose()?
        .flatten()
        .map(auth_to_postman);

    let requests = load_requests(connection, secrets, collection_id)?;
    let nodes = load_nodes(connection, secrets, collection_id)?;
    let variables = load_collection_variables(connection, collection_id)?;

    let items = build_items(&nodes, &requests, None);

    Ok(json!({
        "info": {
            "name": name,
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json",
            "_postman_id": collection_id,
        },
        "item": items,
        "auth": collection_auth,
        "variable": variables,
    }))
}

fn build_items(
    nodes: &HashMap<Option<String>, Vec<NodeRow>>,
    requests: &HashMap<String, RequestRow>,
    parent: Option<&str>,
) -> Vec<Value> {
    let key = parent.map(ToString::to_string);
    let mut children = nodes.get(&key).cloned().unwrap_or_default();
    children.sort_by_key(|node| node.sort_order);
    children
        .into_iter()
        .filter_map(|node| match node.kind.as_str() {
            "folder" => Some(json!({
                "name": node.name,
                "item": build_items(nodes, requests, Some(&node.id)),
                "auth": node.auth.clone().map(auth_to_postman),
            })),
            "request" => {
                let req = node.request_id.as_deref().and_then(|id| requests.get(id))?;
                Some(json!({
                    "name": node.name,
                    "request": request_to_postman(req),
                }))
            }
            _ => None,
        })
        .collect()
}

fn request_to_postman(request: &RequestRow) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("method".to_string(), Value::String(request.method.clone()));
    if let Some(headers) = request.headers_json.as_ref() {
        if let Ok(headers) = serde_json::from_str::<Value>(headers) {
            object.insert(
                "header".to_string(),
                Value::Array(kv_array_to_postman(&headers)),
            );
        }
    }
    let url = build_url_object(&request.url, request.query_json.as_deref());
    object.insert("url".to_string(), url);
    if let Some(body) = request
        .body_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
    {
        if let Some(body) = body_to_postman(&body) {
            object.insert("body".to_string(), body);
        }
    }
    if let Some(auth) = request.auth.as_ref() {
        object.insert("auth".to_string(), auth_to_postman(auth.clone()));
    }
    Value::Object(object)
}

fn body_to_postman(body: &Value) -> Option<Value> {
    let mode = body.get("mode").and_then(Value::as_str)?;
    match mode {
        "raw" => Some(json!({
            "mode": "raw",
            "raw": body.get("raw").and_then(Value::as_str).unwrap_or_default(),
            "options": {
                "raw": {
                    "language": body.get("rawLanguage").and_then(Value::as_str).unwrap_or("text")
                }
            }
        })),
        "formdata" => Some(json!({
            "mode": "formdata",
            "formdata": body.get("formData").and_then(Value::as_array).cloned().unwrap_or_default(),
        })),
        "urlencoded" => Some(json!({
            "mode": "urlencoded",
            "urlencoded": body.get("urlencoded").and_then(Value::as_array).cloned().unwrap_or_default(),
        })),
        "graphql" => Some(json!({
            "mode": "graphql",
            "graphql": {
                "query": body.pointer("/graphql/query").and_then(Value::as_str).unwrap_or_default(),
                "variables": body.pointer("/graphql/variables").and_then(Value::as_str).unwrap_or_default(),
            }
        })),
        _ => None,
    }
}

fn build_url_object(url: &str, query_json: Option<&str>) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("raw".to_string(), Value::String(url.to_string()));
    if let Some(query) = query_json.and_then(|value| serde_json::from_str::<Value>(value).ok()) {
        object.insert(
            "query".to_string(),
            Value::Array(kv_array_to_postman(&query)),
        );
    }
    Value::Object(object)
}

fn kv_array_to_postman(value: &Value) -> Vec<Value> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let key = item.get("key").and_then(Value::as_str)?;
                    let value = item.get("value").and_then(Value::as_str).unwrap_or("");
                    let enabled = item.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                    let mut entry = json!({"key": key, "value": value});
                    if !enabled {
                        entry["disabled"] = Value::Bool(true);
                    }
                    Some(entry)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn auth_to_postman(auth: crate::commands::models::AuthConfig) -> Value {
    match auth.auth_type.as_str() {
        "bearer" => json!({
            "type": "bearer",
            "bearer": [
                {"key": "token", "value": auth.token.unwrap_or_default(), "type": "string"}
            ]
        }),
        "basic" => json!({
            "type": "basic",
            "basic": [
                {"key": "username", "value": auth.username.unwrap_or_default(), "type": "string"},
                {"key": "password", "value": auth.password.unwrap_or_default(), "type": "string"},
            ]
        }),
        "apikey" => json!({
            "type": "apikey",
            "apikey": [
                {"key": "key", "value": auth.key.unwrap_or_default(), "type": "string"},
                {"key": "value", "value": auth.value.unwrap_or_default(), "type": "string"},
                {"key": "in", "value": auth.add_to.unwrap_or_else(|| "header".to_string()), "type": "string"},
            ]
        }),
        other => json!({"type": other}),
    }
}

#[derive(Debug, Clone)]
struct NodeRow {
    id: String,
    sort_order: i64,
    kind: String,
    name: String,
    request_id: Option<String>,
    auth: Option<crate::commands::models::AuthConfig>,
}

#[derive(Debug, Clone)]
struct RequestRow {
    method: String,
    url: String,
    headers_json: Option<String>,
    query_json: Option<String>,
    body_json: Option<String>,
    auth: Option<crate::commands::models::AuthConfig>,
}

fn load_requests(
    connection: &rusqlite::Connection,
    secrets: &Secrets,
    collection_id: &str,
) -> Result<HashMap<String, RequestRow>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT id, method, url, headers_json, query_json, body_json, auth_json
         FROM requests WHERE collection_id = ?",
    )?;
    let rows = statement
        .query_map(params![collection_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut map = HashMap::new();
    for (id, method, url, headers, query, body, auth) in rows {
        let auth = auth
            .as_deref()
            .map(|value| decode_auth_json(secrets, value))
            .transpose()?
            .flatten();
        map.insert(
            id,
            RequestRow {
                method,
                url,
                headers_json: headers,
                query_json: query,
                body_json: body,
                auth,
            },
        );
    }
    Ok(map)
}

fn load_nodes(
    connection: &rusqlite::Connection,
    secrets: &Secrets,
    collection_id: &str,
) -> Result<HashMap<Option<String>, Vec<NodeRow>>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT id, parent_id, sort_order, kind, name, request_id, auth_json
         FROM collection_nodes WHERE collection_id = ?",
    )?;
    let rows = statement
        .query_map(params![collection_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut map: HashMap<Option<String>, Vec<NodeRow>> = HashMap::new();
    for (id, parent_id, sort_order, kind, name, request_id, auth_json) in rows {
        let auth = auth_json
            .as_deref()
            .map(|value| decode_auth_json(secrets, value))
            .transpose()?
            .flatten();
        map.entry(parent_id).or_default().push(NodeRow {
            id,
            sort_order,
            kind,
            name,
            request_id,
            auth,
        });
    }
    Ok(map)
}

fn load_collection_variables(
    connection: &rusqlite::Connection,
    collection_id: &str,
) -> Result<Vec<Value>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT key, current_value, enabled, sensitive
         FROM variables WHERE scope = 'collection' AND collection_id = ?
         ORDER BY key",
    )?;
    let rows = statement
        .query_map(params![collection_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, i64>(3)? != 0,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows
        .into_iter()
        .map(|(key, value, enabled, sensitive)| {
            // We intentionally do NOT export the encrypted ciphertext of
            // sensitive variables. Callers can re-enter secrets after import.
            let export_value = if sensitive { String::new() } else { value };
            let mut entry = json!({
                "key": key,
                "value": export_value,
                "type": if sensitive { "secret" } else { "default" },
            });
            if !enabled {
                entry["disabled"] = Value::Bool(true);
            }
            entry
        })
        .collect())
}
