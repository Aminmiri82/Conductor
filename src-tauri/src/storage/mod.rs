mod connection;
mod migrations;
mod rows;

pub use rows::{
    insert_node, insert_request, upsert_variable, NewNode, NewRequest, NewVariable, NodeKind,
};

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};

use rusqlite::Connection;
use thiserror::Error;

pub struct Database {
    write_connection: Mutex<Connection>,
    read_connections: Vec<Mutex<Connection>>,
    next_read_connection: AtomicUsize,
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
    #[error("cannot move a folder into itself or one of its descendants")]
    InvalidTreeMove,
    #[error("{0}")]
    InvalidInput(String),
}

impl Database {
    pub fn open(app_data_dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        let (write_connection, read_connections) = connection::open(app_data_dir)?;

        Ok(Self {
            write_connection: Mutex::new(write_connection),
            read_connections: read_connections.into_iter().map(Mutex::new).collect(),
            next_read_connection: AtomicUsize::new(0),
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

/// An in-memory database with the real schema, for tests.
#[cfg(test)]
pub fn test_connection() -> Connection {
    let connection = Connection::open_in_memory().expect("open in-memory database");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("enable foreign keys");
    migrations::run(&connection).expect("apply schema");
    connection
}
