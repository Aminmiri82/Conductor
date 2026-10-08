//! The `INSERT`s shared by more than one command (creating, duplicating and
//! importing all write the same rows). They take `&Connection`, which a
//! `Transaction` derefs to, and reuse the prepared statement across rows.

use rusqlite::{params, Connection};

use super::StorageError;
use crate::commands::models::VariableTarget;

pub struct NewRequest<'a> {
    pub id: &'a str,
    pub collection_id: &'a str,
    pub method: &'a str,
    pub url: &'a str,
    pub headers_json: &'a str,
    pub query_json: &'a str,
    pub path_params_json: &'a str,
    pub auth_json: Option<&'a str>,
    pub body_json: Option<&'a str>,
    pub pre_request_script_json: Option<&'a str>,
    pub test_script_json: Option<&'a str>,
}

impl<'a> NewRequest<'a> {
    /// A blank `GET` with no URL, rows, auth, body or scripts.
    pub fn blank(id: &'a str, collection_id: &'a str) -> Self {
        Self {
            id,
            collection_id,
            method: "GET",
            url: "",
            headers_json: "[]",
            query_json: "[]",
            path_params_json: "[]",
            auth_json: None,
            body_json: None,
            pre_request_script_json: None,
            test_script_json: None,
        }
    }
}

pub fn insert_request(
    connection: &Connection,
    request: &NewRequest<'_>,
    now: &str,
) -> Result<(), StorageError> {
    connection
        .prepare_cached(
            "INSERT INTO requests
             (id, collection_id, method, url, headers_json, query_json, path_params_json,
              auth_json, body_json, pre_request_script_json, test_script_json,
              created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )?
        .execute(params![
            request.id,
            request.collection_id,
            request.method,
            request.url,
            request.headers_json,
            request.query_json,
            request.path_params_json,
            request.auth_json,
            request.body_json,
            request.pre_request_script_json,
            request.test_script_json,
            now,
            now
        ])?;
    Ok(())
}

pub enum NodeKind<'a> {
    Folder,
    /// Holds the id of the request the node points at.
    Request(&'a str),
}

pub struct NewNode<'a> {
    pub id: &'a str,
    pub collection_id: &'a str,
    pub parent_id: Option<&'a str>,
    pub sort_order: i64,
    pub kind: NodeKind<'a>,
    pub name: &'a str,
    pub auth_json: Option<&'a str>,
}

pub fn insert_node(
    connection: &Connection,
    node: &NewNode<'_>,
    now: &str,
) -> Result<(), StorageError> {
    let (kind, request_id) = match node.kind {
        NodeKind::Folder => ("folder", None),
        NodeKind::Request(request_id) => ("request", Some(request_id)),
    };
    connection
        .prepare_cached(
            "INSERT INTO collection_nodes
             (id, collection_id, parent_id, sort_order, kind, name, request_id, auth_json,
              created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )?
        .execute(params![
            node.id,
            node.collection_id,
            node.parent_id,
            node.sort_order,
            kind,
            node.name,
            request_id,
            node.auth_json,
            now,
            now
        ])?;
    Ok(())
}

pub struct NewVariable<'a> {
    pub target: &'a VariableTarget,
    pub key: &'a str,
    pub value: &'a str,
    pub enabled: bool,
    pub sensitive: bool,
}

const UPSERT_VARIABLE: &str = "INSERT INTO variables
     (scope, collection_id, environment_id, key, current_value, enabled, sensitive,
      created_at, updated_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT DO UPDATE SET
        current_value = excluded.current_value,
        enabled = excluded.enabled,
        sensitive = excluded.sensitive,
        updated_at = excluded.updated_at";

/// A key repeated within a scope keeps its last value.
pub fn upsert_variable(
    connection: &Connection,
    variable: &NewVariable<'_>,
    now: &str,
) -> Result<(), StorageError> {
    let (scope, collection_id, environment_id) = variable.target.columns();
    connection
        .prepare_cached(UPSERT_VARIABLE)?
        .execute(params![
            scope,
            collection_id,
            environment_id,
            variable.key,
            variable.value,
            variable.enabled,
            variable.sensitive,
            now,
            now
        ])?;
    Ok(())
}
