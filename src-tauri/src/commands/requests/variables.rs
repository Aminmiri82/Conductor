use std::collections::HashMap;

use chrono::Utc;
use rusqlite::params;
use serde_json::Value;
use tauri::State;
use uuid::Uuid;

use crate::commands::models::{
    CreateEnvironmentInput, EntityId, EnvironmentSummary, RenameEnvironmentInput, RequestDetail,
    VariableEntry,
};
use crate::{storage::StorageError, AppState};

#[derive(Debug, Clone)]
pub(super) struct VariableContext {
    values: HashMap<String, String>,
}

impl VariableContext {
    pub(super) fn get(&self, key: &str) -> Option<String> {
        self.values
            .get(key)
            .cloned()
            .or_else(|| dynamic_variable(key))
    }
}

#[tauri::command]
pub fn list_environments(state: State<'_, AppState>) -> Result<Vec<EnvironmentSummary>, String> {
    state
        .database
        .with_read_connection(|connection| {
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
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_environment(
    input: CreateEnvironmentInput,
    state: State<'_, AppState>,
) -> Result<EntityId, String> {
    let name = clean_environment_name(&input.name);
    let now = Utc::now().to_rfc3339();

    state
        .database
        .with_connection(|connection| {
            connection.execute(
                "INSERT INTO environments (name, source, created_at, updated_at)
                 VALUES (?, 'manual', ?, ?)",
                params![name, now, now],
            )?;
            let environment_id = connection.last_insert_rowid();
            Ok(environment_id)
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn rename_environment(
    input: RenameEnvironmentInput,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let name = clean_environment_name(&input.name);
    let now = Utc::now().to_rfc3339();
    state
        .database
        .with_connection(|connection| {
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
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_environment(
    environment_id: EntityId,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .database
        .with_connection(|connection| {
            connection.execute(
                "DELETE FROM environments WHERE id = ?",
                params![environment_id],
            )?;
            Ok(())
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn import_postman_environment(
    postman_json: String,
    file_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<EntityId, String> {
    let imported = parse_imported_environment(&postman_json, file_name.as_deref())?;
    let now = Utc::now().to_rfc3339();

    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            tx.execute(
                "INSERT INTO environments (name, source, created_at, updated_at)
                 VALUES (?, ?, ?, ?)",
                params![imported.name, imported.source, now, now],
            )?;
            let environment_id = tx.last_insert_rowid();

            for variable in imported.variables {
                insert_imported_environment_variable(&tx, environment_id, variable, &now)?;
            }

            tx.commit()?;
            Ok(environment_id)
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_variables(
    scope: String,
    collection_id: Option<EntityId>,
    environment_id: Option<EntityId>,
    state: State<'_, AppState>,
) -> Result<Vec<VariableEntry>, String> {
    validate_scope(&scope, collection_id, environment_id)?;

    state
        .database
        .with_read_connection(|connection| {
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
            let rows =
                statement.query_map(params![scope, collection_id, environment_id], |row| {
                    Ok(VariableEntry {
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
                })?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(StorageError::from)
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_variables(
    scope: String,
    collection_id: Option<EntityId>,
    environment_id: Option<EntityId>,
    variables: Vec<VariableEntry>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    validate_scope(&scope, collection_id, environment_id)?;

    let now = Utc::now().to_rfc3339();
    state
        .database
        .with_connection(|connection| {
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
                        variable.initial_value,
                        variable.value,
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
        .map_err(|error| error.to_string())
}

pub(super) fn load_variable_context(
    connection: &rusqlite::Connection,
    request: &RequestDetail,
    environment_id: Option<EntityId>,
) -> Result<VariableContext, StorageError> {
    let mut values = HashMap::new();
    let mut statement = connection.prepare(
        "SELECT key, current_value
         FROM variables
         WHERE enabled = 1
           AND (
               scope = 'global'
               OR (scope = 'collection' AND collection_id = ?)
               OR (scope = 'environment' AND environment_id = ?)
           )
         ORDER BY CASE scope
             WHEN 'global' THEN 0
             WHEN 'collection' THEN 1
             WHEN 'environment' THEN 2
             ELSE 3
         END, key",
    )?;
    let rows = statement.query_map(params![request.collection_id, environment_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (key, value) = row?;
        values.insert(key, value);
    }
    Ok(VariableContext { values })
}

pub(super) fn save_script_variable(
    connection: &rusqlite::Connection,
    scope: &str,
    collection_id: Option<EntityId>,
    environment_id: Option<EntityId>,
    key: &str,
    value: &str,
    now: &str,
) -> Result<(), StorageError> {
    validate_scope(scope, collection_id, environment_id).map_err(StorageError::InvalidInput)?;
    connection.execute(
        "INSERT INTO variables
         (scope, collection_id, environment_id, key, initial_value, current_value,
          enabled, sensitive, variable_type, created_at, updated_at)
         VALUES (?, ?, ?, ?, NULL, ?, 1, 0, NULL, ?, ?)
         ON CONFLICT DO UPDATE SET
            current_value = excluded.current_value,
            enabled = 1,
            updated_at = excluded.updated_at",
        params![scope, collection_id, environment_id, key, value, now, now],
    )?;
    Ok(())
}

#[derive(Debug)]
struct ImportedEnvironment {
    name: String,
    source: String,
    variables: Vec<ImportedEnvironmentVariable>,
}

#[derive(Debug)]
struct ImportedEnvironmentVariable {
    key: String,
    value: String,
    enabled: bool,
    sensitive: bool,
    variable_type: Option<String>,
}

fn parse_imported_environment(
    contents: &str,
    file_name: Option<&str>,
) -> Result<ImportedEnvironment, String> {
    match serde_json::from_str::<Value>(contents) {
        Ok(environment) => Ok(parse_postman_environment(environment)),
        Err(_) => parse_dotenv_environment(contents, file_name),
    }
}

fn parse_postman_environment(environment: Value) -> ImportedEnvironment {
    let name = environment
        .get("name")
        .and_then(Value::as_str)
        .map(clean_environment_name)
        .unwrap_or_else(|| "Imported environment".to_string());
    let variables = environment
        .get("values")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|variable| {
                    let key = variable.get("key").and_then(Value::as_str)?;
                    let value = variable
                        .get("value")
                        .map(value_to_string)
                        .unwrap_or_default();
                    let variable_type = variable
                        .get("type")
                        .and_then(Value::as_str)
                        .map(ToString::to_string);
                    let enabled = variable
                        .get("enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or_else(|| {
                            !variable
                                .get("disabled")
                                .and_then(Value::as_bool)
                                .unwrap_or(false)
                        });
                    Some(ImportedEnvironmentVariable {
                        key: key.to_string(),
                        value,
                        enabled,
                        sensitive: variable_type.as_deref() == Some("secret"),
                        variable_type,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    ImportedEnvironment {
        name,
        source: "postman".to_string(),
        variables,
    }
}

fn parse_dotenv_environment(
    contents: &str,
    file_name: Option<&str>,
) -> Result<ImportedEnvironment, String> {
    let variables = contents
        .lines()
        .filter_map(parse_dotenv_line)
        .collect::<Vec<_>>();
    if variables.is_empty() {
        return Err(
            "environment import must be Postman JSON or .env key=value content".to_string(),
        );
    }

    Ok(ImportedEnvironment {
        name: file_name
            .and_then(|name| name.rsplit_once('.').map(|(stem, _)| stem).or(Some(name)))
            .map(clean_environment_name)
            .unwrap_or_else(|| "Imported environment".to_string()),
        source: "dotenv".to_string(),
        variables,
    })
}

fn parse_dotenv_line(line: &str) -> Option<ImportedEnvironmentVariable> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let trimmed = trimmed.strip_prefix("export ").unwrap_or(trimmed);
    let (key, value) = trimmed.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some(ImportedEnvironmentVariable {
        key: key.to_string(),
        value: unquote_dotenv_value(value.trim()).to_string(),
        enabled: true,
        sensitive: false,
        variable_type: None,
    })
}

fn unquote_dotenv_value(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn insert_imported_environment_variable(
    tx: &rusqlite::Transaction<'_>,
    environment_id: EntityId,
    variable: ImportedEnvironmentVariable,
    now: &str,
) -> Result<(), StorageError> {
    tx.execute(
        "INSERT INTO variables
         (scope, collection_id, environment_id, key, initial_value, current_value,
          enabled, sensitive, variable_type, created_at, updated_at)
         VALUES ('environment', NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            environment_id,
            variable.key,
            variable.value,
            variable.value,
            variable.enabled as i64,
            variable.sensitive as i64,
            variable.variable_type,
            now,
            now
        ],
    )?;
    Ok(())
}

fn validate_scope(
    scope: &str,
    collection_id: Option<EntityId>,
    environment_id: Option<EntityId>,
) -> Result<(), String> {
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

fn clean_environment_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "New environment".to_string()
    } else {
        trimmed.to_string()
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

fn dynamic_variable(key: &str) -> Option<String> {
    match key {
        "$guid" | "$randomUUID" => Some(Uuid::new_v4().to_string()),
        "$timestamp" => Some(Utc::now().timestamp().to_string()),
        "$isoTimestamp" => Some(Utc::now().to_rfc3339()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn variable_context_applies_environment_over_collection_over_global() {
        let connection = test_connection();
        let request = RequestDetail {
            id: 1,
            collection_id: 2,
            name: "Request".to_string(),
            method: "GET".to_string(),
            url: "{{host}}".to_string(),
            headers: Vec::new(),
            query: Vec::new(),
            path_params: Vec::new(),
            auth: None,
            inherited_auth: None,
            effective_auth: None,
            body: None,
            pre_request_script: None,
            test_script: None,
            updated_at: String::new(),
        };

        insert_test_variable(&connection, "global", None, None, "host", "global");
        insert_test_variable(
            &connection,
            "collection",
            Some(2),
            None,
            "host",
            "collection",
        );
        insert_test_variable(
            &connection,
            "environment",
            None,
            Some(3),
            "host",
            "environment",
        );

        let context = load_variable_context(&connection, &request, Some(3)).unwrap();

        assert_eq!(context.get("host").as_deref(), Some("environment"));
    }

    fn test_connection() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE variables (
                    scope TEXT NOT NULL,
                    collection_id INTEGER,
                    environment_id INTEGER,
                    key TEXT NOT NULL,
                    initial_value TEXT,
                    current_value TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    sensitive INTEGER NOT NULL DEFAULT 0,
                    variable_type TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );",
            )
            .unwrap();
        connection
    }

    fn insert_test_variable(
        connection: &Connection,
        scope: &str,
        collection_id: Option<EntityId>,
        environment_id: Option<EntityId>,
        key: &str,
        value: &str,
    ) {
        connection
            .execute(
                "INSERT INTO variables
                 (scope, collection_id, environment_id, key, current_value, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, '', '')",
                params![scope, collection_id, environment_id, key, value],
            )
            .unwrap();
    }
}
