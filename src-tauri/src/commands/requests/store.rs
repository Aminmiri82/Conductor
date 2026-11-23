use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::commands::{
    collections::{repository, sort_order_after, sort_order_for_position},
    error::CommandError,
    models::{
        CreateRequestInput, CreateRequestResult, DuplicateRequestInput, DuplicateRequestResult,
        RequestDetail,
    },
};
use crate::storage::{insert_node, insert_request, NewNode, NewRequest, NodeKind, StorageError};
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
                    r.test_script_json, f.revision
             FROM requests r
             JOIN collection_nodes n ON n.request_id = r.id
             LEFT JOIN repository_files f ON f.collection_id = r.collection_id
                AND f.kind = 'request' AND f.entity_id = r.id
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
                    repository_revision: row.get(12)?,
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
    Ok(state.database.with_connection(|connection| {
        repository::mutate(connection, &input.collection_id, |connection| {
            add_request(connection, &input)
        })
    })?)
}

pub(crate) fn add_request(
    connection: &mut Connection,
    input: &CreateRequestInput,
) -> Result<CreateRequestResult, StorageError> {
    let now = Utc::now().to_rfc3339();
    let request_id = repository::new_entity_id(connection, &input.collection_id)?;
    let node_id = request_node_id(&request_id);
    let tx = connection.savepoint()?;
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
        request_id,
        node_id,
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn duplicate_request(
    input: DuplicateRequestInput,
    state: State<'_, AppState>,
) -> Result<DuplicateRequestResult, CommandError> {
    Ok(state.database.with_connection(|connection| {
        let owner = request_owner(connection, &input.request_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        repository::mutate(connection, &owner, |connection| {
            copy_request(connection, &input.request_id)
        })
    })?)
}

/// Inserts a copy named "<name> Copy" directly below the original.
pub(crate) fn copy_request(
    connection: &mut Connection,
    request_id: &str,
) -> Result<DuplicateRequestResult, StorageError> {
    let now = Utc::now().to_rfc3339();
    let tx = connection.savepoint()?;
    let source = tx.query_row(
        "SELECT r.collection_id, r.method, r.url, r.headers_json, r.query_json,
                r.path_params_json, r.auth_json, r.body_json,
                r.pre_request_script_json, r.test_script_json,
                n.parent_id, n.sort_order, n.name, n.auth_json
         FROM requests r
         JOIN collection_nodes n ON n.request_id = r.id
         WHERE r.id = ?",
        params![request_id],
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
    let new_request_id = repository::new_entity_id(&tx, &source.collection_id)?;
    let new_node_id = request_node_id(&new_request_id);
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
        request_id: new_request_id,
        node_id: new_node_id,
    })
}

fn request_node_id(request_id: &str) -> String {
    if request_id.contains('/') {
        request_id.to_string()
    } else {
        Uuid::new_v4().to_string()
    }
}

fn request_owner(
    connection: &Connection,
    request_id: &str,
) -> Result<Option<String>, StorageError> {
    Ok(connection
        .query_row(
            "SELECT collection_id FROM requests WHERE id = ?",
            [request_id],
            |row| row.get(0),
        )
        .optional()?)
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum SaveRequestResult {
    Saved {
        revision: Option<String>,
    },
    Conflict {
        revision: Option<String>,
        refresh: Option<repository::RepositoryRefresh>,
    },
}

#[tauri::command(async)]
#[specta::specta]
pub fn save_request(
    request: RequestDetail,
    state: State<'_, AppState>,
) -> Result<SaveRequestResult, CommandError> {
    Ok(state
        .database
        .with_connection(|connection| store_request(connection, request))?)
}

pub(crate) fn store_request(
    connection: &mut Connection,
    mut request: RequestDetail,
) -> Result<SaveRequestResult, StorageError> {
    let Some(owner) = request_owner(connection, &request.id)? else {
        return Ok(if request.repository_revision.is_some() {
            SaveRequestResult::Conflict {
                revision: None,
                refresh: None,
            }
        } else {
            SaveRequestResult::Saved { revision: None }
        });
    };
    if let repository::RequestBaseline::Conflict { revision, refresh } = repository::check_request(
        connection,
        &owner,
        &request.id,
        request.repository_revision.as_deref(),
    )? {
        return Ok(SaveRequestResult::Conflict { revision, refresh });
    }

    // Local saves and repository files both leave file upload paths out.
    if let Some(request_body) = &mut request.body {
        body::strip_file_paths(request_body);
    }
    repository::mutate(connection, &owner, |connection| {
        let tx = connection.savepoint()?;
        update_request(&tx, &request)?;
        tx.commit()?;
        Ok(())
    })?;
    Ok(SaveRequestResult::Saved {
        revision: repository::revision(connection, &request.id)?,
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn save_repository_request_as_new(
    request: RequestDetail,
    state: State<'_, AppState>,
) -> Result<CreateRequestResult, CommandError> {
    Ok(state
        .database
        .with_connection(|connection| save_as_new(connection, request))?)
}

fn save_as_new(
    connection: &mut Connection,
    mut request: RequestDetail,
) -> Result<CreateRequestResult, StorageError> {
    let collection = request.collection_id.clone();
    if !repository::linked(connection, &collection)? {
        return Err(StorageError::InvalidInput(
            "Select a linked collection.".into(),
        ));
    }
    if let Some(request_body) = &mut request.body {
        body::strip_file_paths(request_body);
    }
    repository::mutate(connection, &collection, |connection| {
        let created = add_request(
            connection,
            &CreateRequestInput {
                collection_id: collection.clone(),
                name: request.name.clone(),
                parent_id: None,
                position: i64::MAX,
            },
        )?;
        request.id.clone_from(&created.request_id);
        update_request(connection, &request)?;
        connection.execute(
            "UPDATE requests SET pre_request_script_json = ?, test_script_json = ? WHERE id = ?",
            params![
                request
                    .pre_request_script
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                request
                    .test_script
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                request.id,
            ],
        )?;
        Ok(created)
    })
}

fn update_request(connection: &Connection, request: &RequestDetail) -> Result<(), StorageError> {
    let now = Utc::now().to_rfc3339();
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
            request.id,
        ],
    )?;
    connection.execute(
        "UPDATE collection_nodes SET name = ?, updated_at = ? WHERE request_id = ?",
        params![request.name, now, request.id],
    )?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{collections::read_tree, models::CollectionNodeKind};
    use crate::{fixtures, storage::test_connection};

    #[test]
    fn a_duplicate_lands_directly_below_the_original_with_its_url() {
        let mut connection = test_connection();
        let collection = fixtures::collection(&connection);
        let original = fixtures::request(&mut connection, &collection, None);
        let next = fixtures::request(&mut connection, &collection, None);
        connection
            .execute(
                "UPDATE requests SET method = 'POST', url = 'http://127.0.0.1/items' WHERE id = ?",
                params![original.request_id],
            )
            .unwrap();

        let copy = copy_request(&mut connection, &original.request_id).unwrap();

        let tree = read_tree(&connection, &collection).unwrap();
        let order = tree.iter().map(|node| node.id.as_str()).collect::<Vec<_>>();
        assert_eq!(
            order,
            [
                original.node_id.as_str(),
                copy.node_id.as_str(),
                next.node_id.as_str()
            ]
        );
        assert_eq!(tree[1].name, "Request Copy");
        assert!(matches!(
            &tree[1].kind,
            CollectionNodeKind::Request { method, .. } if method == "POST"
        ));
        let url: String = connection
            .query_row(
                "SELECT url FROM requests WHERE id = ?",
                params![copy.request_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(url, "http://127.0.0.1/items");
    }
}
