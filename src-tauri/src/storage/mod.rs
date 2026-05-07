mod connection;
mod migrations;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rusqlite::{params, Connection};
use serde::Serialize;
use thiserror::Error;

#[derive(Clone)]
pub struct Database {
    connection: Arc<Mutex<Connection>>,
    path: PathBuf,
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("failed to create app data directory at {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to open sqlite database at {path}: {source}")]
    OpenDatabase {
        path: PathBuf,
        source: rusqlite::Error,
    },
    #[error("failed to access sqlite connection")]
    ConnectionPoisoned,
    #[error("sqlite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("cannot move a folder into itself or one of its descendants")]
    InvalidTreeMove,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStatus {
    pub path: String,
    pub schema_version: i64,
    pub collection_count: i64,
    pub request_count: i64,
}

impl Database {
    pub fn open(app_data_dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        let (connection, path) = connection::open(app_data_dir)?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            path,
        })
    }

    pub fn status(&self) -> Result<DatabaseStatus, StorageError> {
        let connection = self.connection()?;
        let schema_version =
            connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;
        let collection_count = count_rows(&connection, "collections")?;
        let request_count = count_rows(&connection, "requests")?;

        Ok(DatabaseStatus {
            path: self.path.display().to_string(),
            schema_version,
            collection_count,
            request_count,
        })
    }

    pub fn with_connection<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let mut connection = self.connection()?;
        operation(&mut connection)
    }

    fn connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        self.connection
            .lock()
            .map_err(|_| StorageError::ConnectionPoisoned)
    }
}

fn count_rows(connection: &Connection, table_name: &str) -> Result<i64, rusqlite::Error> {
    connection.query_row(
        &format!("SELECT COUNT(*) FROM {table_name}"),
        params![],
        |row| row.get(0),
    )
}
