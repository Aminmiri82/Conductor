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
    Ok(state
        .database
        .with_read_connection(|connection| load_request(connection, &request_id))?)
}

fn load_request(connection: &Connection, request_id: &str) -> Result<RequestDetail, StorageError> {
    Ok(connection.query_row(
        "SELECT r.id, r.collection_id, n.name, r.method, r.url, r.headers_json, r.query_json,
                r.path_params_json, r.auth_json, r.body_json, r.pre_request_script_json,
                r.test_script_json, f.revision
         FROM requests r
         JOIN collection_nodes n ON n.request_id = r.id
         LEFT JOIN repository_files f ON f.collection_id = r.collection_id
            AND f.kind = 'request' AND f.entity_id = r.id
         WHERE r.id = ?",
        [request_id],
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
}

#[tauri::command(async)]
#[specta::specta]
pub fn create_request(
    collection_id: String,
    parent_id: Option<String>,
    position: i64,
    state: State<'_, AppState>,
) -> Result<CreateRequestResult, CommandError> {
    let input = CreateRequestInput {
        collection_id,
        parent_id,
        position,
        name: repository::NEW_REQUEST.into(),
    };
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
        name: input.name.clone(),
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
                n.parent_id, n.sort_order, n.name, n.auth_json, n.id
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
                node_id: row.get(14)?,
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
        &source.node_id,
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
        refresh: Option<repository::RepositoryRefresh>,
    },
    Conflict {
        revision: Option<String>,
        refresh: Option<repository::RepositoryRefresh>,
    },
    Failed {
        message: String,
        refresh: repository::RepositoryRefresh,
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
            SaveRequestResult::Saved {
                revision: None,
                refresh: None,
            }
        });
    };
    let baseline = repository::check_request(
        connection,
        &owner,
        &request.id,
        request.repository_revision.as_deref(),
    )?;

    // Local saves and repository files both leave file upload paths out.
    if let Some(request_body) = &mut request.body {
        body::strip_file_paths(request_body);
    }
    let save = |connection: &mut Connection| -> Result<(), StorageError> {
        let tx = connection.savepoint()?;
        update_request(&tx, &request)?;
        tx.commit()?;
        Ok(())
    };
    match baseline {
        repository::RequestBaseline::Conflict { revision, refresh } => {
            Ok(SaveRequestResult::Conflict { revision, refresh })
        }
        repository::RequestBaseline::Local => {
            save(connection)?;
            Ok(SaveRequestResult::Saved {
                revision: None,
                refresh: None,
            })
        }
        repository::RequestBaseline::Current { refresh } => {
            let save_current = |connection: &mut Connection| {
                if repository::revision(connection, &request.id)? != request.repository_revision {
                    return Ok(false);
                }
                save(connection)?;
                Ok(true)
            };
            match repository::mutate_scoped(connection, &owner, Some(&request.id), save_current) {
                Ok((true, revision)) => Ok(SaveRequestResult::Saved { revision, refresh }),
                Ok((false, revision)) => Ok(SaveRequestResult::Conflict { revision, refresh }),
                Err(error) => match refresh {
                    Some(refresh) => Ok(SaveRequestResult::Failed {
                        message: error.to_string(),
                        refresh,
                    }),
                    None => Err(error),
                },
            }
        }
    }
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
        Ok(created)
    })
}

