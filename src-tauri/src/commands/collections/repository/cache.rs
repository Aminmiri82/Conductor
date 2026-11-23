use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

use crate::storage::StorageError;

use super::{
    folder_id,
    format::{self, Environment, Folder, Manifest, Request},
    local_id, remote_id, GroupChange, RepositoryRequestChange,
};

pub(super) const MANIFEST: &str = "collection.json";

#[derive(Clone)]
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

    pub(super) fn bytes(&self) -> Result<Vec<u8>, StorageError> {
        match self {
            Self::Manifest(item) => format::encode(item),
            Self::Request(item) => format::encode(item),
            Self::Environment(item) => format::encode(item),
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

pub(super) fn request_key(id: &str) -> String {
    format!("request/{id}")
}

pub(super) struct RequestGroup {
    pub folder_id: String,
    pub contents: Vec<u8>,
}

pub(super) type Groups = BTreeMap<String, RequestGroup>;

pub(super) fn groups(connection: &Connection, collection_id: &str) -> Result<Groups, StorageError> {
    let mut statement = connection.prepare_cached(
        "SELECT path, folder_id, contents FROM repository_request_groups WHERE collection_id = ?",
    )?;
    let rows = statement.query_map([collection_id], |row| {
        Ok((
            row.get(0)?,
            RequestGroup {
                folder_id: row.get(1)?,
                contents: row.get(2)?,
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
        if previous
            .get(path)
            .is_none_or(|old| old.contents != group.contents)
        {
            write_group(
                connection,
                collection_id,
                path,
                &group.folder_id,
                Some(&group.contents),
            )?;
        }
    }
    for path in previous.keys().filter(|path| !next.contains_key(*path)) {
        write_group(connection, collection_id, path, "", None)?;
    }
    Ok(())
}

pub(super) fn update_groups(
    connection: &Connection,
    collection_id: &str,
    changes: Vec<GroupChange>,
) -> Result<(), StorageError> {
    for change in changes {
        write_group(
            connection,
            collection_id,
            &change.path,
            &change.folder,
            change.contents.as_deref(),
        )?;
    }
    Ok(())
}

fn write_group(
    connection: &Connection,
    collection_id: &str,
    path: &str,
    folder: &str,
    contents: Option<&[u8]>,
) -> Result<(), StorageError> {
    match contents {
        Some(contents) => connection
            .prepare_cached(
                "INSERT INTO repository_request_groups (collection_id, path, folder_id, contents)
             VALUES (?, ?, ?, ?) ON CONFLICT(collection_id,path) DO UPDATE SET
             folder_id = excluded.folder_id, contents = excluded.contents",
            )?
            .execute(params![collection_id, path, folder, contents])?,
        None => connection
            .prepare_cached(
                "DELETE FROM repository_request_groups WHERE collection_id = ? AND path = ?",
            )?
            .execute(params![collection_id, path])?,
    };
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

pub(crate) fn revision(
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

// Project only shared data. In particular the variables table is never read here.
pub(super) fn project(
    connection: &Connection,
    collection_id: &str,
    manifest_id: &str,
    stored: &StoredFiles,
) -> Result<Files, StorageError> {
    let paths: HashMap<_, _> = stored
        .iter()
        .map(|(path, file)| ((file.kind.as_str(), file.id.as_str()), path))
        .collect();
    let (name, auth): (String, Option<String>) = connection.query_row(
        "SELECT name, auth_json FROM collections WHERE id = ?",
        [collection_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let mut manifest = Manifest {
        conductor_format: 1,
        id: manifest_id.to_string(),
        name,
        auth: decode_optional(auth)?,
        folders: Vec::new(),
    };

    let mut files = Files::new();
    let mut nodes = connection.prepare(
        "SELECT n.id, n.parent_id, n.name, n.sort_order, n.auth_json,
                r.id, r.method, r.url, r.headers_json, r.query_json, r.path_params_json,
                r.auth_json, r.body_json, r.pre_request_script_json, r.test_script_json
         FROM collection_nodes n LEFT JOIN requests r ON r.id = n.request_id
         WHERE n.collection_id = ? ORDER BY n.id",
    )?;
    let mut rows = nodes.query([collection_id])?;
    while let Some(row) = rows.next()? {
        let parent_id = row
            .get::<_, Option<String>>(1)?
            .map(|id| remote_id(&id).into());
        let name = row.get(2)?;
        let position = row.get(3)?;
        if let Some(id) = row.get::<_, Option<String>>(5)? {
            let mut request = Request {
                id: remote_id(&id).into(),
                parent_id,
                name,
                position,
                method: row.get(6)?,
                url: row.get(7)?,
                headers: decode(row.get(8)?)?,
                query: decode(row.get(9)?)?,
                path_params: decode(row.get(10)?)?,
                auth: decode_optional(row.get(11)?)?,
                body: decode_optional(row.get(12)?)?,
                pre_request_script: decode_optional(row.get(13)?)?,
                test_script: decode_optional(row.get(14)?)?,
            };
            format::validate_request(&mut request)?;
            let path = request_key(&request.id);
            put(&mut files, path, Entity::Request(Box::new(request)))?;
        } else {
            let id: String = row.get(0)?;
            manifest.folders.push(Folder {
                id: remote_id(&id).into(),
                parent_id,
                name,
                position,
                auth: decode_optional(row.get(4)?)?,
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
                return Err(StorageError::InvalidInput(
                    "Choose a folder as the request's parent.".into(),
                ));
            }
        }
    }
    format::validate_manifest(&manifest)?;
    put(&mut files, MANIFEST.into(), Entity::Manifest(manifest))?;

    let mut environments = connection.prepare(
        "SELECT e.id, e.name FROM environments e LEFT JOIN repository_environments re
         ON re.environment_id = e.id WHERE e.collection_id = ? AND COALESCE(re.active, 1) = 1",
    )?;
    let mut defaults = connection
        .prepare_cached("SELECT key, value FROM repository_defaults WHERE environment_id = ?")?;
    let mut rows = environments.query([collection_id])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let values = defaults.query_map([&id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let environment = Environment {
            id: Uuid::parse_str(remote_id(&id))
                .map_err(|_| StorageError::InvalidInput("Invalid environment ID.".into()))?
                .to_string(),
            name: row.get(1)?,
            defaults: values.collect::<Result<_, _>>()?,
        };
        format::validate_environment(&environment)?;
        let path = paths
            .get(&("environment", id.as_str()))
            .map(|path| path.to_string())
            .unwrap_or_else(|| {
                format::file_name("environments", &environment.name, &environment.id)
            });
        put(&mut files, path, Entity::Environment(environment))?;
    }
    Ok(files)
}

fn put(files: &mut Files, path: String, entity: Entity) -> Result<(), StorageError> {
    let contents = entity.bytes()?;
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
) -> Result<Vec<RepositoryRequestChange>, StorageError> {
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

    if stored
        .get(MANIFEST)
        .is_none_or(|old| old.contents != files[MANIFEST].contents)
    {
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
    let mut changes = BTreeMap::new();
    let mut present_requests = HashSet::new();
    for (path, file) in files {
        let unchanged = stored
            .get(path)
            .is_some_and(|old| old.contents == file.contents);
        match &file.entity {
            Entity::Request(request) => {
                let id = local_id(collection_id, &request.id);
                present_requests.insert(id.clone());
                nodes.insert(id.clone());
                if !unchanged {
                    write_request(tx, collection_id, &id, request, &now)?;
                }
            }
            Entity::Environment(environment) if !unchanged => {
                write_environment(tx, collection_id, environment, &now)?;
            }
            _ => {}
        }
        if !unchanged {
            let revision = record_file(tx, collection_id, path, file, stored.get(path))?;
            if matches!(file.entity, Entity::Request(_)) {
                changes.insert(local_id(collection_id, file.entity.id()), Some(revision));
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
            "request" if !present_requests.contains(&old.id) => {
                tx.execute("DELETE FROM requests WHERE id = ?", [&old.id])?;
                changes.insert(old.id.clone(), None);
            }
            "environment"
                if !files.values().any(|file| {
                    matches!(
                        &file.entity, Entity::Environment(env) if env.id == remote_id(&old.id)
                    )
                }) =>
            {
                tx.execute(
                    "UPDATE repository_environments SET active = 0 WHERE environment_id = ?",
                    [&old.id],
                )?;
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
    Ok(changes
        .into_iter()
        .map(|(request_id, revision)| RepositoryRequestChange {
            request_id,
            revision,
        })
        .collect())
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
         VALUES (?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET name = excluded.name",
        params![id, collection_id, environment.name, now, now],
    )?;
    connection.execute(
        "INSERT INTO repository_environments (environment_id, active) VALUES (?, 1)
         ON CONFLICT(environment_id) DO UPDATE SET active = 1",
        [&id],
    )?;
    connection.execute(
        "DELETE FROM repository_defaults WHERE environment_id = ?",
        [&id],
    )?;
    let mut insert = connection.prepare_cached(
        "INSERT INTO repository_defaults (environment_id, key, value) VALUES (?, ?, ?)",
    )?;
    for (key, value) in &environment.defaults {
        insert.execute(params![id, key, value])?;
    }
    Ok(())
}
