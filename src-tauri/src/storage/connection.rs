use std::{
    fs,
    path::{Path, PathBuf},
};

use rusqlite::Connection;

use super::{migrations, Secrets, StorageError};

const DATABASE_FILE_NAME: &str = "conductor.sqlite3";
const READ_CONNECTIONS: usize = 4;

pub fn open(
    app_data_dir: impl AsRef<Path>,
    secrets: &Secrets,
) -> Result<(Connection, Vec<Connection>, PathBuf), StorageError> {
    let app_data_dir = app_data_dir.as_ref();
    fs::create_dir_all(app_data_dir).map_err(|source| StorageError::CreateDirectory {
        path: app_data_dir.to_path_buf(),
        source,
    })?;

    let path = app_data_dir.join(DATABASE_FILE_NAME);
    let connection = open_connection(&path)?;

    configure(&connection)?;
    migrations::run(&connection, secrets)?;
    let mut read_connections = Vec::with_capacity(READ_CONNECTIONS);
    for _ in 0..READ_CONNECTIONS {
        let read_connection = open_connection(&path)?;
        configure(&read_connection)?;
        read_connection.pragma_update(None, "query_only", "ON")?;
        read_connections.push(read_connection);
    }

    Ok((connection, read_connections, path))
}

fn open_connection(path: &Path) -> Result<Connection, StorageError> {
    Connection::open(path).map_err(|source| StorageError::OpenDatabase {
        path: path.to_path_buf(),
        source,
    })
}

fn configure(connection: &Connection) -> Result<(), StorageError> {
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.pragma_update(None, "busy_timeout", 5_000)?;

    Ok(())
}
