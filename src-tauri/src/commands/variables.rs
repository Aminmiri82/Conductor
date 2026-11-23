use std::collections::{HashMap, HashSet};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use tauri::State;
use uuid::Uuid;

use super::{
    collections::repository,
    error::CommandError,
    models::{
        CreateEnvironmentInput, DuplicateEnvironmentInput, EnvironmentSummary,
        RenameEnvironmentInput, RequestDetail, VariableChange, VariableEntry, VariableTarget,
    },
    postman::{flag, flag_or_false, lenient_list, stringified, text},
};
use crate::storage::{upsert_variable, NewVariable, StorageError};
use crate::AppState;

#[derive(Debug, Default)]
pub(crate) struct VariableContext {
    values: HashMap<String, StoredValue>,
    /// Set by `nothing`, so `{{$guid}}` and friends stay unresolved too.
    no_dynamic: bool,
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
            .or_else(|| (!self.no_dynamic).then(|| dynamic_variable(key)).flatten())
    }

    /// Resolves no variable at all, not even the dynamic ones.
    pub(crate) fn nothing() -> Self {
        Self {
            values: HashMap::new(),
            no_dynamic: true,
        }
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
            no_dynamic: false,
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
    Ok(state.database.with_read_connection(read_environments)?)
}

pub(crate) fn read_environments(
    connection: &Connection,
) -> Result<Vec<EnvironmentSummary>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT environments.id, environments.collection_id, environments.name
         FROM environments
         LEFT JOIN repository_environments ON repository_environments.environment_id = environments.id
         WHERE repository_environments.active IS NOT 0
         ORDER BY environments.name",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(EnvironmentSummary {
            id: row.get(0)?,
            collection_id: row.get(1)?,
            name: row.get(2)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::from)
}

#[tauri::command(async)]
#[specta::specta]
pub fn create_environment(
    input: CreateEnvironmentInput,
    state: State<'_, AppState>,
) -> Result<String, CommandError> {
    let name = clean_environment_name(&input.name);
    let now = Utc::now().to_rfc3339();

    Ok(state.database.with_connection(|connection| {
        let environment_id = repository::new_entity_id(connection, &input.collection_id)?;
        repository::mutate(connection, &input.collection_id, |connection| {
            insert_environment(
                connection,
                &environment_id,
                &input.collection_id,
                &name,
                &now,
            )
        })?;
        Ok(environment_id)
    })?)
}

fn insert_environment(
    connection: &Connection,
    id: &str,
    collection_id: &str,
    name: &str,
    now: &str,
) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO environments (id, collection_id, name, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?)",
        params![id, collection_id, name, now, now],
    )?;
    track_repository_environment(connection, id)?;
    Ok(())
}

fn track_repository_environment(
    connection: &Connection,
    environment_id: &str,
) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO repository_environments (environment_id, active)
         SELECT environments.id, 1
         FROM environments JOIN repository_links USING (collection_id)
         WHERE environments.id = ?",
        [environment_id],
    )?;
    Ok(())
}

/// Copies an environment and all its variables into the same collection,
/// named "<name> copy". Returns the new environment's id.
#[tauri::command(async)]
#[specta::specta]
pub fn duplicate_environment(
    input: DuplicateEnvironmentInput,
    state: State<'_, AppState>,
) -> Result<String, CommandError> {
    let now = Utc::now().to_rfc3339();
    Ok(state.database.with_connection(|connection| {
        let collection_id = environment_owner(connection, &input.environment_id)?
            .ok_or_else(environment_not_found)?;
        let environment_id = repository::new_entity_id(connection, &collection_id)?;
        repository::mutate(connection, &collection_id, |connection| {
            copy_environment(connection, &input.environment_id, &environment_id, &now)
        })?;
        Ok(environment_id)
    })?)
}

