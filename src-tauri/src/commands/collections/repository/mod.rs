mod cache;
mod format;
mod group;
#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::{
    commands::{
        error::CommandError,
        models::{CollectionSummary, ImportedCollection, RepositoryLink},
    },
    storage::StorageError,
    AppState,
};

use super::{read_collections, tree::read_tree};
use cache::{Entity, File, Files, Groups, RequestGroup, StoredFiles, MANIFEST};

const MAX_REPOSITORY_BYTES: usize = 64 * 1024 * 1024;
const REPOSITORY_SIZE_ERROR: &str =
    "A repository collection may contain at most 64 MiB of request and environment files.";

struct Link {
    root: String,
    manifest_id: String,
    issue: Option<String>,
    watch_stamp: Option<String>,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryRequestChange {
    pub request_id: String,
    pub revision: Option<String>,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryRefresh {
    pub collections: Vec<CollectionSummary>,
    pub changed_requests: Vec<RepositoryRequestChange>,
    pub changed_collection_ids: Vec<String>,
}

pub(crate) use cache::revision;

pub(crate) fn linked(connection: &Connection, collection_id: &str) -> Result<bool, StorageError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM repository_links WHERE collection_id = ?)",
        [collection_id],
        |row| row.get(0),
    )?)
}

fn read_link(connection: &Connection, collection_id: &str) -> Result<Option<Link>, StorageError> {
    Ok(connection.query_row(
        "SELECT root, manifest_id, issue, watch_stamp FROM repository_links WHERE collection_id = ?",
        [collection_id],
        |row| Ok(Link { root: row.get(0)?, manifest_id: row.get(1)?, issue: row.get(2)?, watch_stamp: row.get(3)? }),
    ).optional()?)
}

pub(crate) fn new_entity_id(
    connection: &Connection,
    collection_id: &str,
) -> Result<String, StorageError> {
    let id = Uuid::new_v4().to_string();
    Ok(if linked(connection, collection_id)? {
        local_id(collection_id, &id)
    } else {
        id
    })
}

pub(crate) fn new_folder_id(
    connection: &Connection,
    collection_id: &str,
) -> Result<String, StorageError> {
    let id = Uuid::new_v4().to_string();
    Ok(if linked(connection, collection_id)? {
        folder_id(collection_id, &id)
    } else {
        id
    })
}

fn local_id(collection_id: &str, id: &str) -> String {
    format!("{collection_id}/{id}")
}

fn folder_id(collection_id: &str, id: &str) -> String {
    format!("{collection_id}/folder/{id}")
}

fn remote_id(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
}

fn input(message: impl Into<String>) -> StorageError {
    StorageError::InvalidInput(message.into())
}

fn io(error: std::io::Error) -> StorageError {
    input(error.to_string())
}

fn canonical_root(root: &str) -> Result<PathBuf, StorageError> {
    let path = fs::canonicalize(root).map_err(io)?;
    if !path.is_dir() {
        return Err(input("Choose a repository folder."));
    }
    Ok(path)
}

fn root_string(root: &Path) -> Result<&str, StorageError> {
    root.to_str()
        .ok_or_else(|| input("The repository path must be valid Unicode."))
}

#[tauri::command(async)]
#[specta::specta]
pub fn inspect_repository(root: String) -> Result<bool, CommandError> {
    let root = canonical_root(&root)?;
    Ok(repository_exists(&root)?)
}

fn repository_exists(root: &Path) -> Result<bool, StorageError> {
    match fs::symlink_metadata(root.join(".conductor")) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io(error)),
    }
}

