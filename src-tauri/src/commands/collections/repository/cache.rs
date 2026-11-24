use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::StorageError;

use super::{
    folder_id,
    format::{self, Environment, Folder, Manifest, Request},
    local_id, remote_id, values, Changes, RepositoryRequestChange,
};

pub(super) const MANIFEST: &str = "collection.json";

#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum Entity {
    Manifest(Manifest),
    Request(Box<Request>),
    Environment(Environment),
}

impl Entity {
    pub(super) fn kind(&self) -> &'static str {
        match self {
            Self::Manifest(_) => "manifest",
            Self::Request(_) => "request",
            Self::Environment(_) => "environment",
        }
    }

    pub(super) fn id(&self) -> &str {
        match self {
            Self::Manifest(item) => &item.id,
            Self::Request(item) => &item.id,
            Self::Environment(item) => &item.id,
        }
    }
}

pub(super) struct StoredFile {
    pub kind: String,
    pub id: String,
    pub contents: Vec<u8>,
    pub revision: String,
}

pub(super) struct File {
    pub contents: Vec<u8>,
    pub entity: Entity,
}

pub(super) type Files = BTreeMap<String, File>;
pub(super) type StoredFiles = BTreeMap<String, StoredFile>;

pub(super) fn stored_files(
    connection: &Connection,
    collection_id: &str,
) -> Result<StoredFiles, StorageError> {
    let mut statement = connection.prepare_cached(
        "SELECT path, kind, entity_id, contents, revision FROM repository_files
         WHERE collection_id = ?",
    )?;
    let rows = statement.query_map([collection_id], |row| {
        Ok((
            row.get(0)?,
            StoredFile {
                kind: row.get(1)?,
                id: row.get(2)?,
                contents: row.get(3)?,
                revision: row.get(4)?,
            },
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub(super) fn stored_request(
    connection: &Connection,
    id: &str,
) -> Result<Option<StoredFile>, StorageError> {
    Ok(connection
        .prepare_cached(
            "SELECT contents, revision, entity_id, kind FROM repository_files
         WHERE kind = 'request' AND entity_id = ?",
        )?
        .query_row([id], |row| {
            Ok(StoredFile {
                contents: row.get(0)?,
                revision: row.get(1)?,
                id: row.get(2)?,
                kind: row.get(3)?,
            })
        })
        .optional()?)
}

pub(super) fn request_group(
    connection: &Connection,
    collection_id: &str,
    folder_id: &str,
) -> Result<Option<(String, u32)>, StorageError> {
    let mut statement = connection.prepare_cached(
        "SELECT path, byte_count FROM repository_request_groups
         WHERE collection_id = ? AND folder_id = ? LIMIT 2",
    )?;
    let mut rows = statement.query(params![collection_id, folder_id])?;
    let group = rows
        .next()?
        .map(|row| Ok::<_, rusqlite::Error>((row.get(0)?, row.get(1)?)))
        .transpose()?;
    if rows.next()?.is_some() {
        return Err(super::input(
            "Folder has more than one request file. Resolve the duplicate before editing.",
        ));
    }
    Ok(group)
}

pub(super) fn repository_bytes(
    connection: &Connection,
    collection_id: &str,
) -> Result<i64, StorageError> {
    Ok(connection.prepare_cached(
        "SELECT (SELECT coalesce(sum(byte_count), 0) FROM repository_request_groups WHERE collection_id = ?1)
              + (SELECT coalesce(sum(length(contents)), 0) FROM repository_files
                 WHERE collection_id = ?1 AND kind = 'environment')",
    )?.query_row([collection_id], |row| row.get(0))?)
}

pub(super) fn request_key(id: &str) -> String {
    format!("request/{id}")
}

#[derive(Clone, PartialEq)]
pub(super) struct RequestGroup {
    pub folder_id: String,
    pub byte_count: u32,
}

pub(super) type Groups = BTreeMap<String, RequestGroup>;

pub(super) fn groups(connection: &Connection, collection_id: &str) -> Result<Groups, StorageError> {
    let mut statement = connection.prepare_cached(
        "SELECT path, folder_id, byte_count FROM repository_request_groups WHERE collection_id = ?",
    )?;
    let rows = statement.query_map([collection_id], |row| {
        Ok((
            row.get(0)?,
            RequestGroup {
                folder_id: row.get(1)?,
                byte_count: row.get(2)?,
            },
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub(super) fn record_groups(
    connection: &Connection,
    collection_id: &str,
    previous: &Groups,
    next: &Groups,
) -> Result<(), StorageError> {
    for (path, group) in next {
        if previous.get(path) != Some(group) {
            record_group(connection, collection_id, path, group)?;
        }
    }
    for path in previous.keys().filter(|path| !next.contains_key(*path)) {
        connection
            .prepare_cached(
                "DELETE FROM repository_request_groups WHERE collection_id = ? AND path = ?",
            )?
            .execute(params![collection_id, path])?;
    }
    Ok(())
}

pub(super) fn record_group(
    connection: &Connection,
    collection_id: &str,
    path: &str,
    group: &RequestGroup,
) -> Result<(), StorageError> {
    connection
        .prepare_cached(
            "INSERT INTO repository_request_groups (collection_id, path, folder_id, byte_count)
         VALUES (?, ?, ?, ?) ON CONFLICT(collection_id,path) DO UPDATE SET
         folder_id = excluded.folder_id, byte_count = excluded.byte_count",
        )?
        .execute(params![
            collection_id,
            path,
            group.folder_id,
            group.byte_count
        ])?;
    Ok(())
}

pub(super) fn record_file(
    connection: &Connection,
    collection_id: &str,
    path: &str,
    file: &File,
    previous: Option<&StoredFile>,
) -> Result<String, StorageError> {
    let revision = match (&file.entity, previous) {
        (Entity::Request(_), Some(old)) if same_editor_data(&old.contents, &file.contents)? => {
            old.revision.clone()
        }
        _ => Uuid::new_v4().to_string(),
    };
    let id = if matches!(file.entity, Entity::Manifest(_)) {
        collection_id.to_string()
    } else {
        local_id(collection_id, file.entity.id())
    };
    connection.execute(
        "INSERT INTO repository_files (collection_id, path, kind, entity_id, contents, revision)
         VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(collection_id, path) DO UPDATE SET
         kind = excluded.kind, entity_id = excluded.entity_id,
         contents = excluded.contents, revision = excluded.revision",
        params![
            collection_id,
            path,
            file.entity.kind(),
            id,
            file.contents,
            revision
        ],
    )?;
    Ok(revision)
}

// Moving or reordering an open request changes its placement, which the
// editor never writes. Keep the draft's revision valid for these changes.
fn same_editor_data(previous: &[u8], next: &[u8]) -> Result<bool, StorageError> {
    fn editor_data(bytes: &[u8]) -> Result<serde_json::Value, StorageError> {
        let mut value: serde_json::Value = serde_json::from_slice(bytes)?;
        if let Some(fields) = value.as_object_mut() {
            fields.shift_remove("parentId");
            fields.shift_remove("position");
            fields.shift_remove("moveGeneration");
        }
        Ok(value)
    }
    Ok(editor_data(previous)? == editor_data(next)?)
}

pub(super) fn remove_file(
    connection: &Connection,
    collection_id: &str,
    path: &str,
) -> Result<(), StorageError> {
    connection.execute(
        "DELETE FROM repository_files WHERE collection_id = ? AND path = ?",
        params![collection_id, path],
    )?;
    Ok(())
}

#[cfg(test)]
pub(super) fn revision(
    connection: &Connection,
    request_id: &str,
) -> Result<Option<String>, StorageError> {
    Ok(connection
        .query_row(
            "SELECT revision FROM repository_files
             WHERE kind = 'request' AND entity_id = ?",
            [request_id],
            |row| row.get(0),
        )
        .optional()?)
}

// Project only shared data. Local overrides are never serialized here.
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Placement<'a> {
    pub(super) parent_id: Option<&'a str>,
    #[serde(default)]
    move_generation: u32,
}

const REQUEST_PROJECTION: &str = "SELECT n.id, n.parent_id, n.name, n.sort_order, n.auth_json,
            r.id, r.method, r.url, r.headers_json, r.query_json, r.path_params_json,
            r.auth_json, r.body_json, r.pre_request_script_json, r.test_script_json
     FROM collection_nodes n LEFT JOIN requests r ON r.id = n.request_id";

fn project_request(
    row: &Row<'_>,
    connection: &Connection,
    id: &str,
    previous: Option<Placement<'_>>,
    collection_name: Option<&str>,
) -> Result<Request, StorageError> {
    let parent_node_id: Option<String> = row.get(1)?;
    let parent_id = parent_node_id.as_deref().map(|id| remote_id(id).into());
    let move_generation = match previous {
        Some(previous) if previous.parent_id != parent_id.as_deref() => previous
            .move_generation
            .checked_add(1)
            .ok_or_else(|| super::input("Request move generation is exhausted."))?,
        Some(previous) => previous.move_generation,
        None => 0,
    };
    let mut request = Request {
        id: remote_id(id).into(),
        parent_id,
        move_generation,
        name: row.get(2)?,
        position: row.get(3)?,
        method: row.get(6)?,
        url: row.get(7)?,
        headers: decode(row.get(8)?)?,
        query: decode(row.get(9)?)?,
        path_params: decode(row.get(10)?)?,
        auth: format::project_auth(decode_optional(row.get(11)?)?),
        body: decode_optional(row.get(12)?)?,
        pre_request_script: decode_optional(row.get(13)?)?,
        test_script: decode_optional(row.get(14)?)?,
    };
    if let Err(error) = format::validate_request(&mut request) {
        let folder: Option<String> = connection
            .query_row(
                "SELECT name FROM collection_nodes WHERE id = ?",
                [parent_node_id],
                |row| row.get(0),
            )
            .optional()?;
        let collection_name = match (folder, collection_name) {
            (Some(folder), _) => folder,
            (None, Some(name)) => name.into(),
            (None, None) => connection.query_row(
                "SELECT c.name FROM collections c JOIN requests r ON r.collection_id = c.id WHERE r.id = ?",
                [id],
                |row| row.get(0),
            )?,
        };
        return Err(super::input(format!(
            "Request {:?} in {collection_name:?}: {error}",
            request.name
        )));
    }
    Ok(request)
}

pub(super) fn project_one_request(
    connection: &Connection,
    id: &str,
    previous: Placement<'_>,
) -> Result<Request, StorageError> {
    let mut statement =
        connection.prepare_cached(&format!("{REQUEST_PROJECTION} WHERE n.request_id = ?"))?;
    let mut rows = statement.query([id])?;
    let row = rows
        .next()?
        .ok_or_else(|| super::input("The request was removed."))?;
    project_request(row, connection, id, Some(previous), None)
}

pub(super) fn project(
    connection: &Connection,
    collection_id: &str,
    manifest_id: &str,
    stored: &StoredFiles,
    mut root: Option<&Path>,
) -> Result<Files, StorageError> {
    let (name, auth): (String, Option<String>) = connection.query_row(
        "SELECT name, auth_json FROM collections WHERE id = ?",
        [collection_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let shared = values::projection(connection, collection_id, collection_id)?;
    let mut manifest = Manifest {
        conductor_format: format::READABLE_FORMAT,
        id: manifest_id.to_string(),
        name,
        auth: format::project_auth(decode_optional(auth)?),
        folders: Vec::new(),
        values: shared.values,
        secrets: shared.secrets,
    };

    let mut files = Files::new();
    let mut nodes = connection.prepare(&format!(
        "{REQUEST_PROJECTION} WHERE n.collection_id = ? ORDER BY n.id"
    ))?;
    let mut rows = nodes.query([collection_id])?;
    while let Some(row) = rows.next()? {
        if let Some(id) = row.get::<_, Option<String>>(5)? {
            let placement = stored
                .get(&request_key(remote_id(&id)))
                .map(|file| serde_json::from_slice(&file.contents))
                .transpose()?;
            let request = project_request(row, connection, &id, placement, Some(&manifest.name))?;
            put_request(&mut files, request)?;
        } else {
            let id: String = row.get(0)?;
            let parent_node_id: Option<String> = row.get(1)?;
            manifest.folders.push(Folder {
                id: remote_id(&id).into(),
                parent_id: parent_node_id.as_deref().map(|id| remote_id(id).into()),
                name: row.get(2)?,
                position: row.get(3)?,
                auth: format::project_auth(decode_optional(row.get(4)?)?),
            });
        }
    }
    let folder_ids: HashSet<_> = manifest
        .folders
        .iter()
        .map(|folder| folder.id.as_str())
        .collect();
    for file in files.values() {
        if let Entity::Request(request) = &file.entity {
            if request
                .parent_id
                .as_deref()
                .is_some_and(|parent| !folder_ids.contains(parent))
            {
                return Err(super::input("Choose a folder as the request's parent."));
            }
        }
    }
    format::validate_manifest(&manifest)?;
    put(&mut files, MANIFEST.into(), Entity::Manifest(manifest))?;

    let mut slugs = format::Slugs::default();
    for path in stored
        .keys()
        .filter(|path| path.starts_with("environments/"))
    {
        if let Some(stem) = path.strip_suffix(".json") {
            slugs.reserve(stem);
        }
    }
    let mut environments = connection.prepare(
        "SELECT e.id, e.name, f.path FROM environments e
         LEFT JOIN repository_files f ON f.kind = 'environment'
            AND f.entity_id = e.id AND f.collection_id = e.collection_id
         WHERE e.collection_id = ? AND e.archived = 0 ORDER BY e.id",
    )?;
    let mut rows = environments.query([collection_id])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let shared = values::projection(connection, collection_id, &id)?;
        let environment = Environment {
            id: Uuid::parse_str(remote_id(&id))
                .map_err(|_| StorageError::InvalidInput("Invalid environment ID.".into()))?
                .to_string(),
            name: row.get(1)?,
            values: shared.values,
            secrets: shared.secrets,
        };
        format::validate_environment(&environment)?;
        let path = if let Some(path) = row.get::<_, Option<String>>(2)? {
            path
        } else {
            if let Some(root) = root.take() {
                for path in format::list(root, "environments")? {
                    if let Some(stem) = path.to_ascii_lowercase().strip_suffix(".json") {
                        slugs.reserve(stem);
                    }
                }
                values::reserve_environment_paths(root, &mut slugs)?;
            }
            slugs.file("environments", &environment.name)
        };
        put(&mut files, path, Entity::Environment(environment))?;
    }
    Ok(files)
}

pub(super) fn put_request(files: &mut Files, request: Request) -> Result<(), StorageError> {
    put(
        files,
        request_key(&request.id),
        Entity::Request(Box::new(request)),
    )
}

pub(super) fn put(files: &mut Files, path: String, entity: Entity) -> Result<(), StorageError> {
    let contents = format::encode(&entity)?;
    files.insert(path, File { contents, entity });
    Ok(())
}

fn decode<T: DeserializeOwned>(value: String) -> Result<T, StorageError> {
    Ok(serde_json::from_str(&value)?)
}

fn decode_optional<T: DeserializeOwned>(value: Option<String>) -> Result<Option<T>, StorageError> {
    value.map(decode).transpose()
}

fn json<T: Serialize>(value: &Option<T>) -> Result<Option<String>, StorageError> {
    Ok(value.as_ref().map(serde_json::to_string).transpose()?)
}

pub(super) fn apply(
    tx: &Connection,
    collection_id: &str,
    files: &Files,
    stored: &StoredFiles,
) -> Result<Changes, StorageError> {
    let Some(File {
        entity: Entity::Manifest(manifest),
        ..
    }) = files.get(MANIFEST)
    else {
        return Err(StorageError::InvalidInput(
            "Repository manifest is missing.".into(),
        ));
    };
    let now = Utc::now().to_rfc3339();

    let mut summary = false;
    if stored
        .get(MANIFEST)
        .is_none_or(|old| old.contents != files[MANIFEST].contents)
    {
        summary = tx.query_row(
            "SELECT name != ? FROM collections WHERE id = ?",
            params![manifest.name, collection_id],
            |row| row.get(0),
        )?;
        tx.execute(
            "UPDATE collections SET name = ?, auth_json = ? WHERE id = ?",
            params![manifest.name, json(&manifest.auth)?, collection_id],
        )?;
        let folders: HashMap<_, _> = manifest
            .folders
            .iter()
            .map(|folder| (folder.id.as_str(), folder))
            .collect();
        let mut written = HashSet::new();
        for folder in &manifest.folders {
            write_folder(tx, collection_id, &now, folder, &folders, &mut written)?;
        }
    }
    let mut nodes: HashSet<String> = manifest
        .folders
        .iter()
        .map(|folder| folder_id(collection_id, &folder.id))
        .collect();
    let mut changes = Vec::new();
    let mut environments = Vec::new();
    for (path, file) in files {
        let unchanged = stored
            .get(path)
            .is_some_and(|old| old.contents == file.contents);
        match &file.entity {
            Entity::Request(request) => {
                let id = local_id(collection_id, &request.id);
                nodes.insert(id.clone());
                if !unchanged {
                    write_request(tx, collection_id, &id, request, &now)?;
                }
            }
            Entity::Environment(environment) if !unchanged => {
                write_environment(tx, collection_id, environment, &now)?;
                environments.push(local_id(collection_id, &environment.id));
            }
            _ => {}
        }
        if !unchanged {
            let revision = record_file(tx, collection_id, path, file, stored.get(path))?;
            if let Entity::Request(request) = &file.entity {
                changes.push(RepositoryRequestChange {
                    request_id: local_id(collection_id, &request.id),
                    revision: Some(revision),
                    name: Some(request.name.clone()),
                    method: Some(request.method.clone()),
                });
            }
        }
    }
    for (path, old) in stored {
        let replaced = files.get(path).is_some_and(|file| {
            old.kind != file.entity.kind()
                || (old.kind != "manifest" && remote_id(&old.id) != file.entity.id())
        });
        if files.contains_key(path) && !replaced {
            continue;
        }
        if !replaced {
            remove_file(tx, collection_id, path)?;
        }
        match old.kind.as_str() {
            "request" if !nodes.contains(&old.id) => {
                tx.execute("DELETE FROM requests WHERE id = ?", [&old.id])?;
                changes.push(RepositoryRequestChange {
                    request_id: old.id.clone(),
                    revision: None,
                    name: None,
                    method: None,
                });
            }
            "environment"
                if !files.values().any(|file| {
                    matches!(
                        &file.entity, Entity::Environment(env) if env.id == remote_id(&old.id)
                    )
                }) =>
            {
                tx.execute(
                    "UPDATE environments SET archived = 1 WHERE id = ?",
                    [&old.id],
                )?;
                environments.push(old.id.clone());
            }
            _ => {}
        }
    }
    {
        let mut statement =
            tx.prepare("SELECT id FROM collection_nodes WHERE collection_id = ?")?;
        let ids = statement
            .query_map([collection_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for id in ids.into_iter().filter(|id| !nodes.contains(id)) {
            tx.execute("DELETE FROM collection_nodes WHERE id = ?", [&id])?;
        }
    }
    environments.sort_unstable();
    environments.dedup();
    changes.sort_unstable_by(|a, b| a.request_id.cmp(&b.request_id));
    Ok(Changes {
        environments,
        requests: changes,
        summary,
        ..Default::default()
    })
}

fn write_folder<'a>(
    connection: &Connection,
    collection_id: &str,
    now: &str,
    folder: &'a Folder,
    folders: &HashMap<&str, &'a Folder>,
    written: &mut HashSet<&'a str>,
) -> Result<(), StorageError> {
    if !written.insert(&folder.id) {
        return Ok(());
    }
    if let Some(parent) = folder.parent_id.as_deref() {
        write_folder(
            connection,
            collection_id,
            now,
            folders[parent],
            folders,
            written,
        )?;
    }
    connection.execute(
        "INSERT INTO collection_nodes
         (id, collection_id, parent_id, sort_order, kind, name, auth_json, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'folder', ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET
         parent_id = excluded.parent_id, sort_order = excluded.sort_order,
         name = excluded.name, auth_json = excluded.auth_json",
        params![
            folder_id(collection_id, &folder.id),
            collection_id,
            folder
                .parent_id
                .as_ref()
                .map(|id| folder_id(collection_id, id)),
            folder.position,
            folder.name,
            json(&folder.auth)?,
            now,
            now
        ],
    )?;
    Ok(())
}

fn write_request(
    connection: &Connection,
    collection_id: &str,
    id: &str,
    request: &Request,
    now: &str,
) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO requests
         (id, collection_id, method, url, headers_json, query_json, path_params_json,
          auth_json, body_json, pre_request_script_json, test_script_json, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET
         method = excluded.method, url = excluded.url, headers_json = excluded.headers_json,
         query_json = excluded.query_json, path_params_json = excluded.path_params_json,
         auth_json = excluded.auth_json, body_json = excluded.body_json,
         pre_request_script_json = excluded.pre_request_script_json,
         test_script_json = excluded.test_script_json, updated_at = excluded.updated_at",
        params![
            id,
            collection_id,
            request.method,
            request.url,
            serde_json::to_string(&request.headers)?,
            serde_json::to_string(&request.query)?,
            serde_json::to_string(&request.path_params)?,
            json(&request.auth)?,
            json(&request.body)?,
            json(&request.pre_request_script)?,
            json(&request.test_script)?,
            now,
            now
        ],
    )?;
    connection.execute(
        "INSERT INTO collection_nodes
         (id, collection_id, parent_id, sort_order, kind, name, request_id, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'request', ?, ?, ?, ?) ON CONFLICT(request_id)
         WHERE request_id IS NOT NULL DO UPDATE SET parent_id = excluded.parent_id,
         sort_order = excluded.sort_order, name = excluded.name, updated_at = excluded.updated_at",
        params![
            id,
            collection_id,
            request
                .parent_id
                .as_ref()
                .map(|id| folder_id(collection_id, id)),
            request.position,
            request.name,
            id,
            now,
            now
        ],
    )?;
    Ok(())
}

fn write_environment(
    connection: &Connection,
    collection_id: &str,
    environment: &Environment,
    now: &str,
) -> Result<(), StorageError> {
    let id = local_id(collection_id, &environment.id);
    connection.execute(
        "INSERT INTO environments (id, collection_id, name, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET name = excluded.name, archived = 0",
        params![id, collection_id, environment.name, now, now],
    )?;
    Ok(())
}
