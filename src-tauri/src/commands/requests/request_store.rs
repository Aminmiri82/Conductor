use chrono::Utc;
use rusqlite::params;
use serde_json::json;
use tauri::State;
use uuid::Uuid;

use crate::commands::models::{CreateRequestInput, DuplicateRequestInput, RequestDetail};
use crate::AppState;

use super::{collection_tree::shift_node_positions, request_auth, request_body};

#[tauri::command]
pub fn get_request(
    request_id: String,
    state: State<'_, AppState>,
) -> Result<RequestDetail, String> {
    state
        .database
        .with_read_connection(|connection| {
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
            request.inherited_auth = request_auth::inherited_auth_for_request(&connection, &request)?;
            request.effective_auth = request_auth::effective_auth(request.auth.as_ref(), request.inherited_auth.as_ref());
            request_body::normalize_body_files(&mut request);
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
                  auth_json, body_json, pre_request_script_json,
                  test_script_json, created_at, updated_at)
                 VALUES (?, ?, 'GET', '', '[]', '[]', '[]', NULL, NULL, NULL, NULL, ?, ?)",
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
                        r.path_params_json, r.auth_json, r.body_json,
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
                        pre_request_script_json: row.get(8)?,
                        test_script_json: row.get(9)?,
                        parent_id: row.get(10)?,
                        position: row.get(11)?,
                        name: row.get::<_, String>(12)?,
                        variables_json: row.get(13)?,
                        node_auth_json: row.get(14)?,
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
                  auth_json, body_json, pre_request_script_json,
                  test_script_json, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
struct DuplicateSource {
    collection_id: String,
    method: String,
    url: String,
    headers_json: String,
    query_json: String,
    path_params_json: String,
    auth_json: Option<String>,
    body_json: Option<String>,
    pre_request_script_json: Option<String>,
    test_script_json: Option<String>,
    parent_id: Option<String>,
    position: i64,
    name: String,
    variables_json: Option<String>,
    node_auth_json: Option<String>,
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