#[tauri::command(async)]
#[specta::specta]
pub fn open_repository(
    root: String,
    create: bool,
    source_collection_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<ImportedCollection, CommandError> {
    let root = canonical_root(&root)?;
    Ok(state.database.with_connection(|connection| {
        connect(connection, &root, create, source_collection_id.as_deref())
    })?)
}

fn connect(
    connection: &mut Connection,
    root: &Path,
    create: bool,
    source: Option<&str>,
) -> Result<ImportedCollection, StorageError> {
    let root_text = root_string(root)?;
    let existing: Option<String> = connection
        .query_row(
            "SELECT collection_id FROM repository_links WHERE root = ?",
            [root_text],
            |row| row.get(0),
        )
        .optional()?;
    let collection_id = match existing {
        Some(id) => {
            if let Some(link) = read_link(connection, &id)? {
                refresh_link(connection, &id, &link)?;
            }
            id
        }
        None => {
            let id = Uuid::new_v4().to_string();
            let (files, groups, issues) = if create {
                if repository_exists(root)? {
                    return Err(input("This folder already contains .conductor."));
                }
                let files = if let Some(source) = source {
                    if linked(connection, source)? {
                        return Err(input("Choose a local collection to publish."));
                    }
                    cache::project(connection, source, &id, &StoredFiles::new())?
                } else {
                    let manifest = Entity::Manifest(format::Manifest {
                        conductor_format: 1,
                        id: id.clone(),
                        name: root
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("New collection")
                            .to_string(),
                        auth: None,
                        folders: vec![],
                    });
                    BTreeMap::from([(
                        MANIFEST.into(),
                        File {
                            contents: manifest.bytes()?,
                            entity: manifest,
                        },
                    )])
                };
                // Publish an entire initial snapshot together; staging is on the same filesystem.
                publish(root, &files)?
            } else {
                scan(root, None, &StoredFiles::new(), &Groups::new())?
            };
            let Entity::Manifest(manifest) = &files[MANIFEST].entity else {
                unreachable!()
            };
            let now = Utc::now().to_rfc3339();
            let tx = connection.savepoint()?;
            tx.execute(
                "INSERT INTO collections (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)",
                params![id, manifest.name, now, now],
            )?;
            tx.execute(
                "INSERT INTO repository_links (collection_id, root, manifest_id, issue) VALUES (?, ?, ?, ?)",
                params![id, root_text, manifest.id, issue_text(&issues)],
            )?;
            cache::apply(&tx, &id, &files, &StoredFiles::new())?;
            cache::record_groups(&tx, &id, &Groups::new(), &groups)?;
            if create {
                if let Some(source) = source {
                    copy_local_values(&tx, source, &id)?;
                }
            }
            tx.commit()?;
            id
        }
    };

    Ok(ImportedCollection {
        collections: read_collections(connection)?,
        tree: read_tree(connection, &collection_id)?,
        collection_id,
    })
}

fn publish(root: &Path, files: &Files) -> Result<(Files, Groups, Vec<String>), StorageError> {
    let staging = root.join(format!(".conductor-publish-{}", Uuid::new_v4()));
    fs::create_dir(&staging).map_err(io)?;
    let result = (|| {
        fs::create_dir(staging.join(".conductor")).map_err(io)?;
        write_projection(
            &staging,
            &Files::new(),
            files,
            &StoredFiles::new(),
            &Groups::new(),
        )?;
        let snapshot = scan(&staging, None, &StoredFiles::new(), &Groups::new())?;
        if !snapshot.2.is_empty() {
            return Err(input(snapshot.2.join("\n")));
        }
        if repository_exists(root)? {
            return Err(input("This folder already contains .conductor."));
        }
        fs::rename(staging.join(".conductor"), root.join(".conductor")).map_err(io)?;
        Ok(snapshot)
    })();
    let cleanup = fs::remove_dir_all(&staging).map_err(io);
    result.and_then(|snapshot| cleanup.map(|()| snapshot))
}

fn copy_local_values(
    connection: &Connection,
    source: &str,
    target: &str,
) -> Result<(), StorageError> {
    connection.execute(
        "INSERT INTO variables (scope, collection_id, key, current_value, enabled, sensitive, created_at, updated_at)
         SELECT scope, ?, key, current_value, enabled, sensitive, created_at, updated_at
         FROM variables WHERE collection_id = ?",
        params![target, source],
    )?;
    connection.execute(
        "INSERT INTO variables (scope, environment_id, key, current_value, enabled, sensitive, created_at, updated_at)
         SELECT v.scope, ? || '/' ||
         CASE WHEN length(v.environment_id) = 32 THEN
             substr(v.environment_id, 1, 8) || '-' || substr(v.environment_id, 9, 4) || '-' ||
             substr(v.environment_id, 13, 4) || '-' || substr(v.environment_id, 17, 4) || '-' ||
             substr(v.environment_id, 21, 12)
         ELSE v.environment_id END, v.key, v.current_value,
         v.enabled, v.sensitive, v.created_at, v.updated_at
         FROM variables v JOIN environments e ON e.id = v.environment_id
         WHERE e.collection_id = ?",
        params![target, source],
    )?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub fn relink_repository(
    collection_id: String,
    root: String,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    let root = canonical_root(&root)?;
    Ok(state
        .database
        .with_connection(|connection| relocate(connection, &collection_id, &root))?)
}

fn relocate(
    connection: &mut Connection,
    collection_id: &str,
    root: &Path,
) -> Result<(), StorageError> {
    let link = read_link(connection, collection_id)?
        .ok_or_else(|| input("This collection has no repository."))?;
    let stored = cache::stored_files(connection, collection_id)?;
    let groups = cache::groups(connection, collection_id)?;
    let (_, _, issues) = scan(root, Some(&link.manifest_id), &stored, &groups)?;
    let tx = connection.savepoint()?;
    tx.execute(
        "UPDATE repository_links SET root = ?, issue = ?, watch_stamp = NULL WHERE collection_id = ?",
        params![root_string(root)?, issue_text(&issues), collection_id],
    )?;
    // The next workspace refresh delivers changes to any open editors.
    tx.commit()?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
pub fn refresh_repositories(state: State<'_, AppState>) -> Result<RepositoryRefresh, CommandError> {
    Ok(state.database.with_connection(refresh_all)?)
}

pub(crate) fn refresh_all(connection: &mut Connection) -> Result<RepositoryRefresh, StorageError> {
    let links = {
        let mut statement = connection.prepare(
            "SELECT collection_id, root, manifest_id, issue, watch_stamp FROM repository_links",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                Link {
                    root: row.get(1)?,
                    manifest_id: row.get(2)?,
                    issue: row.get(3)?,
                    watch_stamp: row.get(4)?,
                },
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let mut changed_collection_ids = Vec::new();
    let mut changed_requests = Vec::new();
    for (id, link) in links {
        match refresh_link(connection, &id, &link) {
            Ok(Some(mut changes)) => {
                changed_collection_ids.push(id);
                changed_requests.append(&mut changes);
            }
            Ok(None) => {}
            Err(error) => {
                set_issue(connection, &id, Some(&error.to_string()))?;
                changed_collection_ids.push(id);
            }
        }
    }
    Ok(RepositoryRefresh {
        collections: read_collections(connection)?,
        changed_requests,
        changed_collection_ids,
    })
}

fn refresh_link(
    connection: &mut Connection,
    collection_id: &str,
    link: &Link,
) -> Result<Option<Vec<RepositoryRequestChange>>, StorageError> {
    let root = Path::new(&link.root);
    let stamp = watch_stamp(root).ok();
    if stamp.is_some() && stamp == link.watch_stamp {
        return Ok(None);
    }
    let stored = cache::stored_files(connection, collection_id)?;
    let old_groups = cache::groups(connection, collection_id)?;
    let (files, groups, issues) = match scan(root, Some(&link.manifest_id), &stored, &old_groups) {
        Ok(result) => result,
        Err(error) => {
            let issue = Some(error.to_string());
            let changed = issue != link.issue;
            if changed || link.watch_stamp.is_some() {
                set_issue(connection, collection_id, issue.as_deref())?;
            }
            return Ok(changed.then(Vec::new));
        }
    };
    let issue = issue_text(&issues);
    let tx = connection.savepoint()?;
    let mut changed = false;
    let mut changes = Vec::new();
    if files.len() != stored.len()
        || files.iter().any(|(path, file)| {
            stored
                .get(path)
                .is_none_or(|old| old.contents != file.contents)
        })
    {
        changes = cache::apply(&tx, collection_id, &files, &stored)?;
        changed = true;
    }
    cache::record_groups(&tx, collection_id, &old_groups, &groups)?;
    tx.execute(
        "UPDATE repository_links SET issue = ?, watch_stamp = ? WHERE collection_id = ?",
        params![issue, stamp, collection_id],
    )?;
    tx.commit()?;
    changed |= issue != link.issue;
    Ok(changed.then_some(changes))
}

fn watch_stamp(root: &Path) -> Result<String, StorageError> {
    ensure_same_root(root)?;
    let base = root.join(".conductor");
    let metadata = fs::symlink_metadata(&base).map_err(io)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(input("Expected a real .conductor directory."));
    }
    let mut hasher = DefaultHasher::new();
    for path in std::iter::once(MANIFEST.to_string())
        .chain(format::list(root, "requests")?)
        .chain(format::list(root, "environments")?)
    {
        let metadata = fs::symlink_metadata(base.join(&path)).map_err(io)?;
        path.hash(&mut hasher);
        metadata.len().hash(&mut hasher);
        metadata.modified().map_err(io)?.hash(&mut hasher);
        metadata.file_type().hash(&mut hasher);
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            metadata.ino().hash(&mut hasher);
            metadata.ctime().hash(&mut hasher);
            metadata.ctime_nsec().hash(&mut hasher);
        }
    }
    Ok(hasher.finish().to_string())
}

fn set_issue(connection: &Connection, id: &str, issue: Option<&str>) -> Result<(), StorageError> {
    connection.execute(
        "UPDATE repository_links SET issue = ?, watch_stamp = NULL WHERE collection_id = ?",
        params![issue, id],
    )?;
    Ok(())
}

fn issue_text(issues: &[String]) -> Option<String> {
    (!issues.is_empty()).then(|| issues.join("\n"))
}

fn ensure_same_root(root: &Path) -> Result<(), StorageError> {
    let actual = fs::canonicalize(root).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            input("Repository folder is missing. Relink to its new location.")
        } else {
            io(error)
        }
    })?;
    if actual != root {
        return Err(input(
            "Repository folder now points to a different path. Relink to its new location.",
        ));
    }
    Ok(())
}

