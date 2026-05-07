use rusqlite::Connection;

use super::StorageError;

const INITIAL_SCHEMA: &str = include_str!("schema/001_initial.sql");
const REQUEST_METADATA_SCHEMA: &str = include_str!("schema/002_request_metadata.sql");
const AUTH_INHERITANCE_SCHEMA: &str = include_str!("schema/003_auth_inheritance.sql");

pub fn run(connection: &Connection) -> Result<(), StorageError> {
    let current_version =
        connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;

    if current_version < 1 {
        connection.execute_batch(INITIAL_SCHEMA)?;
        connection.pragma_update(None, "user_version", 1)?;
    }

    if current_version < 2 {
        connection.execute_batch(REQUEST_METADATA_SCHEMA)?;
        connection.pragma_update(None, "user_version", 2)?;
    }

    if current_version < 3 {
        connection.execute_batch(AUTH_INHERITANCE_SCHEMA)?;
        connection.pragma_update(None, "user_version", 3)?;
    }

    Ok(())
}
