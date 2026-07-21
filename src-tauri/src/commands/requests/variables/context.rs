use std::collections::HashMap;

use chrono::Utc;
use rusqlite::params;
use uuid::Uuid;

use crate::commands::models::RequestDetail;
use crate::storage::{Secrets, StorageError};

/// A resolved snapshot of all enabled variables for a request execution.
#[derive(Debug, Clone)]
pub(in crate::commands::requests) struct VariableContext {
    values: HashMap<String, String>,
}

impl VariableContext {
    pub(in crate::commands::requests) fn get(&self, key: &str) -> Option<String> {
        self.values
            .get(key)
            .cloned()
            .or_else(|| dynamic_variable(key))
    }
}

pub(in crate::commands::requests) fn load_variable_context(
    connection: &rusqlite::Connection,
    secrets: &Secrets,
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
    let rows = statement.query_map(
        params![request.collection_id, environment_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
            ))
        },
    )?;
    for row in rows {
        let (key, value, sensitive) = row?;
        let value = if sensitive {
            secrets.decrypt(&value)?
        } else {
            value
        };
        values.insert(key, value);
    }
    Ok(VariableContext { values })
}

pub(in crate::commands::requests) fn save_script_variable(
    connection: &rusqlite::Connection,
    scope: &str,
    collection_id: Option<&str>,
    environment_id: Option<&str>,
    key: &str,
    value: &str,
    now: &str,
) -> Result<(), StorageError> {
    super::crud::validate_scope(scope, collection_id, environment_id)
        .map_err(StorageError::InvalidInput)?;
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

fn dynamic_variable(key: &str) -> Option<String> {
    match key {
        "$guid" | "$randomUUID" => Some(Uuid::new_v4().to_string()),
        "$timestamp" => Some(Utc::now().timestamp().to_string()),
        "$isoTimestamp" => Some(Utc::now().to_rfc3339()),
        _ => None,
    }
}