fn update_request(connection: &Connection, request: &RequestDetail) -> Result<(), StorageError> {
    let now = Utc::now().to_rfc3339();
    connection.execute(
        "UPDATE requests
         SET method = ?, url = ?, headers_json = ?, query_json = ?, path_params_json = ?,
             auth_json = ?, body_json = ?, pre_request_script_json = ?, test_script_json = ?,
             updated_at = ?
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
    node_id: String,
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

    #[test]
    fn a_duplicate_lands_below_the_original_when_sibling_positions_are_equal() {
        let mut connection = test_connection();
        let collection = fixtures::collection(&connection);
        let first = fixtures::request(&mut connection, &collection, None);
        let second = fixtures::request(&mut connection, &collection, None);
        connection
            .execute(
                "UPDATE collection_nodes SET sort_order = 1024 WHERE collection_id = ?",
                [&collection],
            )
            .unwrap();
        let (first, second) = if first.node_id < second.node_id {
            (first, second)
        } else {
            (second, first)
        };

        let copy = copy_request(&mut connection, &first.request_id).unwrap();

        let tree = read_tree(&connection, &collection).unwrap();
        let order = tree.iter().map(|node| node.id.as_str()).collect::<Vec<_>>();
        assert_eq!(order, [first.node_id, copy.node_id, second.node_id]);
    }

    #[test]
    fn overwriting_a_repository_conflict_keeps_the_draft_scripts_after_reopening() {
        use crate::commands::models::SendRequestInput;
        use crate::fixtures::TestApp;
        use crate::test_server::TestServer;
        use serde_json::{json, Value};
        use std::{fs, path::PathBuf};

        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        let root = std::env::temp_dir().join(format!("conductor-repo-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let root = Scratch(fs::canonicalize(root).unwrap());
        let server = TestServer::replying(200, "application/json", "{}");
        let app = TestApp::new();
        let pre_request = json!({"exec": ["pm.collectionVariables.set('setup', 'draft');"]});
        let test_script = |value: &str| json!({ "exec": [format!("pm.collectionVariables.set('token', '{value}');")] });

        let mut draft = app.seed(|connection| {
            let link = repository::connect(connection, &root.0, true, None).unwrap();
            let created = save_as_new(
                connection,
                RequestDetail {
                    collection_id: link.collection_id,
                    name: "Log in".into(),
                    method: "POST".into(),
                    url: format!("{}/login", server.url),
                    pre_request_script: Some(pre_request.clone()),
                    test_script: Some(test_script("draft")),
                    ..RequestDetail::default()
                },
            )
            .unwrap();
            load_request(connection, &created.request_id).unwrap()
        });

        let group_path = root.0.join(".conductor/requests/root.json");
        let mut group: Value = serde_json::from_slice(&fs::read(&group_path).unwrap()).unwrap();
        let external = group["requests"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        external["preRequestScript"] = Value::Null;
        external["testScript"] = test_script("external");
        fs::write(&group_path, serde_json::to_vec_pretty(&group).unwrap()).unwrap();

        draft.name = "Log in with my draft".into();
        app.seed(|connection| {
            let SaveRequestResult::Conflict { revision, .. } =
                store_request(connection, draft.clone()).unwrap()
            else {
                panic!("the external script changes must conflict");
            };
            draft.repository_revision = revision;
            assert!(matches!(
                store_request(connection, draft.clone()).unwrap(),
                SaveRequestResult::Saved { .. }
            ));
        });

        let send_now = |app: &TestApp, request| {
            tauri::async_runtime::block_on(super::super::sender::send(
                &app.state,
                SendRequestInput {
                    request,
                    environment_id: None,
                },
            ))
            .unwrap()
        };
        let immediate = send_now(&app, draft);
        let reopened = TestApp::new();
        let reopened_request = reopened.seed(|connection| {
            let link = repository::connect(connection, &root.0, false, None).unwrap();
            let CollectionNodeKind::Request { request_id, .. } = &link.tree[0].kind else {
                panic!("the saved request must be present when reopening the repository");
            };
            load_request(connection, request_id).unwrap()
        });
        let pre_request_after_reopen = reopened_request.pre_request_script.clone();
        let after_reopen = send_now(&reopened, reopened_request);

        assert_eq!(immediate.updated_variables.len(), 1);
        assert_eq!(immediate.updated_variables[0].value, "draft");
        assert_eq!(after_reopen.updated_variables.len(), 1);
        assert_eq!(
            after_reopen.updated_variables[0].value,
            immediate.updated_variables[0].value
        );
        assert_eq!(pre_request_after_reopen, Some(pre_request));
    }
}
