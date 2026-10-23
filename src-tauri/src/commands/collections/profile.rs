//! The profile of one collection for the settings page: what its requests,
//! folders, environments, and variables add up to. Read on demand for the
//! selected collection only, so the counts cost nothing until asked for.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use tauri::State;

use crate::commands::{
    error::CommandError,
    models::{CollectionProfile, EnvironmentVariableCount, FolderSize, MethodCount},
};
use crate::{storage::StorageError, AppState};

#[tauri::command(async)]
#[specta::specta]
pub fn get_collection_profile(
    collection_id: String,
    state: State<'_, AppState>,
) -> Result<CollectionProfile, CommandError> {
    Ok(state
        .database
        .with_read_connection(|connection| read_profile(connection, &collection_id))?)
}

pub(crate) fn read_profile(
    connection: &Connection,
    collection_id: &str,
) -> Result<CollectionProfile, StorageError> {
    let created_at: String = connection
        .query_row(
            "SELECT created_at FROM collections WHERE id = ?",
            params![collection_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| StorageError::InvalidInput("collection not found".to_string()))?;

    let mut methods = Vec::new();
    let mut statement = connection.prepare(
        "SELECT method, COUNT(*) FROM requests WHERE collection_id = ?
         GROUP BY method ORDER BY COUNT(*) DESC, method",
    )?;
    for row in statement.query_map(params![collection_id], |row| {
        Ok(MethodCount {
            method: row.get(0)?,
            count: row.get(1)?,
        })
    })? {
        methods.push(row?);
    }

    let variable_url_count: u32 = connection.query_row(
        "SELECT COUNT(*) FROM requests WHERE collection_id = ? AND instr(url, '{{') > 0",
        params![collection_id],
        |row| row.get(0),
    )?;

    let collection_variable_count: u32 = connection.query_row(
        "SELECT COUNT(*) FROM variables WHERE scope = 'collection' AND collection_id = ?",
        params![collection_id],
        |row| row.get(0),
    )?;

    let mut environments = Vec::new();
    let mut statement = connection.prepare(
        "SELECT id, name,
                (SELECT COUNT(*) FROM variables
                 WHERE scope = 'environment' AND environment_id = environments.id)
         FROM environments WHERE collection_id = ? ORDER BY created_at, name",
    )?;
    for row in statement.query_map(params![collection_id], |row| {
        Ok(EnvironmentVariableCount {
            id: row.get(0)?,
            name: row.get(1)?,
            variable_count: row.get(2)?,
        })
    })? {
        environments.push(row?);
    }

    let tree = read_tree_shape(connection, collection_id)?;

    Ok(CollectionProfile {
        methods,
        collection_variable_count,
        environments,
        created_at,
        last_edited_at: tree.last_edited_at,
        deepest_nesting: tree.deepest_nesting,
        largest_folder: tree.largest_folder,
        latest_request: tree.latest_request,
        variable_url_count,
    })
}

struct TreeShape {
    last_edited_at: Option<String>,
    deepest_nesting: u32,
    largest_folder: Option<FolderSize>,
    latest_request: Option<String>,
}

struct Node {
    parent_id: Option<String>,
    is_folder: bool,
    name: String,
}

/// One pass over the collection's nodes, which are at most a few hundred
/// rows, answers everything about the tree's shape.
fn read_tree_shape(
    connection: &Connection,
    collection_id: &str,
) -> Result<TreeShape, StorageError> {
    let mut statement = connection.prepare(
        "SELECT id, parent_id, kind, name, updated_at FROM collection_nodes
         WHERE collection_id = ? ORDER BY updated_at DESC",
    )?;
    let mut nodes: HashMap<String, Node> = HashMap::new();
    let mut last_edited_at = None;
    let mut latest_request = None;
    for row in statement.query_map(params![collection_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)? == "folder",
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })? {
        let (id, parent_id, is_folder, name, updated_at) = row?;
        // Rows arrive newest first, so the first of each kind is the latest.
        if last_edited_at.is_none() {
            last_edited_at = Some(updated_at);
        }
        if !is_folder && latest_request.is_none() {
            latest_request = Some(name.clone());
        }
        nodes.insert(
            id,
            Node {
                parent_id,
                is_folder,
                name,
            },
        );
    }

    let mut deepest_nesting = 0;
    let mut requests_under: HashMap<&str, u32> = HashMap::new();
    for node in nodes.values() {
        let mut depth = 0;
        let mut parent = node.parent_id.as_deref();
        while let Some(parent_id) = parent {
            depth += 1;
            if !node.is_folder {
                *requests_under.entry(parent_id).or_default() += 1;
            }
            parent = nodes
                .get(parent_id)
                .and_then(|node| node.parent_id.as_deref());
        }
        if node.is_folder {
            deepest_nesting = deepest_nesting.max(depth + 1);
        }
    }

    let largest_folder = requests_under
        .iter()
        .filter_map(|(id, count)| nodes.get(*id).map(|node| (node.name.as_str(), *count)))
        // Ties go to the alphabetically first so the answer is stable.
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(name, request_count)| FolderSize {
            name: name.to_string(),
            request_count,
        });

    Ok(TreeShape {
        last_edited_at,
        deepest_nesting,
        largest_folder,
        latest_request,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::models::VariableTarget;
    use crate::{fixtures, storage::test_connection};

    #[test]
    fn a_profile_counts_what_the_collection_holds_and_nothing_from_other_collections() {
        let mut connection = test_connection();
        let collection = fixtures::collection(&connection);
        let other = fixtures::collection(&connection);
        let folder = fixtures::folder(&mut connection, &collection, None);
        let nested = fixtures::folder(&mut connection, &collection, Some(&folder));
        let deeper = fixtures::folder(&mut connection, &collection, Some(&nested));
        let first = fixtures::request(&mut connection, &collection, Some(&folder));
        fixtures::request(&mut connection, &collection, Some(&deeper));
        let loose = fixtures::request(&mut connection, &collection, None);
        fixtures::request(&mut connection, &other, None);
        connection
            .execute(
                "UPDATE requests SET method = 'POST', url = '{{base}}/items' WHERE id = ?",
                params![first.request_id],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE collection_nodes SET name = 'Orders' WHERE id = ?",
                params![folder],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE collection_nodes SET name = 'Newest', updated_at = '2099-01-01T00:00:00Z'
                 WHERE id = ?",
                params![loose.node_id],
            )
            .unwrap();
        let environment = fixtures::environment(&connection, &collection);
        fixtures::environment(&connection, &other);
        fixtures::variable(
            &connection,
            &VariableTarget::Environment {
                environment_id: environment.clone(),
            },
            "token",
            "x",
        );
        fixtures::variable(
            &connection,
            &VariableTarget::Collection {
                collection_id: collection.clone(),
            },
            "base",
            "http://127.0.0.1",
        );

        let profile = read_profile(&connection, &collection).unwrap();

        let methods: Vec<(&str, u32)> = profile
            .methods
            .iter()
            .map(|m| (m.method.as_str(), m.count))
            .collect();
        assert_eq!(methods, vec![("GET", 2), ("POST", 1)]);
        assert_eq!(profile.variable_url_count, 1);
        assert_eq!(profile.collection_variable_count, 1);
        assert_eq!(profile.environments.len(), 1);
        assert_eq!(profile.environments[0].id, environment);
        assert_eq!(profile.environments[0].variable_count, 1);
        assert_eq!(profile.deepest_nesting, 3);
        let largest = profile.largest_folder.unwrap();
        assert_eq!(largest.name, "Orders");
        assert_eq!(largest.request_count, 2);
        assert_eq!(profile.latest_request.as_deref(), Some("Newest"));
        assert_eq!(
            profile.last_edited_at.as_deref(),
            Some("2099-01-01T00:00:00Z")
        );
    }

    #[test]
    fn an_empty_collection_has_a_profile_with_nothing_in_it() {
        let connection = test_connection();
        let collection = fixtures::collection(&connection);

        let profile = read_profile(&connection, &collection).unwrap();

        assert!(profile.methods.is_empty());
        assert!(profile.environments.is_empty());
        assert_eq!(profile.deepest_nesting, 0);
        assert!(profile.largest_folder.is_none());
        assert!(profile.latest_request.is_none());
        assert!(profile.last_edited_at.is_none());
    }
}