fn read_manifest(root: &Path, expected_id: Option<&str>) -> Result<File, StorageError> {
    ensure_same_root(root)?;
    let contents = format::read(root, MANIFEST)?.ok_or_else(|| {
        input("Repository is missing .conductor/collection.json. Relink to its new location.")
    })?;
    let manifest =
        format::parse_manifest(&contents).map_err(|error| input(format!("{MANIFEST}: {error}")))?;
    if expected_id.is_some_and(|id| id != manifest.id) {
        return Err(input(
            "Repository collection ID changed. Relink to the original collection.",
        ));
    }
    Ok(File {
        contents,
        entity: Entity::Manifest(manifest),
    })
}

fn scan(
    root: &Path,
    expected_id: Option<&str>,
    stored: &StoredFiles,
    old_groups: &Groups,
) -> Result<(Files, Groups, Vec<String>), StorageError> {
    let manifest = read_manifest(root, expected_id)?;
    let Entity::Manifest(data) = &manifest.entity else {
        unreachable!()
    };
    let parents: HashSet<_> = data
        .folders
        .iter()
        .map(|folder| folder.id.as_str())
        .collect();
    let mut files = Files::new();
    let mut groups = Groups::new();
    let mut issues = Vec::new();
    let mut bytes_read = 0;
    let mut account = |bytes: usize| {
        bytes_read += bytes;
        if bytes_read > MAX_REPOSITORY_BYTES {
            return Err(input(REPOSITORY_SIZE_ERROR));
        }
        Ok(())
    };

    let mut folder_paths = HashSet::new();
    let mut candidates: HashMap<String, Vec<(String, format::Request)>> = HashMap::new();
    let mut failed_folders = HashSet::new();
    for path in format::list(root, "requests")? {
        let read = format::read(root, &path);
        if let Ok(Some(contents)) = &read {
            account(contents.len())?;
        }
        let parsed = (|| -> Result<_, StorageError> {
            let Some(contents) = read? else {
                return Ok(None);
            };
            let parsed = group::parse(&contents)?;
            if parsed
                .folder_id
                .as_deref()
                .is_some_and(|folder| !parents.contains(folder))
            {
                return Err(input("Folder ID is missing from collection.json."));
            }
            Ok(Some((contents, parsed)))
        })();
        let (contents, parsed) = match parsed {
            Ok(Some(parsed)) => parsed,
            Ok(None) => continue,
            Err(error) => {
                issues.push(format!("{path}: {error}"));
                if let Some(old) = old_groups.get(&path) {
                    if old.folder_id.is_empty() || parents.contains(old.folder_id.as_str()) {
                        failed_folders.insert(old.folder_id.clone());
                        groups.insert(
                            path,
                            RequestGroup {
                                folder_id: old.folder_id.clone(),
                                contents: old.contents.clone(),
                            },
                        );
                    }
                }
                continue;
            }
        };
        let folder_id = parsed.folder_id.unwrap_or_default();
        if !folder_paths.insert(folder_id.clone()) {
            failed_folders.insert(folder_id.clone());
            issues.push(format!("{path}: Folder has more than one request file."));
        }
        for (id, request) in parsed.requests {
            match request {
                Ok(request) => candidates
                    .entry(id)
                    .or_default()
                    .push((path.clone(), request)),
                Err(error) => {
                    issues.push(format!("{path} [{id}]: {error}"));
                    if let Some(old) = stored.get(&cache::request_key(&id)) {
                        let request = format::parse_request(&old.contents)?;
                        if request.parent_id.as_deref().unwrap_or_default() == folder_id {
                            candidates
                                .entry(id)
                                .or_default()
                                .push((path.clone(), request));
                        }
                    }
                }
            }
        }
        groups.insert(
            path,
            RequestGroup {
                folder_id,
                contents,
            },
        );
    }

    for (id, entries) in candidates {
        if entries.len() == 1 {
            let (_, request) = entries.into_iter().next().expect("one request");
            if !failed_folders.contains(request.parent_id.as_deref().unwrap_or_default()) {
                insert_request_file(&mut files, request)?;
            }
        } else {
            for (path, _) in entries {
                issues.push(format!("{path} [{id}]: Duplicate request ID."));
            }
            if let Some(old) = stored.get(&cache::request_key(&id)) {
                let request = format::parse_request(&old.contents)?;
                if request
                    .parent_id
                    .as_deref()
                    .is_none_or(|parent| parents.contains(parent))
                {
                    insert_request_file(&mut files, request)?;
                }
            }
        }
    }

    if !failed_folders.is_empty() {
        for old in stored.values().filter(|old| old.kind == "request") {
            let request = format::parse_request(&old.contents)?;
            if failed_folders.contains(request.parent_id.as_deref().unwrap_or_default()) {
                insert_request_file(&mut files, request)?;
            }
        }
    }

    let mut environments: HashMap<String, Vec<(String, File)>> = HashMap::new();
    for path in format::list(root, "environments")? {
        let read = format::read(root, &path);
        if let Ok(Some(contents)) = &read {
            account(contents.len())?;
        }
        let parsed = (|| -> Result<_, StorageError> {
            let Some(contents) = read? else {
                return Ok(None);
            };
            let entity = Entity::Environment(format::parse_environment(&contents)?);
            Ok(Some(File { contents, entity }))
        })();
        let file = match parsed {
            Ok(Some(file)) => file,
            Ok(None) => continue,
            Err(error) => {
                issues.push(format!("{path}: {error}"));
                let Some(old) = stored.get(&path) else {
                    continue;
                };
                File {
                    contents: old.contents.clone(),
                    entity: Entity::Environment(format::parse_environment(&old.contents)?),
                }
            }
        };
        environments
            .entry(file.entity.id().into())
            .or_default()
            .push((path, file));
    }
    for (id, entries) in environments {
        if entries.len() == 1 {
            files.extend(entries);
        } else {
            for (path, _) in entries {
                issues.push(format!("{path}: Duplicate environment ID {id}."));
                if let Some(old) = stored.get(&path).filter(|old| remote_id(&old.id) == id) {
                    files.insert(
                        path,
                        File {
                            contents: old.contents.clone(),
                            entity: Entity::Environment(format::parse_environment(&old.contents)?),
                        },
                    );
                }
            }
        }
    }
    files.insert(MANIFEST.into(), manifest);
    issues.sort();
    Ok((files, groups, issues))
}

