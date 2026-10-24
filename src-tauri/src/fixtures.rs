//! Builders for tests that need rows. Each fills in what the test does not
//! care about, writes through the same functions the commands use, and returns
//! the new id. Start from `storage::test_connection()`, or `TestApp` for tests
//! that go through `AppState` (sending).

use std::path::PathBuf;

use rusqlite::{params, Connection};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::commands::{
    collections::add_folder,
    models::{CreateFolderInput, CreateRequestInput, CreateRequestResult, VariableTarget},
    requests::{add_request, BinaryBodies},
};
use crate::storage::{upsert_variable, Database, NewVariable};
use crate::AppState;

pub fn collection(connection: &Connection) -> String {
    let id = Uuid::new_v4().to_string();
    connection
        .execute(
            "INSERT INTO collections (id, name, created_at, updated_at)
             VALUES (?, 'Collection', '', '')",
            params![id],
        )
        .unwrap();
    id
}

pub fn environment(connection: &Connection, collection_id: &str) -> String {
    let id = Uuid::new_v4().to_string();
    connection
        .execute(
            "INSERT INTO environments (id, collection_id, name, created_at, updated_at)
             VALUES (?, ?, 'Environment', '', '')",
            params![id, collection_id],
        )
        .unwrap();
    id
}

/// A folder placed after its existing siblings.
pub fn folder(connection: &mut Connection, collection_id: &str, parent_id: Option<&str>) -> String {
    add_folder(
        connection,
        &CreateFolderInput {
            collection_id: collection_id.to_string(),
            parent_id: parent_id.map(ToString::to_string),
            position: i64::MAX,
            name: "Folder".to_string(),
        },
    )
    .unwrap()
}

/// A blank `GET` placed after its existing siblings.
pub fn request(
    connection: &mut Connection,
    collection_id: &str,
    parent_id: Option<&str>,
) -> CreateRequestResult {
    add_request(
        connection,
        &CreateRequestInput {
            collection_id: collection_id.to_string(),
            parent_id: parent_id.map(ToString::to_string),
            position: i64::MAX,
            name: "Request".to_string(),
        },
    )
    .unwrap()
}

/// Gives a folder auth that the requests under it inherit. `auth` is the
/// stored `AuthConfig` JSON.
pub fn folder_auth(connection: &Connection, node_id: &str, auth: Value) {
    connection
        .execute(
            "UPDATE collection_nodes SET auth_json = ? WHERE id = ?",
            params![auth.to_string(), node_id],
        )
        .unwrap();
}

pub fn variable(connection: &Connection, target: &VariableTarget, key: &str, value: &str) {
    upsert_variable(
        connection,
        &NewVariable {
            target,
            key,
            value,
            enabled: true,
            sensitive: false,
        },
        "",
    )
    .unwrap();
}

/// A Postman file shaped like the perf harness's `largeCollection` fixture
/// (src/app/perf/fixtures.ts): 40 folders of 50 requests, half of them POSTs
/// with a small JSON body. Keep the two in step, so the budgets here hold for
/// what the harness measures.
pub fn large_postman_collection() -> String {
    let base = "http://127.0.0.1:8787";
    let folder = |folder: usize| {
        let requests = (0..50)
            .map(|index| {
                let mut request = json!({
                    "method": if index % 2 == 1 { "POST" } else { "GET" },
                    "header": [
                        { "key": "Accept", "value": "application/json" },
                        { "key": "X-Trace", "value": format!("{folder}-{index}") }
                    ],
                    "url": format!("{base}/folders/{folder}/items/{index}?page=1&limit=50"),
                });
                if index % 2 == 1 {
                    request["body"] = json!({
                        "mode": "raw",
                        "raw": json!({ "folder": folder, "index": index, "note": "x".repeat(200) })
                            .to_string(),
                        "options": { "raw": { "language": "json" } }
                    });
                }
                json!({ "name": format!("Request {folder}.{index}"), "request": request })
            })
            .collect::<Vec<_>>();
        json!({ "name": format!("Folder {folder}"), "item": requests })
    };
    json!({
        "info": { "name": "Perf harness" },
        "item": (0..40).map(folder).collect::<Vec<_>>()
    })
    .to_string()
}

/// The state commands run against, on a database in its own temp directory
/// that is removed when the test ends.
pub struct TestApp {
    pub state: AppState,
    directory: PathBuf,
}

impl TestApp {
    pub fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("conductor-test-{}", Uuid::new_v4()));
        let state = AppState {
            database: Database::open(&directory).unwrap(),
            http_client: crate::build_http_client().unwrap(),
            binary_bodies: BinaryBodies::default(),
        };
        Self { state, directory }
    }

    /// Runs the builders above against this app's database.
    pub fn seed<T>(&self, build: impl FnOnce(&mut Connection) -> T) -> T {
        self.state
            .database
            .with_connection(|connection| Ok(build(connection)))
            .unwrap()
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
