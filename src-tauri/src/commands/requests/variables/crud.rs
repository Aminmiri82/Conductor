use chrono::Utc;
use rusqlite::params;
use tauri::State;

use crate::commands::models::VariableEntry;
use crate::commands::{AppError, AppResult};
use crate::storage::{Secrets, StorageError};
use crate::AppState;

#[tauri::command]
pub fn list_variables(
    scope: String,
    collection_id: Option<String>,
    environment_id: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<Vec<VariableEntry>> {
    validate_scope(&scope, collection_id.as_deref(), environment_id.as_deref())
        .map_err(AppError::from)?;

    Ok(state.database.with_read_connection(|connection| {
        let mut statement = connection.prepare(
            "SELECT scope, collection_id, environment_id, key, current_value,
                    initial_value, enabled, sensitive, variable_type
             FROM variables
             WHERE scope = ?
               AND (
                   (scope = 'global' AND collection_id IS NULL AND environment_id IS NULL)
                   OR collection_id = ?
                   OR environment_id = ?
               )
             ORDER BY key",
        )?;
        let rows = statement
            .query_map(params![scope, collection_id, environment_id], |row| {
                Ok(VariableRow {
                    scope: row.get(0)?,
                    collection_id: row.get(1)?,
                    environment_id: row.get(2)?,
                    key: row.get(3)?,
                    value: row.get(4)?,
                    initial_value: row.get(5)?,
                    enabled: row.get::<_, i64>(6)? != 0,
                    sensitive: row.get::<_, i64>(7)? != 0,
                    variable_type: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;

        rows.into_iter()
            .map(|row| decrypt_variable(state.database.secrets(), row))
            .collect::<Result<Vec<_>, StorageError>>()
    })?)
}

#[tauri::command]
pub fn save_variables(
    scope: String,
    collection_id: Option<String>,
    environment_id: Option<String>,
    variables: Vec<VariableEntry>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    validate_scope(&scope, collection_id.as_deref(), environment_id.as_deref())
        .map_err(AppError::from)?;

    let now = Utc::now().to_rfc3339();
    let secrets = state.database.secrets().clone();
    state.database.with_connection(|connection| {
        let tx = connection.transaction()?;
        tx.execute(
            "DELETE FROM variables
             WHERE scope = ?
               AND (
                   (scope = 'global' AND collection_id IS NULL AND environment_id IS NULL)
                   OR collection_id = ?
                   OR environment_id = ?
               )",
            params![scope, collection_id, environment_id],
        )?;
        for variable in variables
            .into_iter()
            .filter(|variable| !variable.key.trim().is_empty())
        {
            let (current, initial) = if variable.sensitive {
                (
                    secrets.encrypt(&variable.value)?,
                    match variable.initial_value {
                        Some(v) => Some(secrets.encrypt(&v)?),
                        None => None,
                    },
                )
            } else {
                (variable.value, variable.initial_value)
            };
            tx.execute(
                "INSERT INTO variables
                 (scope, collection_id, environment_id, key, initial_value, current_value,
                  enabled, sensitive, variable_type, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    scope,
                    collection_id,
                    environment_id,
                    variable.key.trim(),
                    initial,
                    current,
                    variable.enabled as i64,
                    variable.sensitive as i64,
                    variable.variable_type,
                    now,
                    now
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
    .map_err(AppError::from)
}

pub(super) struct VariableRow {
    pub scope: String,
    pub collection_id: Option<String>,
    pub environment_id: Option<String>,
    pub key: String,
    pub value: String,
    pub initial_value: Option<String>,
    pub enabled: bool,
    pub sensitive: bool,
    pub variable_type: Option<String>,
}

fn decrypt_variable(secrets: &Secrets, row: VariableRow) -> Result<VariableEntry, StorageError> {
    let value = if row.sensitive {
        secrets.decrypt(&row.value)?
    } else {
        row.value
    };
    let initial_value = match row.initial_value {
        Some(v) if row.sensitive => Some(secrets.decrypt(&v)?),
        other => other,
    };
    Ok(VariableEntry {
        scope: row.scope,
        collection_id: row.collection_id,
        environment_id: row.environment_id,
        key: row.key,
        value,
        initial_value,
        enabled: row.enabled,
        sensitive: row.sensitive,
        variable_type: row.variable_type,
    })
}

pub(in crate::commands::requests) fn validate_scope(
    scope: &str,
    collection_id: Option<&str>,
    environment_id: Option<&str>,
) -> Result<(), String> {
    let collection_id = collection_id.filter(|id| !id.is_empty());
    let environment_id = environment_id.filter(|id| !id.is_empty());
    match (scope, collection_id, environment_id) {
        ("global", None, None) => Ok(()),
        ("collection", Some(_), None) => Ok(()),
        ("environment", None, Some(_)) => Ok(()),
        ("global", _, _) => Err("global variables cannot have a target id".to_string()),
        ("collection", None, _) => Err("collection variables require a collection id".to_string()),
        ("collection", _, Some(_)) => {
            Err("collection variables cannot have an environment id".to_string())
        }
        ("environment", _, None) => {
            Err("environment variables require an environment id".to_string())
        }
        ("environment", Some(_), _) => {
            Err("environment variables cannot have a collection id".to_string())
        }
        _ => Err("unsupported variable scope".to_string()),
    }
}

pub(super) fn clean_environment_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "New environment".to_string()
    } else {
        trimmed.to_string()
    }
}
