use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde_json::Value;
use tauri::State;

use crate::commands::{AppError, AppResult};
use crate::{storage::DatabaseStatus, AppState};

#[tauri::command]
pub fn database_status(state: State<'_, AppState>) -> AppResult<DatabaseStatus> {
    state.database.status().map_err(AppError::from)
}

#[tauri::command]
pub fn get_workspace_state(
    key: String,
    state: State<'_, AppState>,
) -> AppResult<Option<Value>> {
    Ok(state.database.with_read_connection(|connection| {
        let value_json = connection
            .query_row(
                "SELECT value_json FROM workspace_state WHERE key = ?",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        value_json
            .map(|value| serde_json::from_str::<Value>(&value).map_err(Into::into))
            .transpose()
    })?)
}

#[tauri::command]
pub fn set_workspace_state(
    key: String,
    value: Value,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    state.database.with_connection(|connection| {
        connection.execute(
            "INSERT INTO workspace_state (key, value_json, updated_at)
             VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET
                value_json = excluded.value_json,
                updated_at = excluded.updated_at",
            params![key, value.to_string(), now],
        )?;
        Ok(())
    })
    .map_err(AppError::from)
}