fn insert_request_file(files: &mut Files, request: format::Request) -> Result<(), StorageError> {
    let path = cache::request_key(&request.id);
    let entity = Entity::Request(Box::new(request));
    files.insert(
        path,
        File {
            contents: entity.bytes()?,
            entity,
        },
    );
    Ok(())
}

/// Reuse the ordinary SQL operation in the linked cache. Compare its shared
/// projection before and after, so a local-only edit or untouched hand-edited
/// file is never rewritten.
pub(crate) fn mutate<T>(
    connection: &mut Connection,
    collection_id: &str,
    operation: impl FnOnce(&mut Connection) -> Result<T, StorageError>,
) -> Result<T, StorageError> {
    let Some(link) = read_link(connection, collection_id)? else {
        return operation(connection);
    };
    let root = Path::new(&link.root);
    read_manifest(root, Some(&link.manifest_id))?;
    let stored = cache::stored_files(connection, collection_id)?;
    let groups = cache::groups(connection, collection_id)?;
    let before = cache::project(connection, collection_id, &link.manifest_id, &stored)?;
    connection.execute_batch("SAVEPOINT repository_edit")?;
    let result = (|| {
        let result = operation(connection)?;
        let after = cache::project(connection, collection_id, &link.manifest_id, &stored)?;
        let (edited, group_changes) = write_projection(root, &before, &after, &stored, &groups)?;
        cache::update_groups(connection, collection_id, group_changes)?;
        for path in edited {
            match after.get(&path) {
                Some(file) => {
                    cache::record_file(connection, collection_id, &path, file, stored.get(&path))?;
                }
                None => cache::remove_file(connection, collection_id, &path)?,
            }
        }
        Ok(result)
    })();
    if result.is_err() {
        connection.execute_batch("ROLLBACK TO repository_edit")?;
    }
    connection.execute_batch("RELEASE repository_edit")?;
    result
}

