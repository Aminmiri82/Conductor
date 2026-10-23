mod import;
mod tree;

pub use import::import_postman_collection;
#[cfg(test)]
pub(crate) use tree::{add_folder, read_tree};
pub use tree::{create_folder, delete_node, get_collection_tree, move_node, rename_folder};
pub(crate) use tree::{sort_order_after, sort_order_for_position};

use chrono::Utc;
use rusqlite::Connection;
use tauri::State;
use uuid::Uuid;

use super::{
    error::CommandError,
    models::{CollectionStats, CollectionSummary, ImportedCollection},
};
use crate::{storage::StorageError, AppState};

#[tauri::command(async)]
#[specta::specta]
pub fn list_collections(
    state: State<'_, AppState>,
) -> Result<Vec<CollectionSummary>, CommandError> {
    Ok(state.database.with_read_connection(read_collections)?)
}

/// Creates an empty collection, so a new user can start without a Postman file.
/// Returns the same shape as an import so the sidebar can switch to it at once.
#[tauri::command(async)]
#[specta::specta]
pub fn create_collection(
    name: String,
    state: State<'_, AppState>,
) -> Result<ImportedCollection, CommandError> {
    let collection_id = state
        .database
        .with_connection(|connection| add_collection(connection, &name))?;
    Ok(state.database.with_read_connection(|connection| {
        Ok(ImportedCollection {
            collections: read_collections(connection)?,
            tree: Vec::new(),
            collection_id,
        })
    })?)
}

fn add_collection(connection: &Connection, name: &str) -> Result<String, StorageError> {
    let name = match name.trim() {
        "" => "New collection",
        trimmed => trimmed,
    };
    let collection_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    connection.execute(
        "INSERT INTO collections (id, name, auth_json, created_at, updated_at)
         VALUES (?, ?, NULL, ?, ?)",
        rusqlite::params![collection_id, name, now, now],
    )?;
    Ok(collection_id)
}

#[tauri::command(async)]
#[specta::specta]
pub fn list_collection_stats(
    state: State<'_, AppState>,
) -> Result<Vec<CollectionStats>, CommandError> {
    Ok(state.database.with_read_connection(read_collection_stats)?)
}

fn read_collection_stats(connection: &Connection) -> Result<Vec<CollectionStats>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT id, name,
                (SELECT COUNT(*) FROM collection_nodes
                 WHERE collection_id = collections.id AND kind = 'request'),
                (SELECT COUNT(*) FROM collection_nodes
                 WHERE collection_id = collections.id AND kind = 'folder'),
                (SELECT COUNT(*) FROM environments
                 WHERE collection_id = collections.id)
         FROM collections ORDER BY updated_at DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(CollectionStats {
            id: row.get(0)?,
            name: row.get(1)?,
            request_count: row.get(2)?,
            folder_count: row.get(3)?,
            environment_count: row.get(4)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::from)
}

/// Deletes a collection and, through the schema's cascades, its folders,
/// requests, environments, and variables. Returns the remaining collections.
#[tauri::command(async)]
#[specta::specta]
pub fn delete_collection(
    collection_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<CollectionSummary>, CommandError> {
    Ok(state.database.with_connection(|connection| {
        remove_collection(connection, &collection_id)?;
        read_collections(connection)
    })?)
}

fn remove_collection(connection: &Connection, collection_id: &str) -> Result<(), StorageError> {
    connection.execute(
        "DELETE FROM collections WHERE id = ?",
        rusqlite::params![collection_id],
    )?;
    Ok(())
}

fn read_collections(connection: &Connection) -> Result<Vec<CollectionSummary>, StorageError> {
    let mut statement =
        connection.prepare("SELECT id, name FROM collections ORDER BY updated_at DESC")?;
    let rows = statement.query_map([], |row| {
        Ok(CollectionSummary {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fixtures, storage::test_connection};

    #[test]
    fn a_new_user_can_create_a_collection_and_add_requests_to_it() {
        let mut connection = test_connection();

        let collection = add_collection(&connection, "  My API  ").unwrap();
        fixtures::request(&mut connection, &collection, None);

        let collections = read_collections(&connection).unwrap();
        assert_eq!(collections.len(), 1);
        assert_eq!(collections[0].id, collection);
        assert_eq!(collections[0].name, "My API");
        assert_eq!(read_tree(&connection, &collection).unwrap().len(), 1);
    }

    #[test]
    fn deleting_a_collection_removes_everything_in_it_and_nothing_else() {
        let mut connection = test_connection();
        let doomed = add_collection(&connection, "Doomed").unwrap();
        let kept = add_collection(&connection, "Kept").unwrap();
        let folder = fixtures::folder(&mut connection, &doomed, None);
        fixtures::request(&mut connection, &doomed, Some(&folder));
        fixtures::environment(&connection, &doomed);
        fixtures::request(&mut connection, &kept, None);
        fixtures::environment(&connection, &kept);

        remove_collection(&connection, &doomed).unwrap();

        let collections = read_collections(&connection).unwrap();
        assert_eq!(collections.len(), 1);
        assert_eq!(collections[0].id, kept);
        let count = |sql: &str| -> i64 {
            connection
                .query_row(sql, rusqlite::params![doomed], |row| row.get(0))
                .unwrap()
        };
        assert_eq!(
            count("SELECT COUNT(*) FROM requests WHERE collection_id = ?"),
            0
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM collection_nodes WHERE collection_id = ?"),
            0
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM environments WHERE collection_id = ?"),
            0
        );
        assert_eq!(read_tree(&connection, &kept).unwrap().len(), 1);
    }

    #[test]
    fn collection_stats_count_requests_folders_and_environments_per_collection() {
        let mut connection = test_connection();
        let busy = add_collection(&connection, "Busy").unwrap();
        let empty = add_collection(&connection, "Empty").unwrap();
        let folder = fixtures::folder(&mut connection, &busy, None);
        fixtures::folder(&mut connection, &busy, Some(&folder));
        fixtures::request(&mut connection, &busy, None);
        fixtures::request(&mut connection, &busy, Some(&folder));
        fixtures::request(&mut connection, &busy, Some(&folder));
        fixtures::environment(&connection, &busy);

        let stats = read_collection_stats(&connection).unwrap();
        let of = |id: &str| stats.iter().find(|stat| stat.id == id).unwrap();

        assert_eq!(
            (
                of(&busy).request_count,
                of(&busy).folder_count,
                of(&busy).environment_count
            ),
            (3, 2, 1)
        );
        assert_eq!(
            (
                of(&empty).request_count,
                of(&empty).folder_count,
                of(&empty).environment_count
            ),
            (0, 0, 0)
        );
    }

    #[test]
    fn a_blank_collection_name_falls_back_to_a_default() {
        let connection = test_connection();

        add_collection(&connection, "   ").unwrap();

        assert_eq!(
            read_collections(&connection).unwrap()[0].name,
            "New collection"
        );
    }
}
