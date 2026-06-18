use rusqlite::Connection;

use super::StorageError;

const INITIAL_SCHEMA: &str = include_str!("schema/001_initial.sql");
const SORT_ORDER_STEP: i64 = 1024;

pub fn run(connection: &Connection) -> Result<(), StorageError> {
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