fn request(file: Option<&File>) -> Option<&format::Request> {
    match file {
        Some(File {
            entity: Entity::Request(request),
            ..
        }) => Some(request.as_ref()),
        _ => None,
    }
}

struct Edit {
    path: String,
    old: Option<Vec<u8>>,
    next: Option<Vec<u8>>,
}

fn write_projection(
    root: &Path,
    before: &Files,
    after: &Files,
    stored: &StoredFiles,
    groups: &Groups,
) -> Result<(Vec<String>, Vec<GroupChange>), StorageError> {
    let changed: Vec<_> = before
        .keys()
        .chain(after.keys().filter(|path| !before.contains_key(*path)))
        .filter(|path| {
            before.get(*path).map(|file| &file.contents)
                != after.get(*path).map(|file| &file.contents)
        })
        .cloned()
        .collect();

    let mut edits = Vec::new();
    let mut request_groups: BTreeMap<&str, BTreeMap<String, Option<&format::Request>>> =
        BTreeMap::new();
    let baseline = |path: &str| stored.get(path).map(|file| file.contents.as_slice());
    let manifest_baseline = format::read(root, MANIFEST)?;
    if manifest_baseline.as_deref() != baseline(MANIFEST) {
        return Err(input(
            "collection.json has changed on disk. Refresh the repository before editing.",
        ));
    }
    for path in &changed {
        let previous = before.get(path);
        let next = after.get(path);
        let (old_request, new_request) = (request(previous), request(next));
        if old_request.is_some() || new_request.is_some() {
            if let Some(old) = old_request {
                let folder = old.parent_id.as_deref().unwrap_or_default();
                request_groups
                    .entry(folder)
                    .or_default()
                    .insert(old.id.clone(), None);
            }
            if let Some(new) = new_request {
                let folder = new.parent_id.as_deref().unwrap_or_default();
                request_groups
                    .entry(folder)
                    .or_default()
                    .insert(new.id.clone(), Some(new));
            }
            continue;
        }
        let old = format::read(root, path)?;
        if old.as_deref() != baseline(path) {
            return Err(input(format!(
                "{path} has changed on disk. Refresh the repository before editing."
            )));
        }
        edits.push(Edit {
            path: path.clone(),
            old,
            next: next.map(|file| file.contents.clone()),
        });
    }

    let folders = match after.get(MANIFEST).map(|file| &file.entity) {
        Some(Entity::Manifest(manifest)) => &manifest.folders[..],
        _ => &[],
    };
    // Creation and deletion of a folder are also visible as creation and deletion
    // of its request file, even if there are no requests in that folder.
    let old_folders = match before.get(MANIFEST).map(|file| &file.entity) {
        Some(Entity::Manifest(manifest)) => &manifest.folders[..],
        _ => &[],
    };
    if changed.iter().any(|path| path == MANIFEST) {
        let current: HashSet<_> = folders.iter().map(|folder| folder.id.as_str()).collect();
        let previous: HashSet<_> = old_folders
            .iter()
            .map(|folder| folder.id.as_str())
            .collect();
        for folder in current.symmetric_difference(&previous) {
            request_groups.entry(folder).or_default();
        }
    }
    if before.is_empty() {
        request_groups.entry("").or_default();
    }
    let mut group_changes = Vec::new();
    for (folder, requests) in request_groups {
        let definition = folders.iter().find(|item| item.id == folder);
        let removed = !folder.is_empty() && definition.is_none();
        let path = if let Some(path) = group_path(groups, folder)? {
            path.to_string()
        } else if folder.is_empty() {
            "requests/root.json".into()
        } else if let Some(item) = definition {
            format::file_name("requests", &item.name, folder)
        } else {
            continue;
        };
        let old = format::read(root, &path)?;
        if old.is_none() && groups.contains_key(&path) {
            return Err(input(format!(
                "{path} was removed on disk. Refresh the repository before editing."
            )));
        }
        if let Some(contents) = &old {
            let actual = group::parse(contents)?;
            if actual.folder_id.as_deref().unwrap_or_default() != folder || (removed && actual.requests.len() != requests.len()) {
                return Err(input(format!("{path} has changed on disk. Refresh the repository before editing.")));
            }
            for id in requests.keys() {
                let expected = before.get(&cache::request_key(id)).and_then(|file| match &file.entity {
                    Entity::Request(request) if request.parent_id.as_deref().unwrap_or_default() == folder => Some(file.contents.as_slice()),
                    _ => None,
                });
                let latest = actual.requests.get(id)
                    .map(|entry| entry.as_ref().map_err(|error| input(format!("{path} [{id}]: {error}"))))
                    .transpose()?
                    .map(format::encode).transpose()?;
                if latest.as_deref() != expected {
                    return Err(input(format!("{path} [{id}] has changed on disk. Refresh the repository before editing.")));
                }
            }
        } else if requests.keys().any(|id| matches!(
            before.get(&cache::request_key(id)).map(|file| &file.entity),
            Some(Entity::Request(item)) if item.parent_id.as_deref().unwrap_or_default() == folder
        )) {
            return Err(input(format!("{path} was removed on disk. Refresh the repository before editing.")));
        }

        let next = if removed {
            None
        } else {
            Some(group::edit(
                old.as_deref(),
                (!folder.is_empty()).then_some(folder),
                &requests,
            )?)
        };
        if old != next {
            group_changes.push(GroupChange {
                path: path.clone(),
                folder: folder.to_string(),
                contents: next.clone(),
            });
            edits.push(Edit { path, old, next });
        }
    }
    let mut bytes = groups
        .values()
        .map(|group| group.contents.len())
        .sum::<usize>()
        + stored
            .values()
            .filter(|file| file.kind == "environment")
            .map(|file| file.contents.len())
            .sum::<usize>();
    for edit in edits.iter().filter(|edit| edit.path != MANIFEST) {
        let old = groups
            .get(&edit.path)
            .map(|group| group.contents.as_slice())
            .or_else(|| baseline(&edit.path));
        bytes -= old.map_or(0, <[u8]>::len);
        bytes += edit.next.as_ref().map_or(0, Vec::len);
    }
    if bytes > MAX_REPOSITORY_BYTES {
        return Err(input(REPOSITORY_SIZE_ERROR));
    }

    // Write dependent files before the manifest. The next scan can recover
    // from failure partway through a multi-file change.
    ensure_same_root(root)?;
    edits.sort_by_key(|edit| (edit.path == MANIFEST, edit.next.is_none()));
    for (index, edit) in edits.iter().enumerate() {
        if let Err(error) =
            format::replace(root, &edit.path, edit.old.as_deref(), edit.next.as_deref())
        {
            // Roll back only files that still match this operation's output.
            let mut rollback_error = None;
            for written in edits[..index].iter().rev() {
                if let Err(rollback) = format::replace(
                    root,
                    &written.path,
                    written.next.as_deref(),
                    written.old.as_deref(),
                ) {
                    rollback_error.get_or_insert(rollback);
                }
            }
            if let Some(rollback) = rollback_error {
                return Err(input(format!("{error}. Repository changed partway through the edit ({rollback}). Refresh it to reconcile the cache.")));
            }
            return Err(error);
        }
    }
    Ok((changed, group_changes))
}

