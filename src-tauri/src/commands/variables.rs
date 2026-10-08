use std::collections::{HashMap, HashSet};

use chrono::Utc;
use rusqlite::params;
use serde::Deserialize;
use tauri::State;
use uuid::Uuid;

use super::{
    error::CommandError,
    models::{
        CreateEnvironmentInput, EnvironmentSummary, RenameEnvironmentInput, RequestDetail,
        VariableChange, VariableEntry, VariableTarget,
    },
    postman::{flag, flag_or_false, lenient_list, stringified, text},
};
use crate::storage::{upsert_variable, NewVariable, StorageError};
use crate::AppState;

#[derive(Debug, Default)]
pub(crate) struct VariableContext {
    values: HashMap<String, StoredValue>,
}

#[derive(Debug)]
struct StoredValue {
    value: String,
    sensitive: bool,
}

impl VariableContext {
    pub(crate) fn get(&self, key: &str) -> Option<String> {
        self.values
            .get(key)
            .map(|stored| stored.value.clone())
            .or_else(|| dynamic_variable(key))
    }

    #[cfg(test)]
    pub(crate) fn with_values(values: &[(&str, &str, bool)]) -> Self {
        Self {
            values: values
                .iter()
                .map(|&(key, value, sensitive)| {
                    (
                        key.to_string(),
                        StoredValue {
                            value: value.to_string(),
                            sensitive,
                        },
                    )
                })
                .collect(),
        }
    }

