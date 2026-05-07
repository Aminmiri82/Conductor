use std::{collections::HashMap, fs, time::Instant};

use chrono::Utc;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Method,
};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use tauri::State;
use uuid::Uuid;

use crate::{storage::StorageError, AppState};

use super::models::{
    BodyField, CreateFolderInput, CreateRequestInput, DuplicateRequestInput, KeyValue,
    MoveNodeInput, RequestBody, RequestDetail, ResolvedRequestPreview, ResponseHeader,
    SaveTextFileInput, SendRequestInput, SendRequestResult, UnresolvedVariable, VariableEntry,
};

#[tauri::command]
pub fn get_request(
    request_id: String,
    state: State<'_, AppState>,
) -> Result<RequestDetail, String> {
    state
        .database
        .with_connection(|connection| {
            let mut request: RequestDetail = connection.query_row(
                "SELECT r.id, r.collection_id, n.name, r.method, r.url, r.headers_json, r.query_json,
                        r.path_params_json, r.auth_json, r.body_json, r.pre_request_script_json,
                        r.test_script_json, r.updated_at
                 FROM requests r
                 JOIN collection_nodes n ON n.request_id = r.id
                 WHERE r.id = ?",
                params![request_id],
                |row| {
                    let headers_json: String = row.get(5)?;
                    let query_json: String = row.get(6)?;
                    let path_params_json: String = row.get(7)?;
                    let auth_json: Option<String> = row.get(8)?;
                    let body_json: Option<String> = row.get(9)?;
                    let pre_request_script_json: Option<String> = row.get(10)?;
                    let test_script_json: Option<String> = row.get(11)?;

                    Ok(RequestDetail {
                        id: row.get(0)?,
                        collection_id: row.get(1)?,
                        name: row.get(2)?,
                        method: row.get(3)?,
                        url: row.get(4)?,
                        headers: parse_json(&headers_json),
                        query: parse_json(&query_json),
                        path_params: parse_json(&path_params_json),
                        auth: auth_json.as_deref().and_then(parse_json_optional),
                        inherited_auth: None,
                        effective_auth: None,
                        body: body_json.as_deref().and_then(parse_json_optional),
                        pre_request_script: pre_request_script_json
                            .as_deref()
                            .and_then(parse_json_optional),
                        test_script: test_script_json.as_deref().and_then(parse_json_optional),
                        updated_at: row.get(12)?,
                    })
                },
            )?;
            request.inherited_auth = inherited_auth_for_request(&connection, &request)?;
            request.effective_auth = effective_auth(request.auth.as_ref(), request.inherited_auth.as_ref());
            normalize_body_files(&mut request);
            Ok(request)
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_request(
    input: CreateRequestInput,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let now = Utc::now().to_rfc3339();
    let request_id = Uuid::new_v4().to_string();
    let node_id = Uuid::new_v4().to_string();

    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            shift_node_positions(
                &tx,
                &input.collection_id,
                input.parent_id.as_deref(),
                input.position,
            )?;
            tx.execute(
                "INSERT INTO requests
                 (id, collection_id, method, url, headers_json, query_json, path_params_json,
                  auth_json, body_json, raw_postman_item_json, pre_request_script_json,
                  test_script_json, created_at, updated_at)
                 VALUES (?, ?, 'GET', '', '[]', '[]', '[]', NULL, NULL, NULL, NULL, NULL, ?, ?)",
                params![request_id, input.collection_id, now, now],
            )?;
            tx.execute(
                "INSERT INTO collection_nodes
                 (id, collection_id, parent_id, position, kind, name, request_id, variables_json,
                  auth_json, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'request', ?, ?, NULL, NULL, ?, ?)",
                params![
                    node_id,
                    input.collection_id,
                    input.parent_id,
                    input.position,
                    input.name,
                    request_id,
                    now,
                    now
                ],
            )?;
            tx.commit()?;
            Ok(request_id.clone())
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_folder(
    input: CreateFolderInput,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let now = Utc::now().to_rfc3339();
    let node_id = Uuid::new_v4().to_string();

    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            shift_node_positions(
                &tx,
                &input.collection_id,
                input.parent_id.as_deref(),
                input.position,
            )?;
            tx.execute(
                "INSERT INTO collection_nodes
                 (id, collection_id, parent_id, position, kind, name, request_id, variables_json,
                  auth_json, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'folder', ?, NULL, NULL, NULL, ?, ?)",
                params![
                    node_id,
                    input.collection_id,
                    input.parent_id,
                    input.position,
                    input.name,
                    now,
                    now
                ],
            )?;
            tx.commit()?;
            Ok(node_id.clone())
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn duplicate_request(
    input: DuplicateRequestInput,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let now = Utc::now().to_rfc3339();
    let new_request_id = Uuid::new_v4().to_string();
    let new_node_id = Uuid::new_v4().to_string();

    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            let source = tx.query_row(
                "SELECT r.collection_id, r.method, r.url, r.headers_json, r.query_json,
                        r.path_params_json, r.auth_json, r.body_json, r.raw_postman_item_json,
                        r.pre_request_script_json, r.test_script_json,
                        n.parent_id, n.position, n.name, n.variables_json, n.auth_json
                 FROM requests r
                 JOIN collection_nodes n ON n.request_id = r.id
                 WHERE r.id = ?",
                params![input.request_id],
                |row| {
                    Ok(DuplicateSource {
                        collection_id: row.get(0)?,
                        method: row.get(1)?,
                        url: row.get(2)?,
                        headers_json: row.get(3)?,
                        query_json: row.get(4)?,
                        path_params_json: row.get(5)?,
                        auth_json: row.get(6)?,
                        body_json: row.get(7)?,
                        raw_postman_item_json: row.get(8)?,
                        pre_request_script_json: row.get(9)?,
                        test_script_json: row.get(10)?,
                        parent_id: row.get(11)?,
                        position: row.get(12)?,
                        name: row.get::<_, String>(13)?,
                        variables_json: row.get(14)?,
                        node_auth_json: row.get(15)?,
                    })
                },
            )?;
            let insert_position = source.position + 1;
            shift_node_positions(
                &tx,
                &source.collection_id,
                source.parent_id.as_deref(),
                insert_position,
            )?;
            tx.execute(
                "INSERT INTO requests
                 (id, collection_id, method, url, headers_json, query_json, path_params_json,
                  auth_json, body_json, raw_postman_item_json, pre_request_script_json,
                  test_script_json, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    new_request_id,
                    source.collection_id,
                    source.method,
                    source.url,
                    source.headers_json,
                    source.query_json,
                    source.path_params_json,
                    source.auth_json,
                    source.body_json,
                    source.raw_postman_item_json,
                    source.pre_request_script_json,
                    source.test_script_json,
                    now,
                    now
                ],
            )?;
            tx.execute(
                "INSERT INTO collection_nodes
                 (id, collection_id, parent_id, position, kind, name, request_id, variables_json,
                  auth_json, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'request', ?, ?, ?, ?, ?, ?)",
                params![
                    new_node_id,
                    source.collection_id,
                    source.parent_id,
                    insert_position,
                    format!("{} Copy", source.name),
                    new_request_id,
                    source.variables_json,
                    source.node_auth_json,
                    now,
                    now
                ],
            )?;
            tx.commit()?;
            Ok(new_request_id.clone())
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_request(request_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .database
        .with_connection(|connection| {
            connection.execute("DELETE FROM requests WHERE id = ?", params![request_id])?;
            Ok(())
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_node(node_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            let request_ids = collect_request_ids_for_node(&tx, &node_id)?;
            tx.execute(
                "DELETE FROM collection_nodes WHERE id = ?",
                params![node_id],
            )?;
            for request_id in request_ids {
                tx.execute("DELETE FROM requests WHERE id = ?", params![request_id])?;
            }
            tx.commit()?;
            Ok(())
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn move_node(input: MoveNodeInput, state: State<'_, AppState>) -> Result<(), String> {
    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            let (collection_id, old_parent_id, old_position, kind): (
                String,
                Option<String>,
                i64,
                String,
            ) = tx.query_row(
                "SELECT collection_id, parent_id, position, kind FROM collection_nodes WHERE id = ?",
                params![input.node_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;

            if kind == "folder" {
                let invalid_target = input.parent_id.as_deref() == Some(input.node_id.as_str())
                    || input
                        .parent_id
                        .as_deref()
                        .map(|parent_id| is_descendant(&tx, parent_id, &input.node_id))
                        .transpose()?
                        .unwrap_or(false);
                if invalid_target {
                    return Err(StorageError::InvalidTreeMove);
                }
            }

            close_position_gap(&tx, &collection_id, old_parent_id.as_deref(), old_position)?;
            open_position_gap(&tx, &collection_id, input.parent_id.as_deref(), input.position)?;
            tx.execute(
                "UPDATE collection_nodes SET parent_id = ?, position = ?, updated_at = ? WHERE id = ?",
                params![input.parent_id, input.position, Utc::now().to_rfc3339(), input.node_id],
            )?;
            tx.commit()?;
            Ok(())
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_text_file(input: SaveTextFileInput) -> Result<(), String> {
    fs::write(input.path, input.contents).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_request(request: RequestDetail, state: State<'_, AppState>) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    state
        .database
        .with_connection(|connection| {
            let mut body = request.body.clone();
            if let Some(body) = &mut body {
                for field in &mut body.form_data {
                    if field.field_type == "file" {
                        field.file_path = None;
                    }
                }
            }

            connection.execute(
                "UPDATE requests
                 SET method = ?, url = ?, headers_json = ?, query_json = ?, path_params_json = ?,
                     auth_json = ?, body_json = ?, updated_at = ?
                 WHERE id = ?",
                params![
                    request.method,
                    request.url,
                    json!(request.headers).to_string(),
                    json!(request.query).to_string(),
                    json!(request.path_params).to_string(),
                    request.auth.map(|auth| json!(auth).to_string()),
                    body.map(|body| json!(body).to_string()),
                    now,
                    request.id
                ],
            )?;
            connection.execute(
                "UPDATE collection_nodes SET name = ?, updated_at = ? WHERE request_id = ?",
                params![request.name, now, request.id],
            )?;
            Ok(())
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_variables(
    scope_kind: String,
    scope_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<VariableEntry>, String> {
    state
        .database
        .with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT scope_kind, scope_id, key, value, enabled, sensitive
                 FROM variables
                 WHERE scope_kind = ? AND scope_id = ?
                 ORDER BY key",
            )?;
            let rows = statement.query_map(params![scope_kind, scope_id], |row| {
                Ok(VariableEntry {
                    scope_kind: row.get(0)?,
                    scope_id: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    enabled: row.get::<_, i64>(4)? != 0,
                    sensitive: row.get::<_, i64>(5)? != 0,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(StorageError::from)
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_variables(
    scope_kind: String,
    scope_id: String,
    variables: Vec<VariableEntry>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            tx.execute(
                "DELETE FROM variables WHERE scope_kind = ? AND scope_id = ?",
                params![scope_kind, scope_id],
            )?;
            for variable in variables
                .into_iter()
                .filter(|variable| !variable.key.is_empty())
            {
                tx.execute(
                    "INSERT INTO variables
                     (scope_kind, scope_id, key, value, enabled, sensitive, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    params![
                        scope_kind,
                        scope_id,
                        variable.key,
                        variable.value,
                        variable.enabled as i64,
                        variable.sensitive as i64,
                        now,
                        now
                    ],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn resolve_request(
    request: RequestDetail,
    state: State<'_, AppState>,
) -> Result<ResolvedRequestPreview, String> {
    state
        .database
        .with_connection(|connection| {
            let variables = load_variable_context(connection, &request)?;
            Ok(resolve_request_with_context(&request, &variables))
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn send_request(
    input: SendRequestInput,
    state: State<'_, AppState>,
) -> Result<SendRequestResult, String> {
    let variables = state
        .database
        .with_connection(|connection| load_variable_context(connection, &input.request))
        .map_err(|error| error.to_string())?;
    let preview = resolve_request_with_context(&input.request, &variables);
    if !preview.unresolved_variables.is_empty() {
        return Err("request has unresolved variables".to_string());
    }

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|error| error.to_string())?;
    let method = Method::from_bytes(input.request.method.as_bytes()).map_err(|e| e.to_string())?;
    let mut builder = client.request(method, preview.url.clone());
    let mut headers = HeaderMap::new();
    let query_pairs = preview
        .query
        .iter()
        .filter(|query| query.enabled && !query.key.is_empty())
        .map(|query| (query.key.clone(), query.value.clone()))
        .collect::<Vec<_>>();
    if !query_pairs.is_empty() {
        builder = builder.query(&query_pairs);
    }

    for header in preview.headers.iter().filter(|header| header.enabled) {
        if header.key.trim().is_empty() {
            continue;
        }
        let name = HeaderName::from_bytes(header.key.as_bytes()).map_err(|e| e.to_string())?;
        let value = HeaderValue::from_str(&header.value).map_err(|e| e.to_string())?;
        headers.insert(name, value);
    }
    builder = builder.headers(headers);

    let auth_to_apply = effective_auth(
        input.request.auth.as_ref(),
        input.request.inherited_auth.as_ref(),
    );
    builder = apply_auth(builder, auth_to_apply.as_ref(), &variables)?;

    if let Some(body) = preview.body.as_ref() {
        builder = apply_body(builder, body).await?;
    }

    let started = Instant::now();
    let response = builder.send().await.map_err(|error| error.to_string())?;
    let duration_ms = started.elapsed().as_millis();
    let status = response.status();
    let headers = response
        .headers()
        .iter()
        .map(|(key, value)| ResponseHeader {
            key: key.to_string(),
            value: value.to_str().unwrap_or_default().to_string(),
        })
        .collect::<Vec<_>>();
    let body_text = response.text().await.map_err(|error| error.to_string())?;
    let body_json = serde_json::from_str::<Value>(&body_text).ok();
    let history_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let updated_variables =
        collect_postman_script_variables(input.request.test_script.as_ref(), body_json.as_ref());

    state
        .database
        .with_connection(|connection| {
            for variable in &updated_variables {
                connection.execute(
                    "INSERT OR REPLACE INTO variables
                     (scope_kind, scope_id, key, value, enabled, sensitive, created_at, updated_at)
                     VALUES ('collection', ?, ?, ?, 1, 0, ?, ?)",
                    params![
                        input.request.collection_id,
                        variable.key,
                        variable.value,
                        now,
                        now
                    ],
                )?;
            }

            connection.execute(
                "INSERT INTO request_history
                 (id, request_id, collection_id, method, url, status_code, duration_ms, request_json,
                  response_meta_json, response_body_path, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?)",
                params![
                    history_id,
                    input.request.id,
                    input.request.collection_id,
                    input.request.method,
                    preview.url,
                    status.as_u16() as i64,
                    duration_ms as i64,
                    json!(input.request).to_string(),
                    json!({"headers": headers, "bodyJson": body_json}).to_string(),
                    now
                ],
            )?;
            Ok(())
        })
        .map_err(|error| error.to_string())?;

    Ok(SendRequestResult {
        history_id,
        status_code: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or_default().to_string(),
        duration_ms,
        headers,
        body_text,
        body_json,
        updated_variables,
        unresolved_variables: preview.unresolved_variables,
    })
}

fn load_variable_context(
    connection: &rusqlite::Connection,
    request: &RequestDetail,
) -> Result<HashMap<String, String>, StorageError> {
    let mut variables = HashMap::new();
    load_variables_for_scope(connection, "global", "global", &mut variables)?;
    load_variables_for_scope(
        connection,
        "collection",
        &request.collection_id,
        &mut variables,
    )?;

    let folder_ids = ancestor_folder_ids(connection, &request.id)?;
    for folder_id in folder_ids {
        load_variables_for_scope(connection, "folder", &folder_id, &mut variables)?;
    }

    load_variables_for_scope(connection, "request", &request.id, &mut variables)?;
    Ok(variables)
}

fn load_variables_for_scope(
    connection: &rusqlite::Connection,
    scope_kind: &str,
    scope_id: &str,
    variables: &mut HashMap<String, String>,
) -> Result<(), StorageError> {
    let mut statement = connection.prepare(
        "SELECT key, value FROM variables
         WHERE scope_kind = ? AND scope_id = ? AND enabled = 1
         ORDER BY key",
    )?;
    let rows = statement.query_map(params![scope_kind, scope_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (key, value) = row?;
        variables.insert(key, value);
    }
    Ok(())
}

fn ancestor_folder_ids(
    connection: &rusqlite::Connection,
    request_id: &str,
) -> Result<Vec<String>, StorageError> {
    let mut parent_id: Option<String> = connection
        .query_row(
            "SELECT parent_id FROM collection_nodes WHERE request_id = ?",
            params![request_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    let mut ids = Vec::new();

    while let Some(id) = parent_id {
        ids.push(id.clone());
        parent_id = connection
            .query_row(
                "SELECT parent_id FROM collection_nodes WHERE id = ?",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
    }

    ids.reverse();
    Ok(ids)
}

fn inherited_auth_for_request(
    connection: &rusqlite::Connection,
    request: &RequestDetail,
) -> Result<Option<super::models::AuthConfig>, StorageError> {
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

    let collection_auth: Option<(Option<String>, Option<String>)> = connection
        .query_row(
            "SELECT auth_json, raw_postman_json FROM collections WHERE id = ?",
            params![request.collection_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    if let Some((auth_json, raw_postman_json)) = collection_auth {
        if let Some(auth) = auth_json.as_deref().and_then(parse_json_optional) {
            return Ok(Some(auth));
        }
        if let Some(auth) = raw_postman_json
            .as_deref()
            .and_then(parse_json_optional::<Value>)
            .and_then(|collection| collection.get("auth").cloned())
            .and_then(|auth| normalize_postman_auth(&auth))
        {
            return Ok(Some(auth));
        }
    }

    Ok(None)
}

fn normalize_postman_auth(auth: &Value) -> Option<super::models::AuthConfig> {
    let auth_type = auth.get("type").and_then(Value::as_str).unwrap_or("noauth");
    match auth_type {
        "bearer" => Some(super::models::AuthConfig {
            auth_type: "bearer".to_string(),
            token: postman_auth_value(auth, "bearer", "token"),
            username: None,
            password: None,
            key: None,
            value: None,
            add_to: None,
        }),
        "basic" => Some(super::models::AuthConfig {
            auth_type: "basic".to_string(),
            token: None,
            username: postman_auth_value(auth, "basic", "username"),
            password: postman_auth_value(auth, "basic", "password"),
            key: None,
            value: None,
            add_to: None,
        }),
        "apikey" => Some(super::models::AuthConfig {
            auth_type: "apikey".to_string(),
            token: None,
            username: None,
            password: None,
            key: postman_auth_value(auth, "apikey", "key")
                .or_else(|| Some("Authorization".to_string())),
            value: postman_auth_value(auth, "apikey", "value"),
            add_to: postman_auth_value(auth, "apikey", "in").or_else(|| Some("header".to_string())),
        }),
        "noauth" => Some(super::models::AuthConfig {
            auth_type: "noauth".to_string(),
            token: None,
            username: None,
            password: None,
            key: None,
            value: None,
            add_to: None,
        }),
        _ => None,
    }
}

fn postman_auth_value(auth: &Value, auth_type: &str, key: &str) -> Option<String> {
    auth.get(auth_type)
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find_map(|item| {
                (item.get("key").and_then(Value::as_str) == Some(key)).then(|| {
                    item.get("value")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                })
            })
        })
        .map(ToString::to_string)
}

fn effective_auth(
    request_auth: Option<&super::models::AuthConfig>,
    inherited_auth: Option<&super::models::AuthConfig>,
) -> Option<super::models::AuthConfig> {
    match request_auth {
        Some(auth) if auth.auth_type == "noauth" => Some(auth.clone()),
        Some(auth) => Some(auth.clone()),
        None => inherited_auth.cloned(),
    }
}

fn resolve_request_with_context(
    request: &RequestDetail,
    variables: &HashMap<String, String>,
) -> ResolvedRequestPreview {
    let mut unresolved = HashMap::<String, Vec<String>>::new();
    let (url, missing) = resolve_text(&request.url, variables);
    add_missing(&mut unresolved, "url", missing);

    let headers = request
        .headers
        .iter()
        .map(|header| {
            let (value, missing) = resolve_text(&header.value, variables);
            add_missing(&mut unresolved, &format!("header:{}", header.key), missing);
            KeyValue {
                key: header.key.clone(),
                value,
                enabled: header.enabled,
            }
        })
        .collect();

    let query = request
        .query
        .iter()
        .map(|query| {
            let (value, missing) = resolve_text(&query.value, variables);
            add_missing(&mut unresolved, &format!("query:{}", query.key), missing);
            KeyValue {
                key: query.key.clone(),
                value,
                enabled: query.enabled,
            }
        })
        .collect();

    let body = request
        .body
        .as_ref()
        .map(|body| resolve_body(body, variables, &mut unresolved));

    let auth_to_resolve = effective_auth(request.auth.as_ref(), request.inherited_auth.as_ref());
    if let Some(auth) = auth_to_resolve.as_ref() {
        if auth.auth_type == "bearer" {
            if let Some(token) = auth.token.as_ref() {
                let (_, missing) = resolve_text(token, variables);
                add_missing(&mut unresolved, "auth:bearer", missing);
            }
        } else if auth.auth_type == "basic" {
            if let Some(username) = auth.username.as_ref() {
                let (_, missing) = resolve_text(username, variables);
                add_missing(&mut unresolved, "auth:basic.username", missing);
            }
            if let Some(password) = auth.password.as_ref() {
                let (_, missing) = resolve_text(password, variables);
                add_missing(&mut unresolved, "auth:basic.password", missing);
            }
        } else if auth.auth_type == "apikey" {
            if let Some(value) = auth.value.as_ref() {
                let (_, missing) = resolve_text(value, variables);
                add_missing(&mut unresolved, "auth:apikey.value", missing);
            }
        }
    }

    ResolvedRequestPreview {
        url,
        headers,
        query,
        body,
        unresolved_variables: unresolved
            .into_iter()
            .map(|(key, locations)| UnresolvedVariable { key, locations })
            .collect(),
    }
}

fn resolve_body(
    body: &RequestBody,
    variables: &HashMap<String, String>,
    unresolved: &mut HashMap<String, Vec<String>>,
) -> RequestBody {
    let (raw, missing) = resolve_text(&body.raw, variables);
    add_missing(unresolved, "body", missing);
    RequestBody {
        mode: body.mode.clone(),
        raw,
        raw_language: body.raw_language.clone(),
        form_data: body
            .form_data
            .iter()
            .map(|field| {
                let (value, missing) = resolve_text(&field.value, variables);
                add_missing(unresolved, &format!("form:{}", field.key), missing);
                BodyField {
                    key: field.key.clone(),
                    value,
                    enabled: field.enabled,
                    field_type: field.field_type.clone(),
                    file_path: field.file_path.clone(),
                }
            })
            .collect(),
        urlencoded: body
            .urlencoded
            .iter()
            .map(|field| {
                let (value, missing) = resolve_text(&field.value, variables);
                add_missing(unresolved, &format!("urlencoded:{}", field.key), missing);
                KeyValue {
                    key: field.key.clone(),
                    value,
                    enabled: field.enabled,
                }
            })
            .collect(),
    }
}

fn resolve_text(text: &str, variables: &HashMap<String, String>) -> (String, Vec<String>) {
    let mut output = String::with_capacity(text.len());
    let mut missing = Vec::new();
    let mut rest = text;

    while let Some(start) = rest.find("{{") {
        let (before, after_start) = rest.split_at(start);
        output.push_str(before);
        if let Some(end) = after_start.find("}}") {
            let key = after_start[2..end].trim();
            if let Some(value) = variables.get(key) {
                output.push_str(value);
            } else {
                output.push_str(&after_start[..end + 2]);
                missing.push(key.to_string());
            }
            rest = &after_start[end + 2..];
        } else {
            output.push_str(after_start);
            rest = "";
        }
    }
    output.push_str(rest);
    (output, missing)
}

fn add_missing(
    unresolved: &mut HashMap<String, Vec<String>>,
    location: &str,
    missing: Vec<String>,
) {
    for key in missing {
        unresolved
            .entry(key)
            .or_default()
            .push(location.to_string());
    }
}

async fn apply_body(
    mut builder: reqwest::RequestBuilder,
    body: &RequestBody,
) -> Result<reqwest::RequestBuilder, String> {
    match body.mode.as_str() {
        "raw" => {
            builder = builder.body(body.raw.clone());
        }
        "urlencoded" => {
            let pairs = body
                .urlencoded
                .iter()
                .filter(|field| field.enabled && !field.key.is_empty())
                .map(|field| (field.key.clone(), field.value.clone()))
                .collect::<Vec<_>>();
            builder = builder.form(&pairs);
        }
        "formdata" => {
            let mut form = reqwest::multipart::Form::new();
            for field in body
                .form_data
                .iter()
                .filter(|field| field.enabled && !field.key.is_empty())
            {
                if field.field_type == "file" {
                    if let Some(path) = field.file_path.as_ref().filter(|path| !path.is_empty()) {
                        let bytes = fs::read(path).map_err(|error| error.to_string())?;
                        let file_name = std::path::Path::new(path)
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("upload")
                            .to_string();
                        let part = reqwest::multipart::Part::bytes(bytes).file_name(file_name);
                        form = form.part(field.key.clone(), part);
                    }
                } else {
                    form = form.text(field.key.clone(), field.value.clone());
                }
            }
            builder = builder.multipart(form);
        }
        _ => {}
    }
    Ok(builder)
}

fn apply_auth(
    mut builder: reqwest::RequestBuilder,
    auth: Option<&super::models::AuthConfig>,
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

fn normalize_body_files(request: &mut RequestDetail) {
    if let Some(body) = &mut request.body {
        for field in &mut body.form_data {
            if field.field_type == "file" {
                field.file_path = None;
            }
        }
    }
}

fn shift_node_positions(
    tx: &rusqlite::Transaction<'_>,
    collection_id: &str,
    parent_id: Option<&str>,
    from_position: i64,
) -> Result<(), StorageError> {
    match parent_id {
        Some(parent_id) => {
            tx.execute(
                "UPDATE collection_nodes
                 SET position = position + 1
                 WHERE collection_id = ? AND parent_id = ? AND position >= ?",
                params![collection_id, parent_id, from_position],
            )?;
        }
        None => {
            tx.execute(
                "UPDATE collection_nodes
                 SET position = position + 1
                 WHERE collection_id = ? AND parent_id IS NULL AND position >= ?",
                params![collection_id, from_position],
            )?;
        }
    }
    Ok(())
}

fn close_position_gap(
    tx: &rusqlite::Transaction<'_>,
    collection_id: &str,
    parent_id: Option<&str>,
    old_position: i64,
) -> Result<(), StorageError> {
    match parent_id {
        Some(parent_id) => {
            tx.execute(
                "UPDATE collection_nodes
                 SET position = position - 1
                 WHERE collection_id = ? AND parent_id = ? AND position > ?",
                params![collection_id, parent_id, old_position],
            )?;
        }
        None => {
            tx.execute(
                "UPDATE collection_nodes
                 SET position = position - 1
                 WHERE collection_id = ? AND parent_id IS NULL AND position > ?",
                params![collection_id, old_position],
            )?;
        }
    }
    Ok(())
}

fn open_position_gap(
    tx: &rusqlite::Transaction<'_>,
    collection_id: &str,
    parent_id: Option<&str>,
    new_position: i64,
) -> Result<(), StorageError> {
    shift_node_positions(tx, collection_id, parent_id, new_position)
}

fn collect_request_ids_for_node(
    tx: &rusqlite::Transaction<'_>,
    node_id: &str,
) -> Result<Vec<String>, StorageError> {
    let mut request_ids = Vec::new();
    let mut stack = vec![node_id.to_string()];

    while let Some(id) = stack.pop() {
        let request_id: Option<String> = tx.query_row(
            "SELECT request_id FROM collection_nodes WHERE id = ?",
            params![id],
            |row| row.get(0),
        )?;
        if let Some(request_id) = request_id {
            request_ids.push(request_id);
        }

        let mut statement = tx.prepare("SELECT id FROM collection_nodes WHERE parent_id = ?")?;
        let child_ids = statement
            .query_map(params![id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        stack.extend(child_ids);
    }

    Ok(request_ids)
}

fn is_descendant(
    tx: &rusqlite::Transaction<'_>,
    possible_descendant_id: &str,
    ancestor_id: &str,
) -> Result<bool, StorageError> {
    let mut current = Some(possible_descendant_id.to_string());
    while let Some(id) = current {
        if id == ancestor_id {
            return Ok(true);
        }
        current = tx
            .query_row(
                "SELECT parent_id FROM collection_nodes WHERE id = ?",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
    }
    Ok(false)
}

struct DuplicateSource {
    collection_id: String,
    method: String,
    url: String,
    headers_json: String,
    query_json: String,
    path_params_json: String,
    auth_json: Option<String>,
    body_json: Option<String>,
    raw_postman_item_json: Option<String>,
    pre_request_script_json: Option<String>,
    test_script_json: Option<String>,
    parent_id: Option<String>,
    position: i64,
    name: String,
    variables_json: Option<String>,
    node_auth_json: Option<String>,
}

fn collect_postman_script_variables(
    script: Option<&Value>,
    body_json: Option<&Value>,
) -> Vec<KeyValue> {
    let Some(lines) = script
        .and_then(|script| script.get("exec"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    lines
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|line| parse_postman_variable_set(line, body_json))
        .collect()
}

fn parse_postman_variable_set(line: &str, body_json: Option<&Value>) -> Option<KeyValue> {
    let marker = if line.contains("postman.setEnvironmentVariable") {
        "postman.setEnvironmentVariable"
    } else if line.contains("pm.environment.set") {
        "pm.environment.set"
    } else if line.contains("pm.collectionVariables.set") {
        "pm.collectionVariables.set"
    } else {
        return None;
    };

    let call = line.split_once(marker)?.1;
    let args = call
        .strip_prefix('(')?
        .trim_end_matches(';')
        .trim_end_matches(')');
    let (key_part, value_part) = args.split_once(',')?;
    let key = unquote(key_part.trim())?;
    let value = resolve_script_expression(value_part.trim(), body_json)?;

    Some(KeyValue {
        key,
        value,
        enabled: true,
    })
}

fn resolve_script_expression(expression: &str, body_json: Option<&Value>) -> Option<String> {
    if let Some(value) = unquote(expression) {
        return Some(value);
    }

    let path = expression.strip_prefix("jsonData.")?;
    let mut current = body_json?;
    for segment in path.split('.') {
        current = current.get(segment.trim())?;
    }

    match current {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => Some(current.to_string()),
    }
}

fn unquote(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.len() < 2 {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let quote = bytes[0];
    if (quote == b'"' || quote == b'\'') && bytes[trimmed.len() - 1] == quote {
        return Some(trimmed[1..trimmed.len() - 1].to_string());
    }
    None
}

fn parse_json<T: serde::de::DeserializeOwned>(value: &str) -> T
where
    T: Default,
{
    serde_json::from_str(value).unwrap_or_default()
}

fn parse_json_optional<T: serde::de::DeserializeOwned>(value: &str) -> Option<T> {
    serde_json::from_str(value).ok()
}
