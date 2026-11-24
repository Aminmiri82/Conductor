use std::collections::{HashMap, HashSet};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use tauri::State;
use uuid::Uuid;

use super::{
    collections::repository::{self, values, RepositoryRefresh},
    error::CommandError,
    models::{
        CreateEnvironmentInput, DuplicateEnvironmentInput, EnvironmentSummary,
        RenameEnvironmentInput, RequestDetail, SharedVariableChange, VariableChange, VariableEntry,
        VariableTarget,
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
    value: Option<String>,
    sensitive: bool,
    private_repository_value: bool,
}

impl VariableContext {
    pub(crate) fn get(&self, key: &str) -> Option<String> {
        match self.values.get(key) {
            Some(stored) => stored.value.clone(),
            None if !self.no_dynamic => dynamic_variable(key),
            None => None,
        }
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
                            value: Some(value.to_string()),
                            sensitive,
                            private_repository_value: false,
                        },
                    )
                })
                .collect(),
            no_dynamic: false,
        }
    }

    pub(crate) fn relies_on_private_repository_value(&self, key: &str) -> bool {
        self.values
            .get(key)
            .is_some_and(|stored| stored.private_repository_value)
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
        "SELECT id, collection_id, name FROM environments WHERE archived = 0 ORDER BY name",
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
    tx.execute(
        "INSERT INTO variables
         (scope, environment_id, key, current_value, enabled, sensitive, created_at, updated_at)
         SELECT 'environment', ?, key, current_value, enabled, sensitive, ?, ?
         FROM variables WHERE scope = 'environment' AND environment_id = ?",
        params![new_id, now, now, source_id],
    )?;
    if let Some(source) = values::target(
        &tx,
        &VariableTarget::Environment {
            environment_id: source_id.into(),
        },
    )? {
        values::stage_environment_copy(&tx, &source, new_id)?;
    }
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
                "UPDATE environments SET archived = 1 WHERE id = ?
                 AND collection_id IN (SELECT collection_id FROM repository_links)",
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
    if let Some(linked) = values::target(&tx, &target)? {
        values::stage_environment_import(
            &tx,
            &linked,
            imported.variables.iter().map(|variable| VariableEntry {
                key: variable.key.clone(),
                value: variable.value.clone(),
                enabled: variable.enabled,
                sensitive: variable.sensitive,
                origin: None,
                presentation: None,
                required_secret: None,
            }),
        )?;
    } else {
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

pub(crate) fn read_variables(
    connection: &Connection,
    target: &VariableTarget,
) -> Result<Vec<VariableEntry>, StorageError> {
    if let Some(target) = values::target(connection, target)? {
        return values::list(connection, &target);
    }
    let (scope, collection_id, environment_id) = target.columns();
    let mut statement = connection.prepare_cached(
        "SELECT key, current_value, enabled, sensitive
         FROM variables
         WHERE scope = ?1 AND collection_id IS ?2 AND environment_id IS ?3
           AND NOT EXISTS (
               SELECT 1 FROM environments WHERE id = ?3 AND archived = 1
           )
         ORDER BY key",
    )?;
    let rows = statement.query_map(params![scope, collection_id, environment_id], |row| {
        Ok(VariableEntry {
            key: row.get(0)?,
            value: row.get(1)?,
            enabled: row.get::<_, i64>(2)? != 0,
            sensitive: row.get::<_, i64>(3)? != 0,
            origin: None,
            presentation: None,
            required_secret: None,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::from)
}

fn repository_target(
    connection: &Connection,
    target: &VariableTarget,
) -> Result<values::Target, StorageError> {
    values::target(connection, target)?.ok_or_else(|| {
        StorageError::InvalidInput("Choose a linked collection or environment.".into())
    })
}

#[tauri::command(async)]
#[specta::specta]
pub fn get_shared_variable(
    target: VariableTarget,
    key: String,
    state: State<'_, AppState>,
) -> Result<String, CommandError> {
    Ok(state.database.with_read_connection(|connection| {
        values::read_shared(connection, &repository_target(connection, &target)?, &key)
    })?)
}

#[tauri::command(async)]
#[specta::specta]
pub fn save_shared_variable(
    target: VariableTarget,
    key: String,
    change: SharedVariableChange,
    state: State<'_, AppState>,
) -> Result<RepositoryRefresh, CommandError> {
    Ok(state.database.with_connection(|connection| {
        let target = repository_target(connection, &target)?;
        values::edit_shared(connection, &target, &key, change)?;
        repository::refresh_all(connection)
    })?)
}

/// Linked edits replace one local.json per repository. Other scopes use the
/// SQLite transaction.
#[tauri::command(async)]
#[specta::specta]
pub fn apply_variable_changes(
    changes: Vec<VariableChange>,
    state: State<'_, AppState>,
) -> Result<bool, CommandError> {
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
) -> Result<bool, StorageError> {
    let mut linked = Vec::new();
    let mut ordinary = Vec::new();
    for change in changes {
        let mut seen = HashSet::new();
        for key in change
            .upserts
            .iter()
            .map(|variable| variable.key.trim())
            .filter(|key| !key.is_empty())
        {
            if !seen.insert(key) {
                return Err(StorageError::InvalidInput(format!(
                    "variable \"{key}\" appears more than once"
                )));
            }
        }
        if let VariableTarget::Environment { environment_id } = &change.target {
            // Queued edits may still target an environment archived by a branch switch.
            let exists: bool = connection
                .prepare_cached("SELECT EXISTS(SELECT 1 FROM environments WHERE id = ?)")?
                .query_row([environment_id], |row| row.get(0))?;
            if !exists {
                return Err(environment_not_found());
            }
        }
        if let Some(target) = values::target(connection, &change.target)? {
            linked.push((target, change));
        } else {
            ordinary.push(change);
        }
    }

    // In a cross-store move, make the SQLite destination durable before
    // removing the file-backed source. On interruption both copies can remain.
    let linked_deletes: HashSet<_> = linked
        .iter()
        .flat_map(|(_, change)| &change.deletes)
        .map(String::as_str)
        .collect();
    let destinations: Vec<_> = ordinary
        .iter()
        .flat_map(|change| change.upserts.iter().map(|entry| (&change.target, entry)))
        .filter(|(_, entry)| linked_deletes.contains(entry.key.trim()))
        .collect();
    if !destinations.is_empty() {
        let tx = connection.transaction()?;
        for (target, entry) in destinations {
            write_entry(&tx, target, entry, now)?;
        }
        tx.commit()?;
    }

    let tx = connection.transaction()?;
    let mut delete = tx.prepare_cached(
        "DELETE FROM variables
         WHERE scope = ? AND key = ? AND collection_id IS ? AND environment_id IS ?",
    )?;
    for change in ordinary {
        let (scope, collection_id, environment_id) = change.target.columns();
        for key in &change.deletes {
            delete.execute(params![scope, key, collection_id, environment_id])?;
        }
        for entry in &change.upserts {
            write_entry(&tx, &change.target, entry, now)?;
        }
    }
    drop(delete);
    let needs_reload = values::edit(&tx, &linked)?;
    tx.commit()?;
    Ok(needs_reload)
}

fn write_entry(
    connection: &Connection,
    target: &VariableTarget,
    entry: &VariableEntry,
    now: &str,
) -> Result<(), StorageError> {
    let key = entry.key.trim();
    if !key.is_empty() {
        upsert_variable(
            connection,
            &NewVariable {
                target,
                key,
                value: &entry.value,
                enabled: entry.enabled,
                sensitive: entry.sensitive,
            },
            now,
        )?;
    }
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
        "SELECT key, current_value, sensitive, private_repository_value
         FROM (
             SELECT key, current_value, sensitive, 0 AS private_repository_value,
                    CASE scope WHEN 'global' THEN 0 WHEN 'collection' THEN 1 ELSE 2 END AS priority
             FROM variables
             WHERE enabled = 1
               AND (
                   scope = 'global'
                   OR (scope = 'collection' AND collection_id = ?1)
                   OR (scope = 'environment' AND collection_id IS NULL AND environment_id = ?2)
               )
             UNION ALL
             SELECT key,
                    CASE WHEN enabled AND (NOT required_secret OR value != '') THEN value END,
                    sensitive, local OR required_secret,
                    CASE WHEN scope_id = ?1 THEN 1 ELSE 2 END
             FROM repository_variables
             WHERE collection_id = ?1 AND scope_id IN (?1, ?2) AND (enabled OR required_secret)
         )
         ORDER BY priority, key",
    )?;
    let mut rows = statement.query(params![request.collection_id, environment_id])?;
    while let Some(row) = rows.next()? {
        let key = row.get::<_, String>(0)?;
        values.insert(
            key,
            StoredValue {
                value: row.get(1)?,
                sensitive: row.get(2)?,
                private_repository_value: row.get(3)?,
            },
        );
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
        .prepare_cached("SELECT collection_id FROM environments WHERE id = ? AND archived = 0")?
        .query_row([environment_id], |row| row.get::<_, String>(0))
        .optional()?)
}

fn environment_not_found() -> StorageError {
    StorageError::InvalidInput("environment not found".to_string())
}

pub(crate) fn save_script_variables(
    connection: &Connection,
    writes: &[(VariableTarget, &str, &str)],
    now: &str,
) -> Result<(), StorageError> {
    let mut linked = Vec::new();
    for (target, key, value) in writes {
        if let Some(target) = values::target(connection, target)? {
            linked.push((target, *key, *value));
            continue;
        }
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
    }
    values::set_script_values(connection, &linked)
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
    use std::fs;

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
            origin: None,
            presentation: None,
            required_secret: None,
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
                baseline: None,
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
                    baseline: None,
                    target: collection_target(),
                    upserts: vec![],
                    deletes: vec!["host".to_string()],
                },
                VariableChange {
                    baseline: None,
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
                baseline: None,
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
                    baseline: None,
                    target: VariableTarget::Global,
                    upserts: vec![entry("other", "1")],
                    deletes: vec!["token".to_string()],
                },
                VariableChange {
                    baseline: None,
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
                baseline: None,
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
                baseline: None,
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
                baseline: None,
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
    fn linked_variable_editors_distinguish_defaults_overrides_and_required_secrets() {
        use crate::commands::models::{VariableOrigin, VariableRemoval};

        let root = std::env::temp_dir().join(format!("conductor-vars-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let mut connection = crate::storage::test_connection();
        let link = repository::connect(&mut connection, &root, true, None).unwrap();
        let target = VariableTarget::Collection {
            collection_id: link.collection_id.clone(),
        };
        let request = RequestDetail {
            collection_id: link.collection_id,
            ..RequestDetail::default()
        };
        let manifest_path = root.join(".conductor/collection.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["values"] = serde_json::json!({"host":"shared"});
        manifest["secrets"] = serde_json::json!(["token"]);
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        fs::write(&manifest_path, &manifest_bytes).unwrap();
        fixtures::variable(&connection, &VariableTarget::Global, "token", "wrong-token");
        repository::refresh_all(&mut connection).unwrap();

        let context = load_variable_context(&connection, &request, None).unwrap();
        assert_eq!(context.get("host").as_deref(), Some("shared"));
        assert_eq!(context.get("token"), None);
        let rows = read_variables(&connection, &target).unwrap();
        let shared = rows[0].presentation.as_ref().unwrap();
        assert_eq!(rows[0].origin, Some(VariableOrigin::Shared));
        assert!(shared.fixed_key && shared.edit_shared);
        assert_eq!(shared.removal, VariableRemoval::None);
        let required = rows[1].presentation.as_ref().unwrap();
        assert_eq!(rows[1].origin, Some(VariableOrigin::MissingSecret));
        assert_eq!(rows[1].required_secret, Some(true));
        assert!(required.fixed_key && !required.edit_shared);

        assert!(apply_changes(
            &mut connection,
            &[VariableChange {
                baseline: None,
                target: target.clone(),
                upserts: vec![entry("host", "private"), entry("token", "private-token")],
                deletes: vec![],
            }],
            "now",
        )
        .unwrap());
        assert_eq!(fs::read(&manifest_path).unwrap(), manifest_bytes);
        assert!(fs::read_to_string(root.join(".conductor/local.json"))
            .unwrap()
            .contains("private-token"));
        let context = load_variable_context(&connection, &request, None).unwrap();
        assert_eq!(context.get("host").as_deref(), Some("private"));
        assert_eq!(context.get("token").as_deref(), Some("private-token"));
        assert!(context.is_sensitive("token"));
        let rows = read_variables(&connection, &target).unwrap();
        assert_eq!(rows[0].origin, Some(VariableOrigin::Local));
        assert_eq!(
            rows[0].presentation.as_ref().unwrap().removal,
            VariableRemoval::UseShared,
        );
        assert_eq!(rows[1].origin, Some(VariableOrigin::Local));
        assert_eq!(
            rows[1].presentation.as_ref().unwrap().removal,
            VariableRemoval::Local,
        );

        fs::write(
            root.join(".conductor/local.json"),
            br#"{"collection":{"host":"editor","token":"editor-token"}}"#,
        )
        .unwrap();
        repository::refresh_all(&mut connection).unwrap();
        assert_eq!(
            listed(&connection, &target),
            [pair("host", "editor"), pair("token", "editor-token")]
        );

        assert!(apply_changes(
            &mut connection,
            &[VariableChange {
                baseline: None,
                target: target.clone(),
                upserts: vec![],
                deletes: vec!["host".into()],
            }],
            "now",
        )
        .unwrap());
        assert_eq!(
            listed(&connection, &target),
            [pair("host", "shared"), pair("token", "editor-token")]
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_branch_removing_an_environment_keeps_its_local_values_for_when_it_returns() {
        let root = std::env::temp_dir().join(format!("conductor-env-test-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let mut connection = crate::storage::test_connection();
        let link = repository::connect(&mut connection, &root, true, None).unwrap();
        let collection_id = link.collection_id;
        let environment_id = repository::new_entity_id(&connection, &collection_id).unwrap();
        repository::mutate(&mut connection, &collection_id, |connection| {
            insert_environment(
                connection,
                &environment_id,
                &collection_id,
                "Staging",
                "now",
            )
        })
        .unwrap();
        let environment_path = fs::read_dir(root.join(".conductor/environments"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let mut environment: serde_json::Value =
            serde_json::from_slice(&fs::read(&environment_path).unwrap()).unwrap();
        environment["values"] = serde_json::json!({"token": "shared"});
        let shared = serde_json::to_vec(&environment).unwrap();
        fs::write(&environment_path, &shared).unwrap();
        let refresh = repository::refresh_all(&mut connection).unwrap();
        assert_eq!(
            refresh.changed_environment_ids,
            std::slice::from_ref(&environment_id)
        );

        let target = VariableTarget::Environment {
            environment_id: environment_id.clone(),
        };
        assert_eq!(listed(&connection, &target), [pair("token", "shared")]);
        apply_changes(
            &mut connection,
            &[VariableChange {
                baseline: None,
                target: target.clone(),
                upserts: vec![entry("token", "private")],
                deletes: vec![],
            }],
            "now",
        )
        .unwrap();
        assert_eq!(fs::read(&environment_path).unwrap(), shared);
        let request = RequestDetail {
            collection_id,
            ..RequestDetail::default()
        };

        fs::remove_file(&environment_path).unwrap();
        let refresh = repository::refresh_all(&mut connection).unwrap();
        assert_eq!(
            refresh.changed_environment_ids,
            std::slice::from_ref(&environment_id)
        );
        assert!(read_environments(&connection).unwrap().is_empty());
        assert!(listed(&connection, &target).is_empty());
        assert!(load_variable_context(&connection, &request, Some(&environment_id)).is_err());

        fs::write(&environment_path, &shared).unwrap();
        let refresh = repository::refresh_all(&mut connection).unwrap();
        assert_eq!(
            refresh.changed_environment_ids,
            std::slice::from_ref(&environment_id)
        );
        fs::remove_dir_all(&root).unwrap();

        assert_eq!(
            read_environments(&connection).unwrap()[0].id,
            environment_id
        );
        assert_eq!(listed(&connection, &target), [pair("token", "private")]);
        let context = load_variable_context(&connection, &request, Some(&environment_id)).unwrap();
        assert_eq!(context.get("token").as_deref(), Some("private"));
    }

    #[test]
    fn a_branch_switch_keeps_local_edits_queued_before_an_environment_disappeared() {
        let mut connection = connection_with_targets();
        connection
            .execute("UPDATE environments SET archived = 1 WHERE id = 'env'", [])
            .unwrap();

        apply_changes(
            &mut connection,
            &[VariableChange {
                baseline: None,
                target: environment_target(),
                upserts: vec![entry("host", "local-override")],
                deletes: vec![],
            }],
            "now",
        )
        .expect("save the queued edit against the archived environment");

        connection
            .execute("UPDATE environments SET archived = 0 WHERE id = 'env'", [])
            .unwrap();
        assert_eq!(
            listed(&connection, &environment_target()),
            [pair("host", "local-override")]
        );
    }

    #[test]
    fn linked_environment_import_and_duplicate_use_the_allocated_stems() {
        let root = std::env::temp_dir().join(format!("conductor-env-test-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let mut connection = crate::storage::test_connection();
        let link = repository::connect(&mut connection, &root, true, None).unwrap();
        let collection = link.collection_id;
        let local_path = root.join(".conductor/local.json");
        let orphan = r#""development-copy" : { "TOKEN" : "fixture-orphan" }"#;
        fs::write(&local_path, format!(r#"{{"environments":{{{orphan}}}}}"#)).unwrap();

        let imported =
            parse_imported_environment("TOKEN=fixture-token", Some("development.env")).unwrap();
        let original_id = repository::new_entity_id(&connection, &collection).unwrap();
        repository::mutate(&mut connection, &collection, |connection| {
            store_imported_environment(connection, &original_id, &collection, &imported, "now")
        })
        .unwrap();
        let copy_id = repository::new_entity_id(&connection, &collection).unwrap();
        repository::mutate(&mut connection, &collection, |connection| {
            copy_environment(connection, &original_id, &copy_id, "now")
        })
        .unwrap();
        repository::refresh_all(&mut connection).unwrap();

        let bytes = fs::read_to_string(&local_path).unwrap();
        assert!(bytes.contains(orphan));
        let local: serde_json::Value = serde_json::from_str(&bytes).unwrap();
        for (stem, environment_id) in [
            ("development", original_id),
            ("development-copy-2", copy_id),
        ] {
            let path = root.join(format!(".conductor/environments/{stem}.json"));
            let shared = fs::read_to_string(&path).unwrap();
            assert!(!shared.contains("fixture-token"));
            assert!(local["environments"][stem].get("TOKEN").is_some());
            let target = VariableTarget::Environment { environment_id };
            assert_eq!(
                listed(&connection, &target),
                [pair("TOKEN", "fixture-token")]
            );

            // Replay the on-disk state after local values are installed but
            // before the shared environment file is installed.
            fs::remove_file(&path).unwrap();
            repository::refresh_all(&mut connection).unwrap();
            assert!(listed(&connection, &target).is_empty());
            assert_eq!(fs::read_to_string(&local_path).unwrap(), bytes);
            fs::write(path, shared).unwrap();
            repository::refresh_all(&mut connection).unwrap();
            assert_eq!(
                listed(&connection, &target),
                [pair("TOKEN", "fixture-token")]
            );
        }
        fs::remove_dir_all(&root).unwrap();
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
