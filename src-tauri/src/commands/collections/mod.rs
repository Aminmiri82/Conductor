mod import;
mod tree;

pub use import::import_postman_collection;
#[cfg(test)]
pub(crate) use tree::{add_folder, read_tree};
pub use tree::{create_folder, delete_node, get_collection_tree, move_node};
pub(crate) use tree::{sort_order_after, sort_order_for_position};

use tauri::State;

use super::{error::CommandError, models::CollectionSummary};
use crate::{storage::StorageError, AppState};

#[tauri::command(async)]
#[specta::specta]
pub fn list_collections(
    state: State<'_, AppState>,
) -> Result<Vec<CollectionSummary>, CommandError> {
    Ok(state.database.with_read_connection(|connection| {
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
    })?)
}