struct GroupChange {
    path: String,
    folder: String,
    contents: Option<Vec<u8>>,
}

fn group_path<'a>(groups: &'a Groups, folder: &str) -> Result<Option<&'a str>, StorageError> {
    let mut found = groups.iter().filter(|(_, value)| value.folder_id == folder);
    let first = found.next().map(|(path, _)| path.as_str());
    if found.next().is_some() {
        return Err(input("A folder has more than one request file. Fix and refresh the repository before editing."));
    }
    Ok(first)
}

pub(crate) enum RequestBaseline {
    Local,
    Current,
    Conflict {
        revision: Option<String>,
        refresh: Option<RepositoryRefresh>,
    },
}

pub(crate) fn check_request(
    connection: &mut Connection,
    collection_id: &str,
    request_id: &str,
    expected: Option<&str>,
) -> Result<RequestBaseline, StorageError> {
    let Some(link) = read_link(connection, collection_id)? else {
        return Ok(RequestBaseline::Local);
    };
    let old: Option<(String, Vec<u8>)> = connection
        .query_row(
            "SELECT COALESCE(n.parent_id, ''), f.contents FROM repository_files f
         JOIN collection_nodes n ON n.request_id = f.entity_id
         WHERE f.collection_id = ? AND f.kind = 'request' AND f.entity_id = ?",
            params![collection_id, request_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let root = Path::new(&link.root);
    read_manifest(root, Some(&link.manifest_id))?;
    let mut refresh = None;
    if let Some((folder, bytes)) = old {
        let groups = cache::groups(connection, collection_id)?;
        let latest = match group_path(&groups, remote_id(&folder))? {
            Some(path) => {
                let contents = format::read(root, path)?;
                contents
                    .as_deref()
                    .map(group::parse)
                    .transpose()?
                    .and_then(|mut group| group.requests.remove(remote_id(request_id)))
                    .map(|request| request.map_err(|error| input(format!("{path}: {error}"))))
                    .transpose()?
                    .as_ref()
                    .map(format::encode)
                    .transpose()?
            }
            None => None,
        };
        if latest.as_deref() != Some(&bytes) {
            connection.execute(
                "UPDATE repository_links SET watch_stamp = NULL WHERE collection_id = ?",
                [collection_id],
            )?;
            refresh = Some(refresh_all(connection)?);
        }
    }
    let revision = cache::revision(connection, request_id)?;
    Ok(
        if revision.is_none() || revision.as_deref() != expected || refresh.is_some() {
            RequestBaseline::Conflict { revision, refresh }
        } else {
            RequestBaseline::Current
        },
    )
}

pub(super) fn summary_link(root: Option<String>, issue: Option<String>) -> Option<RepositoryLink> {
    root.map(|root| RepositoryLink { root, issue })
}
