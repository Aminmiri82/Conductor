use std::collections::HashMap;

use chrono::Utc;
use rusqlite::params;
use tauri::State;

use crate::commands::models::{CollectionNode, CollectionSummary, RenameCollectionInput};
use crate::commands::{AppError, AppResult};
use crate::storage::StorageError;
use crate::AppState;

#[tauri::command]
pub fn list_collections(state: State<'_, AppState>) -> AppResult<Vec<CollectionSummary>> {
    Ok(state.database.with_read_connection(|connection| {
        let mut statement = connection.prepare(
            "SELECT id, name, source, updated_at FROM collections ORDER BY updated_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(CollectionSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                source: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    })?)
}

#[tauri::command]
pub fn get_collection_tree(
    collection_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<CollectionNode>> {
    Ok(state.database.with_read_connection(|connection| {
        let mut statement = connection.prepare(
            "SELECT n.id, n.collection_id, n.parent_id, n.kind, n.name, n.request_id,
                    r.method
             FROM collection_nodes n
             LEFT JOIN requests r ON r.id = n.request_id
             WHERE n.collection_id = ?
             ORDER BY n.parent_id, n.sort_order",
        )?;
        let rows = statement.query_map(params![collection_id], |row| {
            Ok(FlatNode {
                id: row.get(0)?,
                collection_id: row.get(1)?,
                parent_id: row.get(2)?,
                kind: row.get(3)?,
                name: row.get(4)?,
                request_id: row.get(5)?,
                method: row.get(6)?,
            })
        })?;
        let nodes = rows.collect::<Result<Vec<_>, _>>()?;
        Ok(build_tree(nodes, None))
    })?)
}

#[tauri::command]
pub fn rename_collection(
    input: RenameCollectionInput,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let trimmed = input.name.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("collection name cannot be empty"));
    }
    let now = Utc::now().to_rfc3339();
    let collection_id = input.collection_id.clone();
    state
        .database
        .with_connection(|connection| {
            let changed = connection.execute(
                "UPDATE collections SET name = ?, updated_at = ? WHERE id = ?",
                params![trimmed, now, collection_id],
            )?;
            if changed == 0 {
                return Err(StorageError::InvalidInput(
                    "collection not found".to_string(),
                ));
            }
            Ok(())
        })
        .map_err(AppError::from)
}

#[tauri::command]
pub fn delete_collection(collection_id: String, state: State<'_, AppState>) -> AppResult<()> {
    // FKs cascade to `requests`, `collection_nodes`, and collection-scoped
    // variables. Environment variables are unaffected.
    state
        .database
        .with_connection(|connection| {
            connection.execute(
                "DELETE FROM collections WHERE id = ?",
                params![collection_id],
            )?;
            Ok(())
        })
        .map_err(AppError::from)
}

pub(crate) struct FlatNode {
    pub id: String,
    pub collection_id: String,
    pub parent_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub request_id: Option<String>,
    pub method: Option<String>,
}

fn build_tree(nodes: Vec<FlatNode>, parent_id: Option<&str>) -> Vec<CollectionNode> {
    let mut by_parent: HashMap<Option<String>, Vec<FlatNode>> = HashMap::new();
    for node in nodes {
        by_parent
            .entry(node.parent_id.clone())
            .or_default()
            .push(node);
    }
    build_tree_from_map(&mut by_parent, parent_id.map(ToString::to_string))
}

fn build_tree_from_map(
    by_parent: &mut HashMap<Option<String>, Vec<FlatNode>>,
    parent_id: Option<String>,
) -> Vec<CollectionNode> {
    let nodes = by_parent.remove(&parent_id).unwrap_or_default();
    nodes
        .into_iter()
        .enumerate()
        .map(|(position, node)| {
            let children = build_tree_from_map(by_parent, Some(node.id.clone()));
            CollectionNode {
                id: node.id,
                collection_id: node.collection_id,
                parent_id: node.parent_id,
                position: position as i64,
                kind: node.kind,
                name: node.name,
                request_id: node.request_id,
                method: node.method,
                children,
            }
        })
        .collect()
}
