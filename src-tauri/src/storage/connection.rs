use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::Connection;

use super::{migrations, StorageError};

const DATABASE_FILE_NAME: &str = "conductor.sqlite3";

pub fn open(app_data_dir: impl AsRef<Path>) -> Result<(Connection, PathBuf), StorageError> {
    let app_data_dir = app_data_dir.as_ref();
    fs::create_dir_all(app_data_dir).map_err(|source| StorageError::CreateDirectory {
        path: app_data_dir.to_path_buf(),
        source,
    })?;

    let path = app_data_dir.join(DATABASE_FILE_NAME);
    let connection = Connection::open(&path).map_err(|source| StorageError::OpenDatabase {
        path: path.clone(),
        source,
    })?;

    configure(&connection)?;
    migrations::run(&connection)?;

    Ok((connection, path))
}

fn configure(connection: &Connection) -> Result<(), StorageError> {
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.pragma_update(None, "busy_timeout", 5_000)?;

    Ok(())
}
