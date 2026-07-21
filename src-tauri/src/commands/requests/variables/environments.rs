use chrono::Utc;
use rusqlite::params;
use tauri::State;
use uuid::Uuid;

use crate::commands::models::{CreateEnvironmentInput, EnvironmentSummary, RenameEnvironmentInput};
use crate::commands::{AppError, AppResult};
use crate::storage::StorageError;
use crate::AppState;

use super::crud::clean_environment_name;

#[tauri::command]
pub fn list_environments(state: State<'_, AppState>) -> AppResult<Vec<EnvironmentSummary>> {
    Ok(state.database.with_read_connection(|connection| {
        let mut statement = connection
            .prepare("SELECT id, name, updated_at FROM environments ORDER BY name")?;
        let rows = statement.query_map([], |row| {
            Ok(EnvironmentSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                updated_at: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    })?)
}

#[tauri::command]
pub fn create_environment(
    input: CreateEnvironmentInput,
    state: State<'_, AppState>,
) -> AppResult<String> {
    let name = clean_environment_name(&input.name);
    let environment_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    state.database.with_connection(|connection| {
        connection.execute(
            "INSERT INTO environments (id, name, source, created_at, updated_at)
             VALUES (?, ?, 'manual', ?, ?)",
            params![environment_id, name, now, now],
        )?;
        Ok(environment_id)
    })
    .map_err(AppError::from)
}

#[tauri::command]
pub fn rename_environment(
    input: RenameEnvironmentInput,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let name = clean_environment_name(&input.name);
    let now = Utc::now().to_rfc3339();
    state.database.with_connection(|connection| {
        let changed = connection.execute(
            "UPDATE environments SET name = ?, updated_at = ? WHERE id = ?",
            params![name, now, input.environment_id],
        )?;
        if changed == 0 {
            return Err(StorageError::InvalidInput(
                "environment not found".to_string(),
            ));
        }
        Ok(())
    })
    .map_err(AppError::from)
}

#[tauri::command]
pub fn delete_environment(
    environment_id: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    state.database.with_connection(|connection| {
        connection.execute(
            "DELETE FROM environments WHERE id = ?",
            params![environment_id],
        )?;
        Ok(())
    })
    .map_err(AppError::from)
}
