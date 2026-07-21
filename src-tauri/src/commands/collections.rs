use std::{collections::HashMap, fs, path::Path};

use chrono::Utc;
use rusqlite::params;
use serde_json::{json, Value};
use tauri::State;

use crate::{storage::StorageError, AppState};

use super::models::{CollectionNode, CollectionSummary, EntityId};

const SORT_ORDER_STEP: i64 = 1024;

#[tauri::command]
pub fn list_collections(state: State<'_, AppState>) -> Result<Vec<CollectionSummary>, String> {
    state
        .database
        .with_read_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, name, source, updated_at FROM collections ORDER BY updated_at DESC",
            )?;
            let rows = statement.query_map([], |row| {
                Ok(CollectionSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    source: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(StorageError::from)
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_collection_tree(
    collection_id: EntityId,
    state: State<'_, AppState>,
) -> Result<Vec<CollectionNode>, String> {
    state
        .database
        .with_read_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT n.id, n.collection_id, n.parent_id, n.kind, n.name, n.request_id,
                        r.method
                 FROM collection_nodes n
                 LEFT JOIN requests r ON r.id = n.request_id
                 WHERE n.collection_id = ?
                 ORDER BY n.parent_id, n.sort_order",
            )?;
            let rows = statement.query_map(params![collection_id], |row| {
                Ok(FlatNode {
                    id: row.get(0)?,
                    collection_id: row.get(1)?,
                    parent_id: row.get(2)?,
                    kind: row.get(3)?,
                    name: row.get(4)?,
                    request_id: row.get(5)?,
                    method: row.get(6)?,
                })
            })?;
            let nodes = rows.collect::<Result<Vec<_>, _>>()?;
            Ok(build_tree(nodes, None))
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn import_postman_collection(
    postman_json: String,
    state: State<'_, AppState>,
) -> Result<EntityId, String> {
    let collection: Value =
        serde_json::from_str(&postman_json).map_err(|error| error.to_string())?;
    let name = collection
        .pointer("/info/name")
        .and_then(Value::as_str)
        .unwrap_or("Imported collection")
        .to_string();
    let now = Utc::now().to_rfc3339();
    let raw_import_dir = state.database.raw_import_dir().to_path_buf();

    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            tx.execute(
                "INSERT INTO collections
                 (name, source, auth_json, raw_postman_file_path, created_at, updated_at)
                 VALUES (?, 'postman', ?, NULL, ?, ?)",
                params![
                    name,
                    postman_auth(&collection).map(|value| value.to_string()),
                    now,
                    now
                ],
            )?;
            let collection_id = tx.last_insert_rowid();
            let raw_collection_path =
                write_raw_import_file(&raw_import_dir, collection_id, "collection", &postman_json)?;
            tx.execute(
                "UPDATE collections SET raw_postman_file_path = ? WHERE id = ?",
                params![raw_collection_path, collection_id],
            )?;

            if let Some(variables) = collection.get("variable").and_then(Value::as_array) {
                for variable in variables {
                    insert_variable(&tx, "collection", collection_id, variable, &now)?;
                }
            }

            if let Some(items) = collection.get("item").and_then(Value::as_array) {
                import_items(&tx, collection_id, None, items, &now)?;
            }

            tx.commit()?;
            Ok(collection_id)
        })
        .map_err(|error| error.to_string())
}

fn import_items(
    tx: &rusqlite::Transaction<'_>,
    collection_id: EntityId,
    parent_id: Option<EntityId>,
    items: &[Value],
    now: &str,
) -> Result<(), StorageError> {
    for (position, item) in items.iter().enumerate() {
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Untitled")
            .to_string();

        if let Some(children) = item.get("item").and_then(Value::as_array) {
            tx.execute(
                "INSERT INTO collection_nodes
                 (collection_id, parent_id, sort_order, kind, name, request_id, auth_json, created_at, updated_at)
                 VALUES (?, ?, ?, 'folder', ?, NULL, ?, ?, ?)",
                params![
                    collection_id,
                    parent_id,
                    sort_order_for_import(position),
                    name,
                    postman_auth(item).map(|value| value.to_string()),
                    now,
                    now
                ],
            )?;
            let node_id = tx.last_insert_rowid();

            import_items(tx, collection_id, Some(node_id), children, now)?;
        } else if let Some(request) = item.get("request") {
            let method = request
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("GET")
                .to_uppercase();
            let url = postman_url_raw(request);
            let headers = postman_headers(request);
            let query = postman_query(request);
            let body = postman_body(request);
            let auth = postman_auth(request);
            let (pre_request_script, test_script) = postman_scripts(item);

            tx.execute(
                "INSERT INTO requests
                 (collection_id, method, url, headers_json, query_json, path_params_json, auth_json, body_json,
                  pre_request_script_json, test_script_json, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, '[]', ?, ?, ?, ?, ?, ?)",
                params![
                    collection_id,
                    method,
                    url,
                    headers.to_string(),
                    query.to_string(),
                    auth.map(|value| value.to_string()),
                    body.map(|value| value.to_string()),
                    pre_request_script.map(|value| value.to_string()),
                    test_script.map(|value| value.to_string()),
                    now,
                    now
                ],
            )?;
            let request_id = tx.last_insert_rowid();

            tx.execute(
                "INSERT INTO collection_nodes
                 (collection_id, parent_id, sort_order, kind, name, request_id, created_at, updated_at)
                 VALUES (?, ?, ?, 'request', ?, ?, ?, ?)",
                params![
                    collection_id,
                    parent_id,
                    sort_order_for_import(position),
                    name,
                    request_id,
                    now,
                    now
                ],
            )?;
        }
    }

    Ok(())
}

fn write_raw_import_file(
    raw_import_dir: &Path,
    collection_id: EntityId,
    name: &str,
    contents: &str,
) -> Result<String, StorageError> {
    let collection_dir = raw_import_dir.join(collection_id.to_string());
    fs::create_dir_all(&collection_dir).map_err(|source| StorageError::FileOperation {
        path: collection_dir.clone(),
        source,
    })?;
    let path = collection_dir.join(format!("{name}.json"));
    fs::write(&path, contents).map_err(|source| StorageError::FileOperation {
        path: path.clone(),
        source,
    })?;
    Ok(path.display().to_string())
}

fn sort_order_for_import(position: usize) -> i64 {
    (position as i64 + 1) * SORT_ORDER_STEP
}

fn insert_variable(
    tx: &rusqlite::Transaction<'_>,
    scope: &str,
    collection_id: EntityId,
    variable: &Value,
    now: &str,
) -> Result<(), StorageError> {
    let Some(key) = variable.get("key").and_then(Value::as_str) else {
        return Ok(());
    };
    let value = variable
        .get("value")
        .map(value_to_string)
        .unwrap_or_default();
    let enabled = !variable
        .get("disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let variable_type = variable
        .get("type")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    tx.execute(
        "INSERT OR REPLACE INTO variables
         (scope, collection_id, environment_id, key, initial_value, current_value,
          enabled, sensitive, variable_type, created_at, updated_at)
         VALUES (?, ?, NULL, ?, ?, ?, ?, 0, ?, ?, ?)",
        params![
            scope,
            collection_id,
            key,
            value,
            value,
            enabled as i64,
            variable_type,
            now,
            now
        ],
    )?;
    Ok(())
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

fn postman_url_raw(request: &Value) -> String {
    match request.get("url") {
        Some(Value::String(url)) => url.clone(),
        Some(Value::Object(_)) => request
            .pointer("/url/raw")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

fn postman_headers(request: &Value) -> Value {
    let headers = request
        .get("header")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|header| {
                    let key = header.get("key").and_then(Value::as_str)?;
                    Some(json!({
                        "key": key,
                        "value": header.get("value").and_then(Value::as_str).unwrap_or_default(),
                        "enabled": !header.get("disabled").and_then(Value::as_bool).unwrap_or(false)
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Value::Array(headers)
}

fn postman_query(request: &Value) -> Value {
    let query = request
        .pointer("/url/query")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|query| {
                    let key = query.get("key").and_then(Value::as_str)?;
                    Some(json!({
                        "key": key,
                        "value": query.get("value").and_then(Value::as_str).unwrap_or_default(),
                        "enabled": !query.get("disabled").and_then(Value::as_bool).unwrap_or(false)
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Value::Array(query)
}

fn postman_body(request: &Value) -> Option<Value> {
    let body = request.get("body")?;
    let mode = body.get("mode").and_then(Value::as_str).unwrap_or("none");
    match mode {
        "raw" => Some(json!({
            "mode": "raw",
            "raw": body.get("raw").and_then(Value::as_str).unwrap_or_default(),
            "rawLanguage": body.pointer("/options/raw/language").and_then(Value::as_str),
            "formData": [],
            "urlencoded": [],
            "graphql": null,
            "file": null
        })),
        "formdata" => Some(json!({
            "mode": "formdata",
            "raw": "",
            "rawLanguage": null,
            "formData": body.get("formdata").and_then(Value::as_array).map(|items| {
                items.iter().filter_map(|field| {
                    let key = field.get("key").and_then(Value::as_str)?;
                    let field_type = field.get("type").and_then(Value::as_str).unwrap_or("text");
                    Some(json!({
                        "key": key,
                        "value": if field_type == "file" { "" } else { field.get("value").and_then(Value::as_str).unwrap_or_default() },
                        "enabled": !field.get("disabled").and_then(Value::as_bool).unwrap_or(false),
                        "fieldType": field_type,
                        "filePath": null,
                        "contentType": field.get("contentType").and_then(Value::as_str)
                    }))
                }).collect::<Vec<_>>()
            }).unwrap_or_default(),
            "urlencoded": [],
            "graphql": null,
            "file": null
        })),
        "urlencoded" => Some(json!({
            "mode": "urlencoded",
            "raw": "",
            "rawLanguage": null,
            "formData": [],
            "urlencoded": body.get("urlencoded").and_then(Value::as_array).map(|items| {
                items.iter().filter_map(|field| {
                    let key = field.get("key").and_then(Value::as_str)?;
                    Some(json!({
                        "key": key,
                        "value": field.get("value").and_then(Value::as_str).unwrap_or_default(),
                        "enabled": !field.get("disabled").and_then(Value::as_bool).unwrap_or(false)
                    }))
                }).collect::<Vec<_>>()
            }).unwrap_or_default(),
            "graphql": null,
            "file": null
        })),
        "graphql" => Some(json!({
            "mode": "graphql",
            "raw": "",
            "rawLanguage": null,
            "formData": [],
            "urlencoded": [],
            "graphql": {
                "query": body.pointer("/graphql/query").and_then(Value::as_str).unwrap_or_default(),
                "variables": body.pointer("/graphql/variables").and_then(Value::as_str).unwrap_or_default()
            },
            "file": null
        })),
        "file" => Some(json!({
            "mode": "file",
            "raw": "",
            "rawLanguage": null,
            "formData": [],
            "urlencoded": [],
            "graphql": null,
            "file": {
                "path": null,
                "contentType": body.pointer("/file/contentType").and_then(Value::as_str)
            }
        })),
        _ => Some(
            json!({"mode": mode, "raw": "", "rawLanguage": null, "formData": [], "urlencoded": [], "graphql": null, "file": null}),
        ),
    }
}

fn postman_auth(request: &Value) -> Option<Value> {
    let auth = request.get("auth")?;
    let auth_type = auth.get("type").and_then(Value::as_str).unwrap_or("noauth");
    match auth_type {
        "bearer" => {
            let token = auth_value(auth, "bearer", "token");
            Some(json!({"authType": "bearer", "token": token}))
        }
        "basic" => Some(json!({
            "authType": "basic",
            "username": auth_value(auth, "basic", "username").unwrap_or_default(),
            "password": auth_value(auth, "basic", "password").unwrap_or_default()
        })),
        "apikey" => Some(json!({
            "authType": "apikey",
            "key": auth_value(auth, "apikey", "key").unwrap_or_else(|| "Authorization".to_string()),
            "value": auth_value(auth, "apikey", "value").unwrap_or_default(),
            "addTo": auth_value(auth, "apikey", "in").unwrap_or_else(|| "header".to_string())
        })),
        "noauth" => Some(json!({"authType": "noauth"})),
        _ => Some(json!({"authType": auth_type})),
    }
}

fn auth_value(auth: &Value, auth_type: &str, key: &str) -> Option<String> {
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

fn postman_scripts(item: &Value) -> (Option<Value>, Option<Value>) {
    let Some(events) = item.get("event").and_then(Value::as_array) else {
        return (None, None);
    };
    let mut pre_request = None;
    let mut test = None;

    for event in events {
        match event.get("listen").and_then(Value::as_str) {
            Some("prerequest") => pre_request = event.get("script").cloned(),
            Some("test") => test = event.get("script").cloned(),
            _ => {}
        }
    }

    (pre_request, test)
}

#[derive(Debug)]
struct FlatNode {
    id: EntityId,
    collection_id: EntityId,
    parent_id: Option<EntityId>,
    kind: String,
    name: String,
    request_id: Option<EntityId>,
    method: Option<String>,
}

fn build_tree(nodes: Vec<FlatNode>, parent_id: Option<EntityId>) -> Vec<CollectionNode> {
    let mut by_parent: HashMap<Option<EntityId>, Vec<FlatNode>> = HashMap::new();
    for node in nodes {
        by_parent
            .entry(node.parent_id.clone())
            .or_default()
            .push(node);
    }
    build_tree_from_map(&mut by_parent, parent_id)
}

fn build_tree_from_map(
    by_parent: &mut HashMap<Option<EntityId>, Vec<FlatNode>>,
    parent_id: Option<EntityId>,
) -> Vec<CollectionNode> {
    let nodes = by_parent.remove(&parent_id).unwrap_or_default();
    nodes
        .into_iter()
        .enumerate()
        .map(|(position, node)| {
            let children = build_tree_from_map(by_parent, Some(node.id.clone()));
            CollectionNode {
                id: node.id,
                collection_id: node.collection_id,
                parent_id: node.parent_id,
                position: position as i64,
                kind: node.kind,
                name: node.name,
                request_id: node.request_id,
                method: node.method,
                children,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postman_graphql_body_preserves_query_and_variables() {
        let request = json!({
            "body": {
                "mode": "graphql",
                "graphql": {
                    "query": "query Viewer($id: ID!) { viewer(id: $id) { name } }",
                    "variables": "{\"id\":\"123\"}"
                }
            }
        });

        let body = postman_body(&request).expect("body should import");

        assert_eq!(body["mode"], "graphql");
        assert_eq!(
            body["graphql"]["query"],
            "query Viewer($id: ID!) { viewer(id: $id) { name } }"
        );
        assert_eq!(body["graphql"]["variables"], "{\"id\":\"123\"}");
    }

    #[test]
    fn postman_file_body_does_not_preserve_imported_path() {
        let request = json!({
            "body": {
                "mode": "file",
                "file": {
                    "src": "/Users/example/private.bin",
                    "contentType": "application/octet-stream"
                }
            }
        });

        let body = postman_body(&request).expect("body should import");

        assert_eq!(body["mode"], "file");
        assert!(body["file"]["path"].is_null());
        assert_eq!(body["file"]["contentType"], "application/octet-stream");
    }
}
