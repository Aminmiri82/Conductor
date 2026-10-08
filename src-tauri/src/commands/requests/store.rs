use chrono::Utc;
use rusqlite::params;
use tauri::State;
use uuid::Uuid;

use crate::commands::{
    collections::{sort_order_after, sort_order_for_position},
    error::CommandError,
    models::{
        CreateRequestInput, CreateRequestResult, DuplicateRequestInput, DuplicateRequestResult,
        RequestDetail,
    },
};
use crate::storage::{insert_node, insert_request, NewNode, NewRequest, NodeKind};
use crate::AppState;

use super::{body, parse_json_optional};

#[tauri::command(async)]
#[specta::specta]
pub fn get_request(
    request_id: String,
    state: State<'_, AppState>,
) -> Result<RequestDetail, CommandError> {
    Ok(state.database.with_read_connection(|connection| {
        Ok(connection.query_row(
            "SELECT r.id, r.collection_id, n.name, r.method, r.url, r.headers_json, r.query_json,
                    r.path_params_json, r.auth_json, r.body_json, r.pre_request_script_json,
                    r.test_script_json
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
                    headers: parse_json_optional(&headers_json).unwrap_or_default(),
                    query: parse_json_optional(&query_json).unwrap_or_default(),
                    path_params: parse_json_optional(&path_params_json).unwrap_or_default(),
                    auth: auth_json.as_deref().and_then(parse_json_optional),
                    body: body_json.as_deref().and_then(parse_json_optional),
                    pre_request_script: pre_request_script_json
                        .as_deref()
                        .and_then(parse_json_optional),
                    test_script: test_script_json.as_deref().and_then(parse_json_optional),
                })
            },
        )?)
    })?)
}

#[tauri::command(async)]
#[specta::specta]
pub fn create_request(
    input: CreateRequestInput,
    state: State<'_, AppState>,
) -> Result<CreateRequestResult, CommandError> {
    let now = Utc::now().to_rfc3339();
    let request_id = Uuid::new_v4().to_string();
    let node_id = Uuid::new_v4().to_string();

    Ok(state.database.with_connection(|connection| {
        let tx = connection.transaction()?;
        let sort_order = sort_order_for_position(
            &tx,
            &input.collection_id,
            input.parent_id.as_deref(),
            input.position,
            None,
        )?;
        insert_request(
            &tx,
            &NewRequest::blank(&request_id, &input.collection_id),
            &now,
        )?;
        insert_node(
            &tx,
            &NewNode {
                id: &node_id,
                collection_id: &input.collection_id,
                parent_id: input.parent_id.as_deref(),
                sort_order,
                kind: NodeKind::Request(&request_id),
                name: &input.name,
                auth_json: None,
            },
            &now,
        )?;
        tx.commit()?;
        Ok(CreateRequestResult {
            request_id: request_id.clone(),
            node_id: node_id.clone(),
        })
    })?)
}

#[tauri::command(async)]
#[specta::specta]
pub fn duplicate_request(
    input: DuplicateRequestInput,
    state: State<'_, AppState>,
) -> Result<DuplicateRequestResult, CommandError> {
    let now = Utc::now().to_rfc3339();
    let new_request_id = Uuid::new_v4().to_string();
    let new_node_id = Uuid::new_v4().to_string();

    Ok(state.database.with_connection(|connection| {
        let tx = connection.transaction()?;
        let source = tx.query_row(
            "SELECT r.collection_id, r.method, r.url, r.headers_json, r.query_json,
                    r.path_params_json, r.auth_json, r.body_json,
                    r.pre_request_script_json, r.test_script_json,
                    n.parent_id, n.sort_order, n.name, n.auth_json
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
                    sort_order: row.get(11)?,
                    name: row.get::<_, String>(12)?,
                    node_auth_json: row.get(13)?,
                })
            },
        )?;
        let sort_order = sort_order_after(
            &tx,
            &source.collection_id,
            source.parent_id.as_deref(),
            source.sort_order,
        )?;
        insert_request(
            &tx,
            &NewRequest {
                method: &source.method,
                url: &source.url,
                headers_json: &source.headers_json,
                query_json: &source.query_json,
                path_params_json: &source.path_params_json,
                auth_json: source.auth_json.as_deref(),
                body_json: source.body_json.as_deref(),
                pre_request_script_json: source.pre_request_script_json.as_deref(),
                test_script_json: source.test_script_json.as_deref(),
                ..NewRequest::blank(&new_request_id, &source.collection_id)
            },
            &now,
        )?;
        insert_node(
            &tx,
            &NewNode {
                id: &new_node_id,
                collection_id: &source.collection_id,
                parent_id: source.parent_id.as_deref(),
                sort_order,
                kind: NodeKind::Request(&new_request_id),
                name: &format!("{} Copy", source.name),
                auth_json: source.node_auth_json.as_deref(),
            },
            &now,
        )?;
        tx.commit()?;
        Ok(DuplicateRequestResult {
            request_id: new_request_id.clone(),
            node_id: new_node_id.clone(),
        })
    })?)
}

#[tauri::command(async)]
#[specta::specta]
pub fn save_request(
    mut request: RequestDetail,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    let now = Utc::now().to_rfc3339();
    // Every write of `body_json` (this, import, duplicate) leaves file paths
    // out, so a stored body never carries one and reads need not strip them.
    if let Some(request_body) = &mut request.body {
        body::strip_file_paths(request_body);
    }

    Ok(state.database.with_connection(|connection| {
        connection.execute(
            "UPDATE requests
             SET method = ?, url = ?, headers_json = ?, query_json = ?, path_params_json = ?,
                 auth_json = ?, body_json = ?, updated_at = ?
             WHERE id = ?",
            params![
                request.method,
                request.url,
                serde_json::to_string(&request.headers)?,
                serde_json::to_string(&request.query)?,
                serde_json::to_string(&request.path_params)?,
                request
                    .auth
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                request
                    .body
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                now,
                request.id
            ],
        )?;
        connection.execute(
            "UPDATE collection_nodes SET name = ?, updated_at = ? WHERE request_id = ?",
            params![request.name, now, request.id],
        )?;
        Ok(())
    })?)
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
    sort_order: i64,
    name: String,
    node_auth_json: Option<String>,
}
