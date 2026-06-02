use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use tauri::State;
use uuid::Uuid;

use crate::commands::models::{CreateFolderInput, MoveNodeInput};
use crate::{storage::StorageError, AppState};

#[tauri::command]
pub fn create_folder(
    input: CreateFolderInput,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let now = Utc::now().to_rfc3339();
    let node_id = Uuid::new_v4().to_string();

    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            shift_node_positions(
                &tx,
                &input.collection_id,
                input.parent_id.as_deref(),
                input.position,
            )?;
            tx.execute(
                "INSERT INTO collection_nodes
                 (id, collection_id, parent_id, position, kind, name, request_id, auth_json,
                  created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'folder', ?, NULL, NULL, ?, ?)",
                params![
                    node_id,
                    input.collection_id,
                    input.parent_id,
                    input.position,
                    input.name,
                    now,
                    now
                ],
            )?;
            tx.commit()?;
            Ok(node_id.clone())
        })
        .map_err(|error| error.to_string())
}
#[tauri::command]
pub fn delete_node(node_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            let request_ids = collect_request_ids_for_node(&tx, &node_id)?;
            tx.execute(
                "DELETE FROM collection_nodes WHERE id = ?",
                params![node_id],
            )?;
            for request_id in request_ids {
                tx.execute("DELETE FROM requests WHERE id = ?", params![request_id])?;
            }
            tx.commit()?;
            Ok(())
        })
        .map_err(|error| error.to_string())
}
#[tauri::command]
pub fn move_node(input: MoveNodeInput, state: State<'_, AppState>) -> Result<(), String> {
    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            let (collection_id, old_parent_id, old_position, kind): (
                String,
                Option<String>,
                i64,
                String,
            ) = tx.query_row(
                "SELECT collection_id, parent_id, position, kind FROM collection_nodes WHERE id = ?",
                params![input.node_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;

            if kind == "folder" {
                let invalid_target = input.parent_id.as_deref() == Some(input.node_id.as_str())
                    || input
                        .parent_id
                        .as_deref()
                        .map(|parent_id| is_descendant(&tx, parent_id, &input.node_id))
                        .transpose()?
                        .unwrap_or(false);
                if invalid_target {
                    return Err(StorageError::InvalidTreeMove);
                }
            }

            close_position_gap(&tx, &collection_id, old_parent_id.as_deref(), old_position)?;
            open_position_gap(&tx, &collection_id, input.parent_id.as_deref(), input.position)?;
            tx.execute(
                "UPDATE collection_nodes SET parent_id = ?, position = ?, updated_at = ? WHERE id = ?",
                params![input.parent_id, input.position, Utc::now().to_rfc3339(), input.node_id],
            )?;
            tx.commit()?;
            Ok(())
        })
        .map_err(|error| error.to_string())
}
pub(super) fn shift_node_positions(
    tx: &rusqlite::Transaction<'_>,
    collection_id: &str,
    parent_id: Option<&str>,
    from_position: i64,
) -> Result<(), StorageError> {
    match parent_id {
        Some(parent_id) => {
            tx.execute(
                "UPDATE collection_nodes
                 SET position = position + 1
                 WHERE collection_id = ? AND parent_id = ? AND position >= ?",
                params![collection_id, parent_id, from_position],
            )?;
        }
        None => {
            tx.execute(
                "UPDATE collection_nodes
                 SET position = position + 1
                 WHERE collection_id = ? AND parent_id IS NULL AND position >= ?",
                params![collection_id, from_position],
            )?;
        }
    }
    Ok(())
}
fn close_position_gap(
    tx: &rusqlite::Transaction<'_>,
    collection_id: &str,
    parent_id: Option<&str>,
    old_position: i64,
) -> Result<(), StorageError> {
    match parent_id {
        Some(parent_id) => {
            tx.execute(
                "UPDATE collection_nodes
                 SET position = position - 1
                 WHERE collection_id = ? AND parent_id = ? AND position > ?",
                params![collection_id, parent_id, old_position],
            )?;
        }
        None => {
            tx.execute(
                "UPDATE collection_nodes
                 SET position = position - 1
                 WHERE collection_id = ? AND parent_id IS NULL AND position > ?",
                params![collection_id, old_position],
            )?;
        }
    }
    Ok(())
}
fn open_position_gap(
    tx: &rusqlite::Transaction<'_>,
    collection_id: &str,
    parent_id: Option<&str>,
    new_position: i64,
) -> Result<(), StorageError> {
    shift_node_positions(tx, collection_id, parent_id, new_position)
}
fn collect_request_ids_for_node(
    tx: &rusqlite::Transaction<'_>,
    node_id: &str,
) -> Result<Vec<String>, StorageError> {
    let mut request_ids = Vec::new();
    let mut stack = vec![node_id.to_string()];

    while let Some(id) = stack.pop() {
        let request_id: Option<String> = tx.query_row(
            "SELECT request_id FROM collection_nodes WHERE id = ?",
            params![id],
            |row| row.get(0),
        )?;
        if let Some(request_id) = request_id {
            request_ids.push(request_id);
        }

        let mut statement = tx.prepare("SELECT id FROM collection_nodes WHERE parent_id = ?")?;
        let child_ids = statement
            .query_map(params![id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        stack.extend(child_ids);
    }

    Ok(request_ids)
}
fn is_descendant(
    tx: &rusqlite::Transaction<'_>,
    possible_descendant_id: &str,
    ancestor_id: &str,
) -> Result<bool, StorageError> {
    let mut current = Some(possible_descendant_id.to_string());
    while let Some(id) = current {
        if id == ancestor_id {
            return Ok(true);
        }
        current = tx
            .query_row(
                "SELECT parent_id FROM collection_nodes WHERE id = ?",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
    }
    Ok(false)
}
