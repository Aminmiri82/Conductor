mod connection;
mod migrations;
pub mod secrets;

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use rusqlite::{params, Connection};
use serde::Serialize;
use thiserror::Error;

pub use secrets::{Secrets, SecretsError};

#[derive(Clone)]
pub struct Database {
    write_connection: Arc<Mutex<Connection>>,
    read_connections: Arc<Vec<Mutex<Connection>>>,
    next_read_connection: Arc<AtomicUsize>,
    path: PathBuf,
    raw_import_dir: PathBuf,
    secrets: Secrets,
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
    #[error("json operation failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("file operation failed at {path}: {source}")]
    FileOperation {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot move a folder into itself or one of its descendants")]
    InvalidTreeMove,
    #[error("{0}")]
    InvalidInput(String),
    #[error("secrets: {0}")]
    Secrets(#[from] SecretsError),
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
        let app_data_dir = app_data_dir.as_ref();
        let raw_import_dir = app_data_dir.join("raw-imports");
        let secrets = Secrets::init(app_data_dir)?;
        let (write_connection, read_connections, path) =
            connection::open(app_data_dir, &secrets)?;

        let database = Self {
            write_connection: Arc::new(Mutex::new(write_connection)),
            read_connections: Arc::new(read_connections.into_iter().map(Mutex::new).collect()),
            next_read_connection: Arc::new(AtomicUsize::new(0)),
            path,
            raw_import_dir,
            secrets,
        };

        database.purge_expired_request_history()?;
        Ok(database)
    }

    pub fn secrets(&self) -> &Secrets {
        &self.secrets
    }

    /// Delete request history entries older than the retention window.
    pub fn purge_expired_request_history(&self) -> Result<usize, StorageError> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(21);
        let cutoff = cutoff.to_rfc3339();
        self.with_connection(|connection| {
            let deleted = connection.execute(
                "DELETE FROM request_history WHERE created_at < ?",
                params![cutoff],
            )?;
            Ok(deleted)
        })
    }

    pub fn status(&self) -> Result<DatabaseStatus, StorageError> {
        let connection = self.read_connection()?;
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
        let mut connection = self.write_connection()?;
        operation(&mut connection)
    }

    pub fn with_read_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let connection = self.read_connection()?;
        operation(&connection)
    }

    pub fn raw_import_dir(&self) -> &Path {
        &self.raw_import_dir
    }

    fn write_connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        self.write_connection
            .lock()
            .map_err(|_| StorageError::ConnectionPoisoned)
    }

    fn read_connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        let index =
            self.next_read_connection.fetch_add(1, Ordering::Relaxed) % self.read_connections.len();
        self.read_connections[index]
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