fn copy_environment(
    connection: &mut Connection,
    source_id: &str,
    new_id: &str,
    now: &str,
) -> Result<(), StorageError> {
    let tx = connection.savepoint()?;
    let copied = tx.execute(
        "INSERT INTO environments (id, collection_id, name, created_at, updated_at)
         SELECT ?, collection_id, name || ' copy', ?, ?
         FROM environments WHERE id = ?",
        params![new_id, now, now, source_id],
    )?;
    if copied == 0 {
        return Err(environment_not_found());
    }
    track_repository_environment(&tx, new_id)?;
    tx.execute(
        "INSERT INTO variables
         (scope, environment_id, key, current_value, enabled, sensitive, created_at, updated_at)
         SELECT 'environment', ?, key, current_value, enabled, sensitive, ?, ?
         FROM variables WHERE scope = 'environment' AND environment_id = ?",
        params![new_id, now, now, source_id],
    )?;
    tx.execute(
        "INSERT INTO repository_defaults (environment_id, key, value)
         SELECT ?, key, value FROM repository_defaults WHERE environment_id = ?",
        params![new_id, source_id],
    )?;
    tx.execute(
        "INSERT INTO repository_default_hides (environment_id, key)
         SELECT ?, key FROM repository_default_hides WHERE environment_id = ?",
        params![new_id, source_id],
    )?;
    tx.commit()?;
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
        let collection_id = environment_owner(connection, &input.environment_id)?
            .ok_or_else(environment_not_found)?;
        repository::mutate(connection, &collection_id, |connection| {
            connection.execute(
                "UPDATE environments SET name = ?, updated_at = ? WHERE id = ?",
                params![name, now, input.environment_id],
            )?;
            Ok(())
        })
    })?)
}

#[tauri::command(async)]
#[specta::specta]
pub fn delete_environment(
    environment_id: String,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    Ok(state.database.with_connection(|connection| {
        let Some(collection_id) = environment_owner(connection, &environment_id)? else {
            return Ok(());
        };
        repository::mutate(connection, &collection_id, |connection| {
            let archived = connection.execute(
                "UPDATE repository_environments SET active = 0 WHERE environment_id = ?",
                params![environment_id],
            )?;
            if archived == 0 {
                connection.execute(
                    "DELETE FROM environments WHERE id = ?",
                    params![environment_id],
                )?;
            }
            Ok(())
        })
    })?)
}

