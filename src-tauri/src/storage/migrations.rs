use rusqlite::Connection;

use super::StorageError;

const SCHEMA_VERSION: i64 = 1;
const INITIAL_SCHEMA: &str = include_str!("schema/001_initial.sql");

pub fn run(connection: &Connection) -> Result<(), StorageError> {
    let current_version =
        connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;

    if current_version < 1 {
        connection.execute_batch(INITIAL_SCHEMA)?;
        connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }

    Ok(())
}
