use std::collections::HashMap;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use tauri::State;
use uuid::Uuid;

use crate::commands::{
    error::CommandError,
    models::{CollectionNode, CollectionNodeKind, CreateFolderInput, MoveNodeInput},
};
use crate::storage::{insert_node, NewNode, NodeKind, StorageError};
use crate::AppState;

const SORT_ORDER_STEP: i64 = 1024;

/// The `sort_order` of the node at `index` among evenly spaced siblings.
pub(crate) fn sort_order_at(index: usize) -> i64 {
    (index as i64 + 1) * SORT_ORDER_STEP
}

#[derive(Debug)]
struct FlatNode {
    id: String,
    parent_id: Option<String>,
    name: String,
    request: Option<FlatRequest>,
}

#[derive(Debug)]
struct FlatRequest {
    request_id: String,
    method: String,
}

#[tauri::command(async)]
#[specta::specta]
pub fn get_collection_tree(
    collection_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<CollectionNode>, CommandError> {
    Ok(state
        .database
        .with_read_connection(|connection| read_tree(connection, &collection_id))?)
}

pub(crate) fn read_tree(
    connection: &Connection,
    collection_id: &str,
) -> Result<Vec<CollectionNode>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT n.id, n.parent_id, n.name, n.request_id, r.method
         FROM collection_nodes n
         LEFT JOIN requests r ON r.id = n.request_id
         WHERE n.collection_id = ?
         ORDER BY n.parent_id, n.sort_order",
    )?;
    let rows = statement.query_map(params![collection_id], |row| {
        // The schema gives exactly the request nodes a `request_id`, and its
        // foreign key guarantees the request row (and so its method) exists.
        let request = row
            .get::<_, Option<String>>(3)?
            .map(|request_id| {
                Ok::<_, rusqlite::Error>(FlatRequest {
                    request_id,
                    method: row.get(4)?,
                })
            })
            .transpose()?;
        Ok(FlatNode {
            id: row.get(0)?,
            parent_id: row.get(1)?,
            name: row.get(2)?,
            request,
        })
    })?;
    let nodes = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(build_tree(nodes))
}

fn build_tree(nodes: Vec<FlatNode>) -> Vec<CollectionNode> {
    let mut by_parent: HashMap<Option<String>, Vec<FlatNode>> = HashMap::new();
    for node in nodes {
        by_parent
            .entry(node.parent_id.clone())
            .or_default()
            .push(node);
    }
    build_tree_from_map(&mut by_parent, None)
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
            let kind = match node.request {
                Some(FlatRequest { request_id, method }) => {
                    CollectionNodeKind::Request { request_id, method }
                }
                None => CollectionNodeKind::Folder {
                    children: build_tree_from_map(by_parent, Some(node.id.clone())),
                },
            };
            CollectionNode {
                id: node.id,
                parent_id: node.parent_id,
                position: position as i64,
                name: node.name,
                kind,
            }
        })
        .collect()
}

#[tauri::command(async)]
#[specta::specta]
pub fn create_folder(
    input: CreateFolderInput,
    state: State<'_, AppState>,
) -> Result<String, CommandError> {
    Ok(state
        .database
        .with_connection(|connection| add_folder(connection, &input))?)
}

pub(crate) fn add_folder(
    connection: &mut Connection,
    input: &CreateFolderInput,
) -> Result<String, StorageError> {
    let now = Utc::now().to_rfc3339();
    let node_id = Uuid::new_v4().to_string();
    let tx = connection.transaction()?;
    let sort_order = sort_order_for_position(
        &tx,
        &input.collection_id,
        input.parent_id.as_deref(),
        input.position,
        None,
    )?;
    insert_node(
        &tx,
        &NewNode {
            id: &node_id,
            collection_id: &input.collection_id,
            parent_id: input.parent_id.as_deref(),
            sort_order,
            kind: NodeKind::Folder,
            name: &input.name,
            auth_json: None,
        },
        &now,
    )?;
    tx.commit()?;
    Ok(node_id)
}

#[tauri::command(async)]
#[specta::specta]
pub fn delete_node(node_id: String, state: State<'_, AppState>) -> Result<(), CommandError> {
    Ok(state
        .database
        .with_connection(|connection| delete_subtree(connection, &node_id))?)
}

