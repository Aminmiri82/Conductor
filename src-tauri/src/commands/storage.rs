use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::{storage::DatabaseStatus, AppState};

/// JSON blob whose shape is owned by the frontend; Rust only stores it.
// Typed as `unknown` because specta overflows the stack exporting `serde_json::Value`.
#[derive(Debug, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
pub struct FrontendJson(#[specta(type = specta_typescript::Unknown)] pub Value);

#[tauri::command]
#[specta::specta]
pub fn database_status(state: State<'_, AppState>) -> Result<DatabaseStatus, String> {
    state.database.status().map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn get_workspace_state(
    key: String,
    state: State<'_, AppState>,
) -> Result<Option<FrontendJson>, String> {
    state
        .database
        .with_read_connection(|connection| {
            let value_json = connection
                .query_row(
                    "SELECT value_json FROM workspace_state WHERE key = ?",
                    params![key],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;

            value_json
                .map(|value| {
                    serde_json::from_str::<Value>(&value)
                        .map(FrontendJson)
                        .map_err(Into::into)
                })
                .transpose()
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn set_workspace_state(
    key: String,
    value: FrontendJson,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    state
        .database
        .with_connection(|connection| {
            connection.execute(
                "INSERT INTO workspace_state (key, value_json, updated_at)
                 VALUES (?, ?, ?)
                 ON CONFLICT(key) DO UPDATE SET
                    value_json = excluded.value_json,
                    updated_at = excluded.updated_at",
                params![key, value.0.to_string(), now],
            )?;
            Ok(())
        })
        .map_err(|error| error.to_string())
}