#[tauri::command(async)]
#[specta::specta]
pub fn import_postman_environment(
    collection_id: String,
    postman_json: String,
    file_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, CommandError> {
    let imported = parse_imported_environment(&postman_json, file_name.as_deref())?;
    let now = Utc::now().to_rfc3339();

    Ok(state.database.with_connection(|connection| {
        let environment_id = repository::new_entity_id(connection, &collection_id)?;
        repository::mutate(connection, &collection_id, |connection| {
            store_imported_environment(connection, &environment_id, &collection_id, &imported, &now)
        })?;
        Ok(environment_id)
    })?)
}

fn store_imported_environment(
    connection: &mut Connection,
    environment_id: &str,
    collection_id: &str,
    imported: &ImportedEnvironment,
    now: &str,
) -> Result<(), StorageError> {
    let tx = connection.savepoint()?;
    insert_environment(&tx, environment_id, collection_id, &imported.name, now)?;
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
    connection: &Connection,
    target: &VariableTarget,
) -> Result<Vec<VariableEntry>, StorageError> {
    let (scope, collection_id, environment_id) = target.columns();
    let mut statement = connection.prepare_cached(
        "SELECT key, current_value, enabled, sensitive
         FROM variables
         WHERE scope = ?1 AND collection_id IS ?2 AND environment_id IS ?3
           AND NOT EXISTS (
               SELECT 1 FROM repository_environments WHERE environment_id = ?3 AND active = 0
           )
         UNION ALL
         SELECT key, value, 1, 0 FROM repository_default_values WHERE environment_id = ?3
           AND NOT EXISTS (
               SELECT 1 FROM repository_environments WHERE environment_id = ?3 AND active = 0
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
/// Edits in linked environments stay in SQLite, including hides for deleted defaults.
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
    connection: &mut Connection,
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
        if let Some(environment_id) = environment_id {
            environment_owner(&tx, environment_id)?.ok_or_else(environment_not_found)?;
        }
        for key in &change.deletes {
            delete.execute(params![scope, key, collection_id, environment_id])?;
            if let Some(environment_id) = environment_id {
                tx.prepare_cached(
                    "INSERT OR IGNORE INTO repository_default_hides (environment_id, key)
                     SELECT environment_id, key FROM repository_defaults
                     WHERE environment_id = ? AND key = ?",
                )?
                .execute(params![environment_id, key])?;
            }
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
            if let Some(environment_id) = environment_id {
                tx.prepare_cached(
                    "DELETE FROM repository_default_hides WHERE environment_id = ? AND key = ?",
                )?
                .execute(params![environment_id, key])?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}

/// Every send and preview goes through here, so this is where an environment
/// from another collection is refused rather than silently mixed in.
pub(crate) fn load_variable_context(
    connection: &Connection,
    request: &RequestDetail,
    environment_id: Option<&str>,
) -> Result<VariableContext, StorageError> {
    if let Some(environment_id) = environment_id {
        ensure_environment_in_collection(connection, environment_id, &request.collection_id)?;
    }
    let mut values = HashMap::new();
    let mut statement = connection.prepare_cached(
        "SELECT key, current_value, sensitive
         FROM (
             SELECT key, current_value, sensitive, scope
             FROM variables
             WHERE enabled = 1
               AND (
                   scope = 'global'
                   OR (scope = 'collection' AND collection_id = ?1)
                   OR (scope = 'environment' AND collection_id IS NULL AND environment_id = ?2)
               )
             UNION ALL
             SELECT key, value, 0, 'environment'
             FROM repository_default_values WHERE environment_id = ?2
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
    Ok(VariableContext {
        values,
        no_dynamic: false,
    })
}

fn ensure_environment_in_collection(
    connection: &Connection,
    environment_id: &str,
    collection_id: &str,
) -> Result<(), StorageError> {
    match environment_owner(connection, environment_id)? {
        Some(owner) if owner == collection_id => Ok(()),
        Some(_) => Err(StorageError::InvalidInput(
            "the environment belongs to a different collection".to_string(),
        )),
        None => Err(environment_not_found()),
    }
}

fn environment_owner(
    connection: &Connection,
    environment_id: &str,
) -> Result<Option<String>, StorageError> {
    Ok(connection
        .prepare_cached(
            "SELECT environments.collection_id FROM environments
             LEFT JOIN repository_environments ON repository_environments.environment_id = environments.id
             WHERE environments.id = ? AND repository_environments.active IS NOT 0",
        )?
        .query_row(
            [environment_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?)
}

fn environment_not_found() -> StorageError {
    StorageError::InvalidInput("environment not found".to_string())
}

pub(crate) fn save_script_variable(
    connection: &Connection,
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
    use crate::fixtures;
    use rusqlite::Connection;

    #[test]
    fn env_file_with_a_repeated_key_imports_with_the_last_value() {
        let mut connection = crate::storage::test_connection();
        let collection = fixtures::collection(&connection);
        let imported = parse_imported_environment(
            "API_URL=http://old\nTOKEN=abc\nAPI_URL=http://new\n",
            Some("local.env"),
        )
        .unwrap();

        store_imported_environment(&mut connection, "env", &collection, &imported, "now").unwrap();

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
                 INSERT INTO environments (id, collection_id, name, created_at, updated_at)
                 VALUES ('env', 'col', 'E', '', '');",
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
        let connection = crate::storage::test_connection();
        let collection = fixtures::collection(&connection);
        let environment = fixtures::environment(&connection, &collection);
        let request = RequestDetail {
            collection_id: collection.clone(),
            url: "{{host}}".to_string(),
            ..RequestDetail::default()
        };
        fixtures::variable(&connection, &VariableTarget::Global, "host", "global");
        let collection_target = VariableTarget::Collection {
            collection_id: collection,
        };
        fixtures::variable(&connection, &collection_target, "host", "collection");
        let environment_target = VariableTarget::Environment {
            environment_id: environment.clone(),
        };
        fixtures::variable(&connection, &environment_target, "host", "environment");

        let context = load_variable_context(&connection, &request, Some(&environment)).unwrap();

        assert_eq!(context.get("host").as_deref(), Some("environment"));
    }

    #[test]
    fn a_request_cannot_use_another_collections_environment() {
        let connection = crate::storage::test_connection();
        let billing = fixtures::collection(&connection);
        let storefront = fixtures::collection(&connection);
        let storefront_environment = fixtures::environment(&connection, &storefront);
        let request = RequestDetail {
            collection_id: billing,
            url: "{{host}}".to_string(),
            ..RequestDetail::default()
        };

        let result = load_variable_context(&connection, &request, Some(&storefront_environment));

        assert!(
            matches!(result, Err(StorageError::InvalidInput(ref message)) if message.contains("different collection")),
            "{result:?}"
        );
    }

    #[test]
    fn repository_defaults_are_overridden_and_hidden_locally() {
        let mut connection = connection_with_targets();
        connection
            .execute_batch(
                "INSERT INTO repository_environments (environment_id, active) VALUES ('env', 1);
                 INSERT INTO repository_defaults (environment_id, key, value)
                 VALUES ('env', 'host', 'shared'), ('env', 'region', 'default');",
            )
            .unwrap();
        let request = RequestDetail {
            collection_id: "col".to_string(),
            ..RequestDetail::default()
        };
        let context = load_variable_context(&connection, &request, Some("env")).unwrap();
        assert_eq!(context.get("host").as_deref(), Some("shared"));

        let mut secret = entry("host", "private");
        secret.sensitive = true;
        apply_changes(
            &mut connection,
            &[VariableChange {
                target: environment_target(),
                upserts: vec![secret],
                deletes: vec!["region".to_string()],
            }],
            "now",
        )
        .unwrap();

        let context = load_variable_context(&connection, &request, Some("env")).unwrap();
        assert_eq!(context.get("host").as_deref(), Some("private"));
        assert!(context.is_sensitive("host"));
        assert_eq!(context.get("region"), None);
        assert_eq!(
            listed(&connection, &environment_target()),
            [pair("host", "private")]
        );
        let shared: String = connection
            .query_row(
                "SELECT value FROM repository_defaults WHERE environment_id = 'env' AND key = 'host'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(shared, "shared");
    }

    #[test]
    fn a_branch_removing_an_environment_keeps_its_local_values_for_when_it_returns() {
        let connection = connection_with_targets();
        connection
            .execute(
                "INSERT INTO repository_environments (environment_id, active) VALUES ('env', 0)",
                [],
            )
            .unwrap();
        save_script_variable(
            &connection,
            &environment_target(),
            "token",
            "private",
            "now",
        )
        .unwrap();
        let request = RequestDetail {
            collection_id: "col".to_string(),
            ..RequestDetail::default()
        };

        assert!(read_environments(&connection).unwrap().is_empty());
        assert!(listed(&connection, &environment_target()).is_empty());
        assert!(load_variable_context(&connection, &request, Some("env")).is_err());

        connection
            .execute(
                "UPDATE repository_environments SET active = 1 WHERE environment_id = 'env'",
                [],
            )
            .unwrap();
        assert_eq!(read_environments(&connection).unwrap()[0].id, "env");
        let context = load_variable_context(&connection, &request, Some("env")).unwrap();
        assert_eq!(context.get("token").as_deref(), Some("private"));
        let shared_count: i64 = connection
            .query_row("SELECT count(*) FROM repository_defaults", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(shared_count, 0);
    }

    #[test]
    fn duplicating_an_environment_copies_its_variables_into_the_same_collection() {
        let mut connection = crate::storage::test_connection();
        let collection = fixtures::collection(&connection);
        let original = fixtures::environment(&connection, &collection);
        let original_target = VariableTarget::Environment {
            environment_id: original.clone(),
        };
        fixtures::variable(&connection, &original_target, "host", "staging");

        copy_environment(&mut connection, &original, "copy", "now").unwrap();
        fixtures::variable(&connection, &original_target, "host", "changed later");

        let (owner, name): (String, String) = connection
            .query_row(
                "SELECT collection_id, name FROM environments WHERE id = 'copy'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((owner, name), (collection, "Environment copy".to_string()));
        let copy_target = VariableTarget::Environment {
            environment_id: "copy".to_string(),
        };
        assert_eq!(listed(&connection, &copy_target), [pair("host", "staging")]);
    }
}
