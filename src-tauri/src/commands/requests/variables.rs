use std::collections::HashMap;

use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use tauri::State;

use crate::commands::models::{RequestDetail, VariableEntry};
use crate::{storage::StorageError, AppState};

#[tauri::command]
pub fn list_variables(
    scope_kind: String,
    scope_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<VariableEntry>, String> {
    state
        .database
        .with_read_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT scope_kind, scope_id, key, value, enabled, sensitive
                 FROM variables
                 WHERE scope_kind = ? AND scope_id = ?
                 ORDER BY key",
            )?;
            let rows = statement.query_map(params![scope_kind, scope_id], |row| {
                Ok(VariableEntry {
                    scope_kind: row.get(0)?,
                    scope_id: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                    enabled: row.get::<_, i64>(4)? != 0,
                    sensitive: row.get::<_, i64>(5)? != 0,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(StorageError::from)
        })
        .map_err(|error| error.to_string())
}
#[tauri::command]
pub fn save_variables(
    scope_kind: String,
    scope_id: String,
    variables: Vec<VariableEntry>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            tx.execute(
                "DELETE FROM variables WHERE scope_kind = ? AND scope_id = ?",
                params![scope_kind, scope_id],
            )?;
            for variable in variables
                .into_iter()
                .filter(|variable| !variable.key.is_empty())
            {
                tx.execute(
                    "INSERT INTO variables
                     (scope_kind, scope_id, key, value, enabled, sensitive, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    params![
                        scope_kind,
                        scope_id,
                        variable.key,
                        variable.value,
                        variable.enabled as i64,
                        variable.sensitive as i64,
                        now,
                        now
                    ],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .map_err(|error| error.to_string())
}
pub(super) fn load_variable_context(
    connection: &rusqlite::Connection,
    request: &RequestDetail,
) -> Result<HashMap<String, String>, StorageError> {
    let mut variables = HashMap::new();
    load_variables_for_scope(connection, "global", "global", &mut variables)?;
    load_variables_for_scope(
        connection,
        "collection",
        &request.collection_id,
        &mut variables,
    )?;

    let folder_ids = ancestor_folder_ids(connection, &request.id)?;
    for folder_id in folder_ids {
        load_variables_for_scope(connection, "folder", &folder_id, &mut variables)?;
    }

    load_variables_for_scope(connection, "request", &request.id, &mut variables)?;
    Ok(variables)
}
fn load_variables_for_scope(
    connection: &rusqlite::Connection,
    scope_kind: &str,
    scope_id: &str,
    variables: &mut HashMap<String, String>,
) -> Result<(), StorageError> {
    let mut statement = connection.prepare(
        "SELECT key, value FROM variables
         WHERE scope_kind = ? AND scope_id = ? AND enabled = 1
         ORDER BY key",
    )?;
    let rows = statement.query_map(params![scope_kind, scope_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (key, value) = row?;
        variables.insert(key, value);
    }
    Ok(())
}
fn ancestor_folder_ids(
    connection: &rusqlite::Connection,
    request_id: &str,
) -> Result<Vec<String>, StorageError> {
    let mut parent_id: Option<String> = connection
        .query_row(
            "SELECT parent_id FROM collection_nodes WHERE request_id = ?",
            params![request_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    let mut ids = Vec::new();

    while let Some(id) = parent_id {
        ids.push(id.clone());
        parent_id = connection
            .query_row(
                "SELECT parent_id FROM collection_nodes WHERE id = ?",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
    }

    ids.reverse();
    Ok(ids)
}
