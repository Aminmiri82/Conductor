use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde_json::Value;
use tauri::State;

use crate::{
    storage::{
        reset_learned_rowid_stats, set_learned_rowid_enabled as set_sqlite_learned_rowid_enabled,
        DatabaseStatus, LearnedRowidBenchmark,
    },
    AppState,
};

#[tauri::command]
pub fn database_status(state: State<'_, AppState>) -> Result<DatabaseStatus, String> {
    state.database.status().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_learned_rowid_enabled(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<DatabaseStatus, String> {
    set_sqlite_learned_rowid_enabled(enabled);
    state.database.status().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn reset_learned_rowid_counters(state: State<'_, AppState>) -> Result<DatabaseStatus, String> {
    reset_learned_rowid_stats();
    state.database.status().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn run_learned_rowid_benchmark(
    state: State<'_, AppState>,
) -> Result<LearnedRowidBenchmark, String> {
    state
        .database
        .benchmark_learned_rowid()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn rebuild_learned_rowid_models(state: State<'_, AppState>) -> Result<DatabaseStatus, String> {
    state
        .database
        .rebuild_learned_rowid_models()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_workspace_state(
    key: String,
    state: State<'_, AppState>,
) -> Result<Option<Value>, String> {
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
                .map(|value| serde_json::from_str::<Value>(&value).map_err(Into::into))
                .transpose()
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_workspace_state(
    key: String,
    value: Value,
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
                params![key, value.to_string(), now],
            )?;
            Ok(())
        })
        .map_err(|error| error.to_string())
}
