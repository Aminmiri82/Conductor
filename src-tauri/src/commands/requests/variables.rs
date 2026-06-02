use std::collections::HashMap;

use chrono::Utc;
use rusqlite::params;
use tauri::State;

use crate::commands::models::{RequestDetail, VariableEntry};
use crate::{storage::StorageError, AppState};

#[tauri::command]
pub fn list_variables(
    scope: String,
    collection_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<VariableEntry>, String> {
    validate_scope(&scope, collection_id.as_deref())?;

    state
        .database
        .with_read_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT scope, collection_id, key, value, enabled, sensitive
                 FROM variables
                 WHERE scope = ?
                   AND (
                       (scope = 'global' AND collection_id IS NULL)
                       OR collection_id = ?
                   )
                 ORDER BY key",
            )?;
            let rows = statement.query_map(params![scope, collection_id], |row| {
                Ok(VariableEntry {
                    scope: row.get(0)?,
                    collection_id: row.get(1)?,
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
    scope: String,
    collection_id: Option<String>,
    variables: Vec<VariableEntry>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    validate_scope(&scope, collection_id.as_deref())?;

    let now = Utc::now().to_rfc3339();
    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            tx.execute(
                "DELETE FROM variables
                 WHERE scope = ?
                   AND (
                       (scope = 'global' AND collection_id IS NULL)
                       OR collection_id = ?
                   )",
                params![scope, collection_id],
            )?;
            for variable in variables
                .into_iter()
                .filter(|variable| !variable.key.is_empty())
            {
                tx.execute(
                    "INSERT INTO variables
                     (scope, collection_id, key, value, enabled, sensitive, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    params![
                        scope,
                        collection_id,
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
    let mut statement = connection.prepare(
        "SELECT key, value
         FROM variables
         WHERE enabled = 1
           AND (
               scope = 'global'
               OR (scope = 'collection' AND collection_id = ?)
           )
         ORDER BY CASE scope WHEN 'global' THEN 0 ELSE 1 END, key",
    )?;
    let rows = statement.query_map(params![request.collection_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (key, value) = row?;
        variables.insert(key, value);
    }
    Ok(variables)
}
fn validate_scope(scope: &str, collection_id: Option<&str>) -> Result<(), String> {
    match (scope, collection_id) {
        ("global", None) => Ok(()),
        ("collection", Some(collection_id)) if !collection_id.is_empty() => Ok(()),
        ("global", Some(_)) => Err("global variables cannot have a collection id".to_string()),
        ("collection", _) => Err("collection variables require a collection id".to_string()),
        _ => Err("unsupported variable scope".to_string()),
    }
}