/// Deletes the node, every node under it, and the requests they point at.
pub(crate) fn delete_subtree(
    connection: &mut Connection,
    node_id: &str,
) -> Result<(), StorageError> {
    let tx = connection.transaction()?;
    let request_ids = collect_request_ids_for_subtree(&tx, node_id)?;
    tx.execute(
        "DELETE FROM collection_nodes WHERE id = ?",
        params![node_id],
    )?;
    for request_id in request_ids {
        tx.execute("DELETE FROM requests WHERE id = ?", params![request_id])?;
    }
    tx.commit()?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub fn move_node(input: MoveNodeInput, state: State<'_, AppState>) -> Result<(), CommandError> {
    Ok(state
        .database
        .with_connection(|connection| reparent_node(connection, &input))?)
}

pub(crate) fn reparent_node(
    connection: &mut Connection,
    input: &MoveNodeInput,
) -> Result<(), StorageError> {
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
        params![
            input.parent_id,
            sort_order,
            Utc::now().to_rfc3339(),
            input.node_id
        ],
    )?;
    tx.commit()?;
    Ok(())
}
pub(crate) fn sort_order_for_position(
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

pub(crate) fn sort_order_after(
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
        update.execute(params![sort_order_at(index), id])?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fixtures, storage::test_connection};

    fn ids(nodes: &[CollectionNode]) -> Vec<&str> {
        nodes.iter().map(|node| node.id.as_str()).collect()
    }

    fn children(nodes: &[CollectionNode]) -> &[CollectionNode] {
        match &nodes[0].kind {
            CollectionNodeKind::Folder { children } => children,
            CollectionNodeKind::Request { .. } => panic!("first node is not a folder"),
        }
    }

    #[test]
    fn deleting_a_folder_deletes_every_request_under_it_and_nothing_else() {
        let mut connection = test_connection();
        let collection = fixtures::collection(&connection);
        let folder = fixtures::folder(&mut connection, &collection, None);
        let nested = fixtures::folder(&mut connection, &collection, Some(&folder));
        fixtures::request(&mut connection, &collection, Some(&folder));
        fixtures::request(&mut connection, &collection, Some(&nested));
        let outside = fixtures::request(&mut connection, &collection, None);

        delete_subtree(&mut connection, &folder).unwrap();

        let tree = read_tree(&connection, &collection).unwrap();
        assert_eq!(ids(&tree), [outside.node_id.as_str()]);
        let requests: i64 = connection
            .query_row("SELECT COUNT(*) FROM requests", [], |row| row.get(0))
            .unwrap();
        assert_eq!(requests, 1, "requests under the folder were left behind");
    }

    #[test]
    fn a_folder_cannot_be_moved_into_its_own_descendant() {
        let mut connection = test_connection();
        let collection = fixtures::collection(&connection);
        let folder = fixtures::folder(&mut connection, &collection, None);
        let nested = fixtures::folder(&mut connection, &collection, Some(&folder));

        let moved = reparent_node(
            &mut connection,
            &MoveNodeInput {
                node_id: folder.clone(),
                parent_id: Some(nested.clone()),
                position: 0,
            },
        );

        assert!(matches!(moved, Err(StorageError::InvalidTreeMove)));
        let tree = read_tree(&connection, &collection).unwrap();
        assert_eq!(ids(&tree), [folder.as_str()]);
        assert_eq!(ids(children(&tree)), [nested.as_str()]);
    }

    #[test]
    fn a_moved_node_lands_at_the_position_it_was_dropped() {
        let mut connection = test_connection();
        let collection = fixtures::collection(&connection);
        let folder = fixtures::folder(&mut connection, &collection, None);
        let first = fixtures::request(&mut connection, &collection, Some(&folder)).node_id;
        let second = fixtures::request(&mut connection, &collection, Some(&folder)).node_id;
        let moved = fixtures::request(&mut connection, &collection, None).node_id;

        reparent_node(
            &mut connection,
            &MoveNodeInput {
                node_id: moved.clone(),
                parent_id: Some(folder),
                position: 1,
            },
        )
        .unwrap();

        let tree = read_tree(&connection, &collection).unwrap();
        assert_eq!(
            ids(children(&tree)),
            [first.as_str(), moved.as_str(), second.as_str()]
        );
    }
}
