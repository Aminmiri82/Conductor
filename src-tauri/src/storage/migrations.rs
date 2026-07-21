use rusqlite::Connection;

use super::StorageError;

const INITIAL_SCHEMA: &str = include_str!("schema/001_initial.sql");
const SORT_ORDER_STEP: i64 = 1024;
const SCHEMA_VERSION: i64 = 5;

pub fn run(connection: &Connection) -> Result<(), StorageError> {
    let current_version =
        connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;

    if current_version > 0 && current_version < SCHEMA_VERSION {
        reset_schema(connection)?;
        connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        return Ok(());
    }

    if current_version < 1 {
        connection.execute_batch(INITIAL_SCHEMA)?;
        connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }

    let current_version =
        connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;

    if current_version < 2 {
        migrate_collection_node_sort_order(connection)?;
        connection.pragma_update(None, "user_version", 2)?;
    }

    Ok(())
}

fn reset_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "DROP TABLE IF EXISTS request_history;
         DROP TABLE IF EXISTS learned_index_models;
         DROP TABLE IF EXISTS workspace_state;
         DROP TABLE IF EXISTS variables;
         DROP TABLE IF EXISTS environments;
         DROP TABLE IF EXISTS collection_nodes;
         DROP TABLE IF EXISTS requests;
         DROP TABLE IF EXISTS collections;",
    )?;
    connection.execute_batch(INITIAL_SCHEMA)?;
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
