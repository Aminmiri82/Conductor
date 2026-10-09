//! Builders for tests that need rows. Each fills in what the test does not
//! care about, writes through the same functions the commands use, and returns
//! the new id. Start from `storage::test_connection()`, or `TestApp` for tests
//! that go through `AppState` (sending).

use std::path::PathBuf;

use rusqlite::{params, Connection};
use serde_json::Value;
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

pub fn environment(connection: &Connection) -> String {
    let id = Uuid::new_v4().to_string();
    connection
        .execute(
            "INSERT INTO environments (id, name, created_at, updated_at)
             VALUES (?, 'Environment', '', '')",
            params![id],
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