    /// Whether the value is one the user marked secret. Dynamic variables
    /// are not.
    pub(crate) fn is_sensitive(&self, key: &str) -> bool {
        self.values.get(key).is_some_and(|stored| stored.sensitive)
    }
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_environments(
    state: State<'_, AppState>,
) -> Result<Vec<EnvironmentSummary>, CommandError> {
    Ok(state.database.with_read_connection(|connection| {
        let mut statement =
            connection.prepare("SELECT id, name FROM environments ORDER BY name")?;
        let rows = statement.query_map([], |row| {
            Ok(EnvironmentSummary {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    })?)
}

#[tauri::command(async)]
#[specta::specta]
pub fn create_environment(
    input: CreateEnvironmentInput,
    state: State<'_, AppState>,
) -> Result<String, CommandError> {
    let name = clean_environment_name(&input.name);
    let environment_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    Ok(state.database.with_connection(|connection| {
        insert_environment(connection, &environment_id, &name, &now)?;
        Ok(environment_id)
    })?)
}

fn insert_environment(
    connection: &rusqlite::Connection,
    id: &str,
    name: &str,
    now: &str,
) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO environments (id, name, created_at, updated_at)
         VALUES (?, ?, ?, ?)",
        params![id, name, now, now],
    )?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub fn rename_environment(
    input: RenameEnvironmentInput,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    let name = clean_environment_name(&input.name);
    let now = Utc::now().to_rfc3339();
    Ok(state.database.with_connection(|connection| {
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
    })?)
}

#[tauri::command(async)]
#[specta::specta]
pub fn delete_environment(
    environment_id: String,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    Ok(state.database.with_connection(|connection| {
        connection.execute(
            "DELETE FROM environments WHERE id = ?",
            params![environment_id],
        )?;
        Ok(())
    })?)
}

#[tauri::command(async)]
#[specta::specta]
pub fn import_postman_environment(
    postman_json: String,
    file_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, CommandError> {
    let imported = parse_imported_environment(&postman_json, file_name.as_deref())?;
    let environment_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    state.database.with_connection(|connection| {
        store_imported_environment(connection, &environment_id, &imported, &now)
    })?;
    Ok(environment_id)
}

fn store_imported_environment(
    connection: &mut rusqlite::Connection,
    environment_id: &str,
    imported: &ImportedEnvironment,
    now: &str,
) -> Result<(), StorageError> {
    let tx = connection.transaction()?;
    insert_environment(&tx, environment_id, &imported.name, now)?;
    let target = VariableTarget::Environment {
        environment_id: environment_id.to_string(),
    };

    // A key repeated in the file (common in `.env`) keeps its last value, as
    // a shell sourcing the file would.
    for variable in &imported.variables {
        upsert_variable(
            &tx,
            &NewVariable {
                target: &target,
                key: &variable.key,
                value: &variable.value,
                enabled: variable.enabled,
                sensitive: variable.sensitive,
            },
            now,
        )?;
    }

    tx.commit()?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_variables(
    target: VariableTarget,
    state: State<'_, AppState>,
) -> Result<Vec<VariableEntry>, CommandError> {
    Ok(state
        .database
        .with_read_connection(|connection| read_variables(connection, &target))?)
}

fn read_variables(
    connection: &rusqlite::Connection,
    target: &VariableTarget,
) -> Result<Vec<VariableEntry>, StorageError> {
    let (scope, collection_id, environment_id) = target.columns();
    let mut statement = connection.prepare_cached(
        "SELECT key, current_value, enabled, sensitive
         FROM variables
         WHERE scope = ?
           AND (
               (scope = 'global' AND collection_id IS NULL AND environment_id IS NULL)
               OR collection_id = ?
               OR environment_id = ?
           )
         ORDER BY key",
    )?;
    let rows = statement.query_map(params![scope, collection_id, environment_id], |row| {
        Ok(VariableEntry {
            key: row.get(0)?,
            value: row.get(1)?,
            enabled: row.get::<_, i64>(2)? != 0,
            sensitive: row.get::<_, i64>(3)? != 0,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::from)
}

/// The one way editors save variables: every change in one transaction, so a
/// failure leaves nothing half-applied (for example a variable moved between
/// scopes is never deleted from one and missing from the other).
#[tauri::command(async)]
#[specta::specta]
pub fn apply_variable_changes(
    changes: Vec<VariableChange>,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    let now = Utc::now().to_rfc3339();
    Ok(state
        .database
        .with_connection(|connection| apply_changes(connection, &changes, &now))?)
}

/// Changes apply in order; within one change the deletes run first, so a key
/// in both ends up set. Upserted keys are trimmed and blank ones ignored, as
/// the editors treat an empty key cell as no variable. Deletes match the key
/// exactly, so a stored key with stray spaces (from an import) can still be
/// removed.
fn apply_changes(
    connection: &mut rusqlite::Connection,
    changes: &[VariableChange],
    now: &str,
) -> Result<(), StorageError> {
    let tx = connection.transaction()?;
    for change in changes {
        let (scope, collection_id, environment_id) = change.target.columns();
        let mut delete = tx.prepare_cached(
            "DELETE FROM variables
             WHERE scope = ? AND key = ? AND collection_id IS ? AND environment_id IS ?",
        )?;
        for key in &change.deletes {
            delete.execute(params![scope, key, collection_id, environment_id])?;
        }

        let mut seen = HashSet::new();
        for variable in &change.upserts {
            let key = variable.key.trim();
            if key.is_empty() {
                continue;
            }
            if !seen.insert(key) {
                return Err(StorageError::InvalidInput(format!(
                    "variable \"{key}\" appears more than once"
                )));
            }
            upsert_variable(
                &tx,
                &NewVariable {
                    target: &change.target,
                    key,
                    value: &variable.value,
                    enabled: variable.enabled,
                    sensitive: variable.sensitive,
                },
                now,
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub(crate) fn load_variable_context(
    connection: &rusqlite::Connection,
    request: &RequestDetail,
    environment_id: Option<&str>,
) -> Result<VariableContext, StorageError> {
    let mut values = HashMap::new();
    let mut statement = connection.prepare(
        "SELECT key, current_value, sensitive
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
        Ok((
            row.get::<_, String>(0)?,
            StoredValue {
                value: row.get(1)?,
                sensitive: row.get(2)?,
            },
        ))
    })?;
    for row in rows {
        let (key, stored) = row?;
        values.insert(key, stored);
    }
    Ok(VariableContext { values })
}

pub(crate) fn save_script_variable(
    connection: &rusqlite::Connection,
    target: &VariableTarget,
    key: &str,
    value: &str,
    now: &str,
) -> Result<(), StorageError> {
    let (scope, collection_id, environment_id) = target.columns();
    connection.execute(
        "INSERT INTO variables
         (scope, collection_id, environment_id, key, current_value, enabled, sensitive,
          created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, 1, 0, ?, ?)
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
    variables: Vec<ImportedEnvironmentVariable>,
}

#[derive(Debug)]
struct ImportedEnvironmentVariable {
    key: String,
    value: String,
    enabled: bool,
    sensitive: bool,
}

fn parse_imported_environment(
    contents: &str,
    file_name: Option<&str>,
) -> Result<ImportedEnvironment, String> {
    match serde_json::from_str::<PostmanEnvironment>(contents) {
        Ok(environment) => Ok(environment.into()),
        Err(_) => parse_dotenv_environment(contents, file_name),
    }
}

/// A Postman environment export. Unknown fields are ignored and missing ones
/// defaulted, like the collection import.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanEnvironment {
    #[serde(deserialize_with = "text")]
    name: Option<String>,
    #[serde(deserialize_with = "lenient_list")]
    values: Vec<PostmanEnvironmentValue>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanEnvironmentValue {
    #[serde(deserialize_with = "text")]
    key: Option<String>,
    #[serde(deserialize_with = "stringified")]
    value: String,
    #[serde(deserialize_with = "flag")]
    enabled: Option<bool>,
    #[serde(deserialize_with = "flag_or_false")]
    disabled: bool,
    #[serde(rename = "type", deserialize_with = "text")]
    value_type: Option<String>,
}

impl From<PostmanEnvironment> for ImportedEnvironment {
    fn from(environment: PostmanEnvironment) -> Self {
        Self {
            name: environment
                .name
                .as_deref()
                .map(clean_environment_name)
                .unwrap_or_else(|| "Imported environment".to_string()),
            variables: environment
                .values
                .into_iter()
                .filter_map(|variable| {
                    Some(ImportedEnvironmentVariable {
                        key: variable.key?,
                        value: variable.value,
                        enabled: variable.enabled.unwrap_or(!variable.disabled),
                        sensitive: variable.value_type.as_deref() == Some("secret"),
                    })
                })
                .collect(),
        }
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

fn clean_environment_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "New environment".to_string()
    } else {
        trimmed.to_string()
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
    fn env_file_with_a_repeated_key_imports_with_the_last_value() {
        let mut connection = crate::storage::test_connection();
        let imported = parse_imported_environment(
            "API_URL=http://old\nTOKEN=abc\nAPI_URL=http://new\n",
            Some("local.env"),
        )
        .unwrap();

        store_imported_environment(&mut connection, "env", &imported, "now").unwrap();

        let values = connection
            .prepare(
                "SELECT key, current_value FROM variables
                 WHERE environment_id = 'env' ORDER BY key",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            values,
            [
                ("API_URL".to_string(), "http://new".to_string()),
                ("TOKEN".to_string(), "abc".to_string())
            ]
        );
    }

    #[test]
    fn postman_environment_imports_secrets_as_sensitive_and_honours_disabled_rows() {
        let imported = parse_imported_environment(
            r#"{
                "id": "abc",
                "name": " Staging ",
                "_postman_variable_scope": "environment",
                "values": [
                    { "key": "token", "value": "t0k", "type": "secret", "enabled": true },
                    { "key": "port", "value": 8080, "type": "default" },
                    { "key": "off", "value": "x", "enabled": false },
                    { "key": "legacy", "value": "y", "disabled": true },
                    { "value": "no key" }
                ]
            }"#,
            None,
        )
        .unwrap();

        assert_eq!(imported.name, "Staging");
        let rows: Vec<_> = imported
            .variables
            .iter()
            .map(|v| (v.key.as_str(), v.value.as_str(), v.enabled, v.sensitive))
            .collect();
        assert_eq!(
            rows,
            [
                ("token", "t0k", true, true),
                ("port", "8080", true, false),
                ("off", "x", false, false),
                ("legacy", "y", false, false)
            ]
        );
    }

    fn collection_target() -> VariableTarget {
        VariableTarget::Collection {
            collection_id: "col".to_string(),
        }
    }

    fn environment_target() -> VariableTarget {
        VariableTarget::Environment {
            environment_id: "env".to_string(),
        }
    }

    fn entry(key: &str, value: &str) -> VariableEntry {
        VariableEntry {
            key: key.to_string(),
            value: value.to_string(),
            enabled: true,
            sensitive: false,
        }
    }

    /// A database with one collection (`col`) and one environment (`env`).
    fn connection_with_targets() -> Connection {
        let connection = crate::storage::test_connection();
        connection
            .execute_batch(
                "INSERT INTO collections (id, name, created_at, updated_at) VALUES ('col', 'C', '', '');
                 INSERT INTO environments (id, name, created_at, updated_at) VALUES ('env', 'E', '', '');",
            )
            .unwrap();
        connection
    }

    fn listed(connection: &Connection, target: &VariableTarget) -> Vec<(String, String)> {
        read_variables(connection, target)
            .unwrap()
            .into_iter()
            .map(|variable| (variable.key, variable.value))
            .collect()
    }

    fn pair(key: &str, value: &str) -> (String, String) {
        (key.to_string(), value.to_string())
    }

    #[test]
    fn moving_a_variable_between_scopes_in_one_save_leaves_it_in_exactly_one_place() {
        let mut connection = connection_with_targets();
        apply_changes(
            &mut connection,
            &[VariableChange {
                target: collection_target(),
                upserts: vec![entry("host", "old"), entry("untouched", "keep")],
                deletes: vec![],
            }],
            "t1",
        )
        .unwrap();

        apply_changes(
            &mut connection,
            &[
                VariableChange {
                    target: collection_target(),
                    upserts: vec![],
                    deletes: vec!["host".to_string()],
                },
                VariableChange {
                    target: environment_target(),
                    upserts: vec![entry("host", "new")],
                    deletes: vec![],
                },
            ],
            "t2",
        )
        .unwrap();

        assert_eq!(
            listed(&connection, &collection_target()),
            [pair("untouched", "keep")]
        );
        assert_eq!(
            listed(&connection, &environment_target()),
            [pair("host", "new")]
        );
    }

    #[test]
    fn a_failing_change_set_leaves_nothing_half_applied() {
        let mut connection = connection_with_targets();
        apply_changes(
            &mut connection,
            &[VariableChange {
                target: VariableTarget::Global,
                upserts: vec![entry("token", "abc")],
                deletes: vec![],
            }],
            "t1",
        )
        .unwrap();

        let result = apply_changes(
            &mut connection,
            &[
                VariableChange {
                    target: VariableTarget::Global,
                    upserts: vec![entry("other", "1")],
                    deletes: vec!["token".to_string()],
                },
                VariableChange {
                    target: environment_target(),
                    upserts: vec![entry("dup", "1"), entry(" dup ", "2")],
                    deletes: vec![],
                },
            ],
            "t2",
        );

        assert!(result.is_err());
        assert_eq!(
            listed(&connection, &VariableTarget::Global),
            [pair("token", "abc")]
        );
        assert_eq!(listed(&connection, &environment_target()), []);
    }

    #[test]
    fn saving_updates_existing_keys_and_ignores_blank_ones() {
        let mut connection = connection_with_targets();
        let mut secret = entry("  api_key ", "v2");
        secret.sensitive = true;
        apply_changes(
            &mut connection,
            &[VariableChange {
                target: VariableTarget::Global,
                upserts: vec![entry("api_key", "v1"), entry("  ", "ignored")],
                deletes: vec![],
            }],
            "t1",
        )
        .unwrap();

        apply_changes(
            &mut connection,
            &[VariableChange {
                target: VariableTarget::Global,
                upserts: vec![secret],
                deletes: vec!["missing".to_string()],
            }],
            "t2",
        )
        .unwrap();

        let stored = read_variables(&connection, &VariableTarget::Global).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(
            (
                stored[0].key.as_str(),
                stored[0].value.as_str(),
                stored[0].sensitive
            ),
            ("api_key", "v2", true)
        );
    }

    #[test]
    fn removing_a_stored_key_with_stray_spaces_deletes_that_key() {
        let mut connection = connection_with_targets();
        connection
            .execute(
                "INSERT INTO variables (scope, key, current_value, created_at, updated_at)
                 VALUES ('global', ' host ', 'padded', '', ''), ('global', 'host', 'plain', '', '')",
                [],
            )
            .unwrap();

        apply_changes(
            &mut connection,
            &[VariableChange {
                target: VariableTarget::Global,
                upserts: vec![],
                deletes: vec![" host ".to_string()],
            }],
            "t",
        )
        .unwrap();

        assert_eq!(
            listed(&connection, &VariableTarget::Global),
            [pair("host", "plain")]
        );
    }

    #[test]
    fn variable_context_applies_environment_over_collection_over_global() {
        let connection = test_connection();
        let request = RequestDetail {
            id: "request".to_string(),
            collection_id: "collection".to_string(),
            name: "Request".to_string(),
            method: "GET".to_string(),
            url: "{{host}}".to_string(),
            headers: Vec::new(),
            query: Vec::new(),
            path_params: Vec::new(),
            ..RequestDetail::default()
        };

        insert_test_variable(&connection, "global", None, None, "host", "global");
        insert_test_variable(
            &connection,
            "collection",
            Some("collection"),
            None,
            "host",
            "collection",
        );
        insert_test_variable(
            &connection,
            "environment",
            None,
            Some("env"),
            "host",
            "environment",
        );

        let context = load_variable_context(&connection, &request, Some("env")).unwrap();

        assert_eq!(context.get("host").as_deref(), Some("environment"));
    }

    fn test_connection() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE variables (
                    scope TEXT NOT NULL,
                    collection_id TEXT,
                    environment_id TEXT,
                    key TEXT NOT NULL,
                    current_value TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    sensitive INTEGER NOT NULL DEFAULT 0,
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
        collection_id: Option<&str>,
        environment_id: Option<&str>,
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
