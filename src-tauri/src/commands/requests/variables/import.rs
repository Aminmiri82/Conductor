use chrono::Utc;
use rusqlite::params;
use serde_json::Value;
use tauri::State;
use uuid::Uuid;

use crate::commands::{AppError, AppResult};
use crate::storage::{Secrets, StorageError};
use crate::AppState;

use super::crud::clean_environment_name;

#[tauri::command]
pub fn import_postman_environment(
    postman_json: String,
    file_name: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<String> {
    let imported = parse_imported_environment(&postman_json, file_name.as_deref())
        .map_err(AppError::from)?;
    let environment_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let secrets = state.database.secrets().clone();

    state.database.with_connection(|connection| {
        let tx = connection.transaction()?;
        tx.execute(
            "INSERT INTO environments (id, name, source, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)",
            params![environment_id, imported.name, imported.source, now, now],
        )?;

        for variable in imported.variables {
            insert_imported_environment_variable(&tx, &secrets, &environment_id, variable, &now)?;
        }

        tx.commit()?;
        Ok(environment_id)
    })
    .map_err(AppError::from)
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
                        sensitive: variable_type.as_deref() == Some("secret")
                            || super::looks_sensitive_variable_key(key),
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
        // Dotenv files are almost always secrets; default to sensitive so the
        // values are encrypted at rest.
        sensitive: true,
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
    secrets: &Secrets,
    environment_id: &str,
    variable: ImportedEnvironmentVariable,
    now: &str,
) -> Result<(), StorageError> {
    let (current, initial) = if variable.sensitive {
        let encrypted = secrets.encrypt(&variable.value)?;
        (encrypted.clone(), encrypted)
    } else {
        (variable.value.clone(), variable.value)
    };
    tx.execute(
        "INSERT INTO variables
         (scope, collection_id, environment_id, key, initial_value, current_value,
          enabled, sensitive, variable_type, created_at, updated_at)
         VALUES ('environment', NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            environment_id,
            variable.key,
            initial,
            current,
            variable.enabled as i64,
            variable.sensitive as i64,
            variable.variable_type,
            now,
            now
        ],
    )?;
    Ok(())
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}
