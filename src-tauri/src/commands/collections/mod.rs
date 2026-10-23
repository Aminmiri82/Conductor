mod import;
mod tree;

pub use import::import_postman_collection;
#[cfg(test)]
pub(crate) use tree::{add_folder, read_tree};
pub use tree::{create_folder, delete_node, get_collection_tree, move_node};
pub(crate) use tree::{sort_order_after, sort_order_for_position};

use chrono::Utc;
use rusqlite::Connection;
use tauri::State;
use uuid::Uuid;

use super::{
    error::CommandError,
    models::{CollectionSummary, ImportedCollection},
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
    fn a_blank_collection_name_falls_back_to_a_default() {
        let connection = test_connection();

        add_collection(&connection, "   ").unwrap();

        assert_eq!(
            read_collections(&connection).unwrap()[0].name,
            "New collection"
        );
    }
}
