use rusqlite::{params, Connection};
use serde_json::Value;

use super::{secrets, Secrets, StorageError};

const INITIAL_SCHEMA: &str = include_str!("schema/001_initial.sql");
const SORT_ORDER_STEP: i64 = 1024;

pub fn run(connection: &Connection, secrets: &Secrets) -> Result<(), StorageError> {
    let current_version =
        connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;

    if current_version < 1 {
        connection.execute_batch(INITIAL_SCHEMA)?;
        connection.pragma_update(None, "user_version", 1)?;
    }

    let current_version =
        connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;

    if current_version < 2 {
        migrate_collection_node_sort_order(connection)?;
        connection.pragma_update(None, "user_version", 2)?;
    }

    let current_version =
        connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;

    if current_version < 3 {
        connection.execute_batch("BEGIN IMMEDIATE")?;
        match encrypt_existing_secrets(connection, secrets) {
            Ok(()) => {
                connection.pragma_update(None, "user_version", 3)?;
                connection.execute_batch("COMMIT")?;
            }
            Err(error) => {
                let _ = connection.execute_batch("ROLLBACK");
                return Err(error);
            }
        }
    }

    Ok(())
}

fn migrate_collection_node_sort_order(connection: &Connection) -> Result<(), StorageError> {
    let has_position = connection
        .prepare("SELECT position FROM collection_nodes LIMIT 0")
        .is_ok();
    let has_sort_order = connection
        .prepare("SELECT sort_order FROM collection_nodes LIMIT 0")
        .is_ok();

    if has_position && !has_sort_order {
        connection.execute_batch(
            "DROP INDEX IF EXISTS idx_collection_nodes_parent;
             ALTER TABLE collection_nodes RENAME COLUMN position TO sort_order;",
        )?;
        connection.execute(
            "UPDATE collection_nodes SET sort_order = (sort_order + 1) * ?",
            [SORT_ORDER_STEP],
        )?;
        connection.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_collection_nodes_parent
                ON collection_nodes(collection_id, parent_id, sort_order);",
        )?;
    }

    Ok(())
}

/// Encrypt existing sensitive plaintext values (variables + auth JSON blobs).
///
/// The operation is idempotent because [`Secrets::encrypt`] returns the input
/// unchanged when it is already a valid `enc:v1:` ciphertext.
fn encrypt_existing_secrets(
    connection: &Connection,
    secrets: &Secrets,
) -> Result<(), StorageError> {
    encrypt_sensitive_variables(connection, secrets)?;
    encrypt_auth_json_column(connection, secrets, "requests", "id")?;
    encrypt_auth_json_column(connection, secrets, "collections", "id")?;
    encrypt_auth_json_column(connection, secrets, "collection_nodes", "id")?;
    Ok(())
}

fn encrypt_sensitive_variables(
    connection: &Connection,
    secrets: &Secrets,
) -> Result<(), StorageError> {
    // Sensitive vars are keyed by (scope, collection_id/environment_id, key).
    let mut select = connection.prepare(
        "SELECT rowid, current_value, initial_value FROM variables WHERE sensitive = 1",
    )?;
    let rows = select
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    for (rowid, current, initial) in rows {
        let encrypted_current = secrets.encrypt(&current)?;
        let encrypted_initial = match initial {
            Some(value) => Some(secrets.encrypt(&value)?),
            None => None,
        };
        connection.execute(
            "UPDATE variables SET current_value = ?, initial_value = ? WHERE rowid = ?",
            params![encrypted_current, encrypted_initial, rowid],
        )?;
    }
    Ok(())
}

fn encrypt_auth_json_column(
    connection: &Connection,
    secrets: &Secrets,
    table: &str,
    id_column: &str,
) -> Result<(), StorageError> {
    let query = format!(
        "SELECT {id_column}, auth_json FROM {table} WHERE auth_json IS NOT NULL AND auth_json != ''"
    );
    let mut select = connection.prepare(&query)?;
    let rows = select
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let update_sql = format!("UPDATE {table} SET auth_json = ? WHERE {id_column} = ?");
    for (id, auth_json) in rows {
        let mut value = serde_json::from_str::<Value>(&auth_json).map_err(|error| {
            StorageError::InvalidInput(format!(
                "invalid auth_json in {table} id={id}: {error}"
            ))
        })?;
        let mut changed = false;
        for field in ["token", "password", "value"] {
            if let Some(existing) = value.get(field).and_then(Value::as_str) {
                if !existing.is_empty() && !secrets::is_encrypted(existing) {
                    let encrypted = secrets.encrypt(existing)?;
                    value[field] = Value::String(encrypted);
                    changed = true;
                }
            }
        }
        if changed {
            connection.execute(&update_sql, params![value.to_string(), id])?;
        }
    }
    Ok(())
}
