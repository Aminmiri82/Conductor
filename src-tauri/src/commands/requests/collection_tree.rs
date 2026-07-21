use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use tauri::State;
use uuid::Uuid;

use crate::commands::models::{CreateFolderInput, MoveNodeInput};
use crate::commands::{AppError, AppResult};
use crate::{storage::StorageError, AppState};

const SORT_ORDER_STEP: i64 = 1024;

#[tauri::command]
pub fn create_folder(
    input: CreateFolderInput,
    state: State<'_, AppState>,
) -> AppResult<String> {
    let now = Utc::now().to_rfc3339();
    let node_id = Uuid::new_v4().to_string();

    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            let sort_order = sort_order_for_position(
                &tx,
                &input.collection_id,
                input.parent_id.as_deref(),
                input.position,
                None,
            )?;
            tx.execute(
                "INSERT INTO collection_nodes
                 (id, collection_id, parent_id, sort_order, kind, name, request_id, auth_json,
                  created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'folder', ?, NULL, NULL, ?, ?)",
                params![
                    node_id,
                    input.collection_id,
                    input.parent_id,
                    sort_order,
                    input.name,
                    now,
                    now
                ],
            )?;
            tx.commit()?;
            Ok(node_id.clone())
        })
        .map_err(AppError::from)
}
#[tauri::command]
pub fn delete_node(node_id: String, state: State<'_, AppState>) -> AppResult<()> {
    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            let request_ids = collect_request_ids_for_subtree(&tx, &node_id)?;
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
        .map_err(AppError::from)
}
#[tauri::command]
pub fn move_node(input: MoveNodeInput, state: State<'_, AppState>) -> AppResult<()> {
    state
        .database
        .with_connection(|connection| {
            let tx = connection.transaction()?;
            let (collection_id, kind): (String, String) = tx.query_row(
                "SELECT collection_id, kind FROM collection_nodes WHERE id = ?",
                params![input.node_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
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

            let sort_order = sort_order_for_position(
                &tx,
                &collection_id,
                input.parent_id.as_deref(),
                input.position,
                Some(input.node_id.as_str()),
            )?;
            tx.execute(
                "UPDATE collection_nodes SET parent_id = ?, sort_order = ?, updated_at = ? WHERE id = ?",
                params![input.parent_id, sort_order, Utc::now().to_rfc3339(), input.node_id],
            )?;
            tx.commit()?;
            Ok(())
        })
        .map_err(AppError::from)
}
pub(super) fn sort_order_for_position(
    tx: &rusqlite::Transaction<'_>,
    collection_id: &str,
    parent_id: Option<&str>,
    position: i64,
    exclude_node_id: Option<&str>,
) -> Result<i64, StorageError> {
    let orders = sibling_sort_orders(tx, collection_id, parent_id, exclude_node_id)?;
    let position = position.clamp(0, orders.len() as i64) as usize;
    let previous = position
        .checked_sub(1)
        .and_then(|index| orders.get(index).copied());
    let next = orders.get(position).copied();

    if let Some(sort_order) = order_between(previous, next) {
        return Ok(sort_order);
    }

    rebalance_siblings(tx, collection_id, parent_id, exclude_node_id)?;
    let orders = sibling_sort_orders(tx, collection_id, parent_id, exclude_node_id)?;
    let position = position.min(orders.len());
    let previous = position
        .checked_sub(1)
        .and_then(|index| orders.get(index).copied());
    let next = orders.get(position).copied();
    order_between(previous, next).ok_or_else(|| {
        StorageError::InvalidInput("unable to allocate collection node order".to_string())
    })
}

pub(super) fn sort_order_after(
    tx: &rusqlite::Transaction<'_>,
    collection_id: &str,
    parent_id: Option<&str>,
    after_sort_order: i64,
) -> Result<i64, StorageError> {
    let position =
        sibling_count_through_sort_order(tx, collection_id, parent_id, after_sort_order)?;
    sort_order_for_position(tx, collection_id, parent_id, position, None)
}

fn order_between(previous: Option<i64>, next: Option<i64>) -> Option<i64> {
    match (previous, next) {
        (None, None) => Some(SORT_ORDER_STEP),
        (Some(previous), None) => Some(previous + SORT_ORDER_STEP),
        (None, Some(next)) if next > 1 => Some(next / 2),
        (Some(previous), Some(next)) if next - previous > 1 => {
            Some(previous + (next - previous) / 2)
        }
        _ => None,
    }
}

fn sibling_sort_orders(
    tx: &rusqlite::Transaction<'_>,
    collection_id: &str,
    parent_id: Option<&str>,
    exclude_node_id: Option<&str>,
) -> Result<Vec<i64>, StorageError> {
    let mut statement = tx.prepare(
        "SELECT sort_order
         FROM collection_nodes
         WHERE collection_id = ?
           AND parent_id IS ?
           AND (? IS NULL OR id != ?)
         ORDER BY sort_order",
    )?;
    let rows = statement.query_map(
        params![collection_id, parent_id, exclude_node_id, exclude_node_id],
        |row| row.get::<_, i64>(0),
    )?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::from)
}

fn sibling_count_through_sort_order(
    tx: &rusqlite::Transaction<'_>,
    collection_id: &str,
    parent_id: Option<&str>,
    sort_order: i64,
) -> Result<i64, StorageError> {
    tx.query_row(
        "SELECT COUNT(*)
         FROM collection_nodes
         WHERE collection_id = ?
           AND parent_id IS ?
           AND sort_order <= ?",
        params![collection_id, parent_id, sort_order],
        |row| row.get(0),
    )
    .map_err(StorageError::from)
}

fn rebalance_siblings(
    tx: &rusqlite::Transaction<'_>,
    collection_id: &str,
    parent_id: Option<&str>,
    exclude_node_id: Option<&str>,
) -> Result<(), StorageError> {
    let mut statement = tx.prepare(
        "SELECT id
         FROM collection_nodes
         WHERE collection_id = ?
           AND parent_id IS ?
           AND (? IS NULL OR id != ?)
         ORDER BY sort_order",
    )?;
    let sibling_ids = statement
        .query_map(
            params![collection_id, parent_id, exclude_node_id, exclude_node_id],
            |row| row.get::<_, String>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;

    let mut update = tx.prepare("UPDATE collection_nodes SET sort_order = ? WHERE id = ?")?;
    for (index, id) in sibling_ids.iter().enumerate() {
        update.execute(params![(index as i64 + 1) * SORT_ORDER_STEP, id])?;
    }

    Ok(())
}

fn collect_request_ids_for_subtree(
    tx: &rusqlite::Transaction<'_>,
    node_id: &str,
) -> Result<Vec<String>, StorageError> {
    let mut statement = tx.prepare(
        "WITH RECURSIVE subtree(id, request_id) AS (
             SELECT id, request_id FROM collection_nodes WHERE id = ?
             UNION ALL
             SELECT child.id, child.request_id
             FROM collection_nodes child
             JOIN subtree ON child.parent_id = subtree.id
         )
         SELECT request_id FROM subtree WHERE request_id IS NOT NULL",
    )?;
    let rows = statement.query_map(params![node_id], |row| row.get::<_, String>(0))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::from)
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
