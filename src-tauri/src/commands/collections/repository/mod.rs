mod cache;
mod format;
mod group;
#[cfg(test)]
mod tests;
pub(crate) mod values;

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

struct Edit {
    order: u8,
    path: String,
    old: Option<Vec<u8>>,
    next: Option<Vec<u8>>,
}

impl Edit {
    fn apply(&self, root: &Path) -> Result<(), StorageError> {
        format::replace(root, &self.path, self.old.as_deref(), self.next.as_deref())
    }
}

// A directory lock coordinates processes using the same worktree even when
// their databases differ. It requires no lock files or worktree changes.
#[cfg(unix)]
struct RootLock(fs::File);

#[cfg(unix)]
impl Drop for RootLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[cfg(unix)]
fn lock_root(root: &Path) -> Result<RootLock, StorageError> {
    ensure_same_root(root)?;
    let directory = fs::File::open(root).map_err(io)?;
    directory
        .try_lock()
        .map_err(|_| input("Another Conductor window is using this repository. Try again."))?;
    Ok(RootLock(directory))
}

#[cfg(not(unix))]
fn lock_root(root: &Path) -> Result<std::sync::MutexGuard<'static, ()>, StorageError> {
    use std::sync::Mutex;
    static LOCKS: [Mutex<()>; 64] = [const { Mutex::new(()) }; 64];
    ensure_same_root(root)?;
    let mut hasher = DefaultHasher::new();
    root.hash(&mut hasher);
    LOCKS[hasher.finish() as usize % LOCKS.len()]
        .try_lock()
        .map_err(|_| input("Another Conductor window is using this repository. Try again."))
}

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
    pub changed_environment_ids: Vec<String>,
    pub changed_variable_collection_ids: Vec<String>,
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
    let selected = Path::new(root);
    let root = if selected
        .file_name()
        .is_some_and(|name| name == ".conductor")
    {
        selected
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
    } else {
        selected
    };
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

pub(crate) fn connect(
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
            if create && !repository_exists(root)? {
                return Err(input("This repository is already linked. Restore its .conductor directory or relink the collection."));
            }
            if let Some(link) = read_link(connection, &id)? {
                refresh_link(connection, &id, &link)?;
            }
            id
        }
        None => {
            let _lock = lock_root(root)?;
            let pending: Option<(String, Option<String>)> = connection.query_row(
                "SELECT manifest_id, source_collection_id FROM repository_pending_publishes WHERE root = ?",
                [root_text],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ).optional()?;
            clean_staging(root)?;
            values::clean_legacy_staging(root)?;
            let has_repository = repository_exists(root)?;
            let mut expected = None;
            let mut publish_source = None;
            if has_repository {
                if let Some((id, previous_source)) = pending {
                    if create && source != previous_source.as_deref() {
                        return Err(input("A different collection was already published here. Open the repository to finish linking it."));
                    }
                    if read_manifest(root, None)?.entity.id() == id {
                        expected = Some(id);
                        publish_source = previous_source;
                    } else if create {
                        return Err(input(
                            "This folder already contains a different .conductor collection.",
                        ));
                    }
                }
                if create && expected.is_none() {
                    return Err(input("This folder already contains .conductor."));
                }
            } else if create {
                let manifest_id = Uuid::new_v4().to_string();
                let staging = Publish::new(root)?;
                values::ensure_ignore(&staging.path)?;
                let files = if let Some(source) = source {
                    if linked(connection, source)? {
                        return Err(input("Choose a local collection to publish."));
                    }
                    let mut files = cache::project(
                        connection,
                        source,
                        &manifest_id,
                        &StoredFiles::new(),
                        None,
                        None,
                    )?;
                    values::share_source(connection, source, &mut files)?;
                    files
                } else {
                    let manifest = Entity::Manifest(format::Manifest {
                        conductor_format: format::READABLE_FORMAT,
                        id: manifest_id.clone(),
                        name: root
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("New collection")
                            .to_string(),
                        auth: None,
                        folders: vec![],
                        values: Default::default(),
                        secrets: Default::default(),
                    });
                    let mut files = Files::new();
                    cache::put(&mut files, MANIFEST.into(), manifest)?;
                    files
                };
                let (_, _, edits) = plan_projection(
                    &staging.path,
                    &Files::new(),
                    &files,
                    &StoredFiles::new(),
                    &Groups::new(),
                    false,
                )?;
                for edit in &edits {
                    edit.apply(&staging.path)?;
                }
                format::sync_directories(
                    &staging.path,
                    edits.iter().map(|edit| edit.path.as_str()),
                )?;
                let (_, _, issues) = scan(
                    &staging.path,
                    Some(&manifest_id),
                    &StoredFiles::new(),
                    &Groups::new(),
                )?;
                if !issues.is_empty() {
                    return Err(input(issues.join("\n")));
                }
                // Record only the identity needed to finish linking if the app
                // exits after installing the directory.
                connection.execute(
                    "INSERT OR REPLACE INTO repository_pending_publishes
                     (root, manifest_id, source_collection_id) VALUES (?, ?, ?)",
                    params![root_text, manifest_id, source],
                )?;
                if repository_exists(root)? {
                    return Err(input("This folder already contains .conductor."));
                }
                fs::rename(staging.path.join(".conductor"), root.join(".conductor")).map_err(io)?;
                format::sync_directories(root, [MANIFEST])?;
                expected = Some(manifest_id);
                publish_source = source.map(str::to_owned);
            }

            let before = Watch::read(root)?;
            let (files, groups, mut issues) = scan(
                root,
                expected.as_deref(),
                &StoredFiles::new(),
                &Groups::new(),
            )?;
            values::ensure_ignore(root)?;
            if let Some(source) = publish_source {
                let local = values::publish_local(connection, &source, &files)?;
                values::write_local(root, &local)?;
            }
            let watch = Watch::read(root)?;
            let local = values::read(root)?;
            values::report_orphans(&files, &local, &mut issues);
            if watch.0.contains_key(values::LOCAL) {
                if let Err(error) = values::check_private(root, &[values::LOCAL]) {
                    issues.push(error.to_string());
                }
            }
            if !before.shared().eq(watch.shared()) || watch_stamp(root)? != watch.stamp() {
                return Err(input(
                    "The repository changed while opening. Try opening it again.",
                ));
            }
            let stamp = watch.stamp();
            let Entity::Manifest(manifest) = &files[MANIFEST].entity else {
                unreachable!()
            };
            let id = Uuid::new_v4().to_string();
            let now = Utc::now().to_rfc3339();
            let tx = connection.savepoint()?;
            tx.execute(
                "INSERT INTO collections (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)",
                params![id, manifest.name, now, now],
            )?;
            tx.execute(
                "INSERT INTO repository_links (collection_id, root, manifest_id, issue, watch_stamp) VALUES (?, ?, ?, ?, ?)",
                params![id, root_text, manifest.id, issue_text(&issues), stamp],
            )?;
            cache::apply(&tx, &id, &files, &StoredFiles::new())?;
            cache::record_groups(&tx, &id, &Groups::new(), &groups)?;
            values::cache(&tx, &id, &files, &local)?;
            tx.execute(
                "DELETE FROM repository_pending_publishes WHERE root = ?",
                [root_text],
            )?;
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

const PUBLISH_MARKER: &str = ".conductor-staging";

struct Publish {
    path: PathBuf,
    marker: Option<fs::File>,
}

impl Publish {
    fn new(root: &Path) -> Result<Self, StorageError> {
        let path = root.join(format!(".conductor-publish-{}", Uuid::new_v4()));
        fs::create_dir(&path).map_err(io)?;
        let mut staging = Self { path, marker: None };
        let marker = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(staging.path.join(PUBLISH_MARKER))
            .map_err(io)?;
        marker
            .try_lock()
            .map_err(|_| input("Could not lock the staging directory."))?;
        staging.marker = Some(marker);
        fs::create_dir(staging.path.join(".conductor")).map_err(io)?;
        Ok(staging)
    }
}

impl Drop for Publish {
    fn drop(&mut self) {
        drop(self.marker.take());
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn clean_staging(root: &Path) -> Result<(), StorageError> {
    for entry in fs::read_dir(root).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry.file_name();
        let owned_name = name
            .to_str()
            .and_then(|name| name.strip_prefix(".conductor-publish-"))
            .is_some_and(|suffix| Uuid::parse_str(suffix).is_ok());
        if !owned_name || !entry.file_type().map_err(io)?.is_dir() {
            continue;
        }
        let path = entry.path();
        let marker_path = path.join(PUBLISH_MARKER);
        if !fs::symlink_metadata(&marker_path).is_ok_and(|metadata| metadata.is_file()) {
            continue;
        }
        let marker = match fs::OpenOptions::new().write(true).open(&marker_path) {
            Ok(marker) => marker,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io(error)),
        };
        if marker.try_lock().is_err() {
            continue;
        }
        fs::remove_dir_all(&path).map_err(io)?;
    }
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
    let stored = cache::stored_files(connection, collection_id, None)?;
    let groups = cache::groups(connection, collection_id)?;
    let _lock = lock_root(root)?;
    let (_, _, issues) = scan(root, Some(&link.manifest_id), &stored, &groups)?;
    values::ensure_ignore(root)?;
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
    let mut changed_environment_ids = Vec::new();
    let mut changed_variable_collection_ids = Vec::new();
    for (id, link) in links {
        let mut changes = match refresh_link(connection, &id, &link) {
            Ok(changes) => changes,
            Err(error) => {
                set_issue(connection, &id, Some(&error.to_string()))?;
                Changes {
                    shared: true,
                    ..Default::default()
                }
            }
        };
        if changes.shared {
            changed_collection_ids.push(id.clone());
        }
        if changes.variables {
            changed_variable_collection_ids.push(id);
        }
        changed_requests.append(&mut changes.requests);
        changed_environment_ids.append(&mut changes.environments);
    }
    Ok(RepositoryRefresh {
        collections: read_collections(connection)?,
        changed_requests,
        changed_collection_ids,
        changed_environment_ids,
        changed_variable_collection_ids,
    })
}

#[derive(Default)]
struct Changes {
    shared: bool,
    variables: bool,
    requests: Vec<RepositoryRequestChange>,
    environments: Vec<String>,
}

fn refresh_link(
    connection: &mut Connection,
    collection_id: &str,
    link: &Link,
) -> Result<Changes, StorageError> {
    let root = Path::new(&link.root);
    let snapshot = (|| {
        let lock = lock_root(root)?;
        let watch = Watch::read(root);
        if watch
            .as_ref()
            .is_ok_and(|watch| link.watch_stamp.as_deref() == Some(watch.stamp().as_str()))
        {
            return Ok(None);
        }
        values::clean_legacy_staging(root)?;
        // A branch can remove the tracked ignore file while leaving local.json.
        let protection = if file_stamp(root, values::LOCAL)?.is_some() {
            values::ensure_ignore(root).and_then(|()| values::check_private(root, &[values::LOCAL]))
        } else if watch.is_ok() {
            values::ensure_ignore(root)
        } else {
            Ok(())
        };
        let watch = watch.map_err(|error| match &protection {
            Ok(()) => error,
            Err(protection) => input(format!("{error}\n{protection}")),
        })?;
        let stored = cache::stored_files(connection, collection_id, None)?;
        let old_groups = cache::groups(connection, collection_id)?;
        let (files, groups, mut issues) =
            scan(root, Some(&link.manifest_id), &stored, &old_groups)?;
        let local = match values::read(root) {
            Ok(local) => {
                values::report_orphans(&files, &local, &mut issues);
                local
            }
            Err(error) => {
                issues.push(format!("{INVALID_LOCAL}: {error}"));
                values::cached_local(connection, collection_id, &files)?
            }
        };
        if let Err(error) = protection {
            issues.push(error.to_string());
        }
        let after = Watch::read(root)?;
        if !watch.contents().eq(after.contents()) {
            return Err(input(
                "The repository changed during refresh. Focus the window again to retry.",
            ));
        }
        Ok(Some((
            lock,
            after.stamp(),
            stored,
            old_groups,
            files,
            groups,
            issues,
            local,
        )))
    })();
    let (_lock, stamp, stored, old_groups, files, groups, issues, local) = match snapshot {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => return Ok(Changes::default()),
        Err(error) => {
            let issue = Some(error.to_string());
            let changed = issue != link.issue;
            if changed || link.watch_stamp.is_some() {
                set_issue(connection, collection_id, issue.as_deref())?;
            }
            return Ok(Changes {
                shared: changed,
                ..Default::default()
            });
        }
    };
    let issue = issue_text(&issues);
    let tx = connection.savepoint()?;
    let mut changes = Changes::default();
    if files.len() != stored.len()
        || files.iter().any(|(path, file)| {
            stored
                .get(path)
                .is_none_or(|old| old.contents != file.contents)
        })
    {
        changes = cache::apply(&tx, collection_id, &files, &stored)?;
        changes.shared = true;
    }
    cache::record_groups(&tx, collection_id, &old_groups, &groups)?;
    changes.variables = values::cache(&tx, collection_id, &files, &local)?;
    tx.execute(
        "UPDATE repository_links SET issue = ?, watch_stamp = ? WHERE collection_id = ?",
        params![issue, stamp, collection_id],
    )?;
    tx.commit()?;
    changes.shared |= issue != link.issue;
    Ok(changes)
}

const INVALID_LOCAL: &str = ".conductor/local.json is unreadable";

fn repair_local() -> StorageError {
    input("Repair .conductor/local.json and refresh the repository before sending.")
}

pub(crate) fn private_needs_refresh_for_send(
    connection: &Connection,
    collection_id: &str,
) -> Result<bool, StorageError> {
    let Some(link) = read_link(connection, collection_id)? else {
        return Ok(false);
    };
    let current = file_stamp(Path::new(&link.root), values::LOCAL).map_err(|_| repair_local())?;
    let cached = link
        .watch_stamp
        .as_deref()
        .and_then(|stamp| stamp.split_once(':'))
        .map(|(_, local)| local);
    if !cached.is_some_and(|cached| current == cached.parse().ok()) {
        return Ok(true);
    }
    if link
        .issue
        .as_deref()
        .is_some_and(|issue| issue.lines().any(|line| line.starts_with(INVALID_LOCAL)))
    {
        return Err(repair_local());
    }
    Ok(false)
}

pub(crate) fn refresh_private_for_send(
    connection: &mut Connection,
    collection_id: &str,
) -> Result<(), StorageError> {
    if private_needs_refresh_for_send(connection, collection_id)? {
        let link = read_link(connection, collection_id)?.expect("checked linked collection");
        refresh_link(connection, collection_id, &link)?;
        if private_needs_refresh_for_send(connection, collection_id)? {
            return Err(repair_local());
        }
    }
    Ok(())
}

fn local_stamp(stamp: Option<u64>) -> String {
    stamp.map_or_else(String::new, |stamp| stamp.to_string())
}

fn watch_stamp(root: &Path) -> Result<String, StorageError> {
    Ok(Watch::read(root)?.stamp())
}

struct Watch(BTreeMap<String, u64>);

impl Watch {
    fn read(root: &Path) -> Result<Self, StorageError> {
        ensure_same_root(root)?;
        let mut files = BTreeMap::new();
        for path in std::iter::once(MANIFEST.to_string())
            .chain(format::list(root, "requests")?)
            .chain(format::list(root, "environments")?)
        {
            if path != MANIFEST && format::validate_readable_path(&path).is_err() {
                files.insert(path, 0);
                continue;
            }
            files.insert(
                path.clone(),
                file_stamp(root, &path)?
                    .ok_or_else(|| input("A repository file disappeared during refresh."))?,
            );
        }
        for path in [values::LOCAL, values::IGNORE] {
            if let Some(stamp) = file_stamp(root, path)? {
                files.insert(path.into(), stamp);
            }
        }
        Ok(Self(files))
    }

    fn shared(&self) -> impl Iterator<Item = (&String, &u64)> {
        self.0
            .iter()
            .filter(|(path, _)| !matches!(path.as_str(), values::LOCAL | values::IGNORE))
    }

    fn contents(&self) -> impl Iterator<Item = (&String, &u64)> {
        self.0
            .iter()
            .filter(|(path, _)| path.as_str() != values::IGNORE)
    }

    fn stamp(&self) -> String {
        let mut hasher = DefaultHasher::new();
        self.0.hash(&mut hasher);
        format!(
            "{}:{}",
            hasher.finish(),
            local_stamp(self.0.get(values::LOCAL).copied())
        )
    }

    fn matches_before(&self, root: &Path, edits: &[Edit]) -> bool {
        edits
            .iter()
            .all(|edit| file_stamp(root, &edit.path).ok() == Some(self.0.get(&edit.path).copied()))
    }

    fn after_edit(mut self, root: &Path, edits: &[Edit]) -> Option<String> {
        for edit in edits {
            let stamp = file_stamp(root, &edit.path).ok()?;
            if format::read(root, &edit.path).ok()?.as_deref() != edit.next.as_deref()
                || file_stamp(root, &edit.path).ok()? != stamp
            {
                return None;
            }
            if let Some(stamp) = stamp {
                self.0.insert(edit.path.clone(), stamp);
            } else {
                self.0.remove(&edit.path);
            }
        }
        Some(self.stamp())
    }
}

fn file_stamp(root: &Path, path: &str) -> Result<Option<u64>, StorageError> {
    let metadata = match fs::symlink_metadata(root.join(".conductor").join(path)) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io(error)),
    };
    let mut hasher = DefaultHasher::new();
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
    Ok(Some(hasher.finish()))
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
        format::parse_manifest(&contents).map_err(|error| format::at_path(MANIFEST, error))?;
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
    let mut folder_paths = HashSet::new();
    let mut unknown_group = false;
    let mut duplicate = false;
    let mut candidates: HashMap<String, Vec<(String, Option<format::Request>)>> = HashMap::new();
    let mut failed_folders = HashSet::new();
    let mut invalid_requests = HashMap::new();
    let environment_file = |contents: Vec<u8>| -> Result<_, StorageError> {
        let entity = Entity::Environment(format::parse_environment(&contents)?);
        Ok(File { contents, entity })
    };
    let mut environments: HashMap<String, Vec<(String, File)>> = HashMap::new();
    let paths = format::list(root, "requests")?
        .into_iter()
        .chain(format::list(root, "environments")?);
    for path in paths {
        if let Err(error) = format::validate_readable_path(&path) {
            issues.push(error.to_string());
            unknown_group |= path.starts_with("requests/");
            continue;
        }
        let Some(read) = format::read(root, &path).transpose() else {
            continue;
        };
        if let Ok(bytes) = &read {
            bytes_read += bytes.len();
            if bytes_read > MAX_REPOSITORY_BYTES + format::MAX_FILE_BYTES as usize {
                return Err(input(REPOSITORY_SIZE_ERROR));
            }
        }
        if path.starts_with("environments/") {
            let parsed = read.and_then(environment_file);
            let file = match parsed {
                Ok(file) => file,
                Err(error) => {
                    issues.push(format::at_path(&path, error).to_string());
                    let Some(old) = stored.get(&path) else {
                        continue;
                    };
                    environment_file(old.contents.clone())?
                }
            };
            environments
                .entry(file.entity.id().into())
                .or_default()
                .push((path, file));
            continue;
        }
        let parsed = read.and_then(|contents| {
            let parsed = group::parse(&contents)?;
            if (path == "requests/root.json") != parsed.folder_id.is_none() {
                return Err(input("Only requests/root.json can omit folderId."));
            }
            if parsed
                .folder_id
                .as_deref()
                .is_some_and(|folder| !parents.contains(folder))
            {
                return Err(input("Folder ID is missing from collection.json."));
            }
            Ok((contents.len() as u32, parsed))
        });
        let (byte_count, parsed) = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                issues.push(format::at_path(&path, error).to_string());
                if let Some(old) = old_groups.get(&path) {
                    if old.folder_id.is_empty() || parents.contains(old.folder_id.as_str()) {
                        failed_folders.insert(old.folder_id.clone());
                        groups.insert(path, old.clone());
                    }
                } else {
                    unknown_group = true;
                }
                continue;
            }
        };
        let folder_id = parsed.folder_id.unwrap_or_default();
        if !folder_paths.insert(folder_id.clone()) {
            duplicate = true;
            issues.push(format!("{path}: Folder has more than one request file."));
        }
        if let Some(error) = parsed.identity_issue {
            failed_folders.insert(folder_id.clone());
            issues.push(format!("{path}: {error}"));
        }
        for key in parsed.duplicate_keys {
            issues.push(format!("{path}: Duplicate request key {key:?}. Saving the group will assign distinct keys."));
        }
        for (id, request) in parsed.requests {
            let request = match request {
                Ok(request) => Some(request),
                Err(error) => {
                    issues.push(format!("{path} [{id}]: {error}"));
                    invalid_requests
                        .entry(id.clone())
                        .or_insert_with(|| (!folder_id.is_empty()).then(|| folder_id.clone()));
                    None
                }
            };
            candidates
                .entry(id)
                .or_default()
                .push((path.clone(), request));
        }
        groups.insert(
            path,
            RequestGroup {
                folder_id,
                byte_count,
            },
        );
    }

    // An unreadable file at a new path could be a renamed group. A missing
    // path is only a confirmed removal once the new groups can be identified.
    if unknown_group {
        for (path, old) in old_groups {
            if !folder_paths.contains(&old.folder_id)
                && (old.folder_id.is_empty() || parents.contains(old.folder_id.as_str()))
            {
                failed_folders.insert(old.folder_id.clone());
                groups.entry(path.clone()).or_insert_with(|| old.clone());
            }
        }
    }

    for (id, entries) in candidates {
        let winner = entries
            .iter()
            .enumerate()
            .filter_map(|(index, (path, request))| {
                let request = request.as_ref()?;
                if failed_folders.contains(request.parent_id.as_deref().unwrap_or_default()) {
                    return None;
                }
                Some((
                    std::cmp::Reverse(request.move_generation),
                    placeholder_group(path),
                    path,
                    index,
                ))
            })
            .min()
            .map(|(_, _, _, index)| index);
        if entries.len() > 1 {
            duplicate = true;
            let selection = winner.map_or_else(String::new, |index| {
                format!(" (using {})", entries[index].0)
            });
            for (index, (path, _)) in entries.iter().enumerate() {
                if Some(index) != winner {
                    issues.push(format!("{path} [{id}]: Duplicate request ID{selection}."));
                }
            }
        }
        if let Some(index) = winner {
            let (_, request) = entries.into_iter().nth(index).expect("selected entry");
            cache::put_request(&mut files, request.expect("selected valid request"))?;
        }
    }

    // A valid ID in a malformed member still identifies the user's last
    // readable request, including after a move to a different group.
    for (id, parent_id) in invalid_requests {
        let key = cache::request_key(&id);
        if let Some(old) = stored.get(&key).filter(|_| !files.contains_key(&key)) {
            let mut request: format::Request = serde_json::from_slice(&old.contents)?;
            request.parent_id = parent_id;
            cache::put_request(&mut files, request)?;
        }
    }

    if !failed_folders.is_empty() {
        for old in stored.values().filter(|old| old.kind == "request") {
            let request = serde_json::from_slice::<format::Request>(&old.contents)?;
            if failed_folders.contains(request.parent_id.as_deref().unwrap_or_default())
                && !files.contains_key(&cache::request_key(&request.id))
            {
                cache::put_request(&mut files, request)?;
            }
        }
    }

    for (id, entries) in environments {
        if entries.len() == 1 {
            files.extend(entries);
        } else {
            for (path, _) in entries {
                issues.push(format!("{path}: Duplicate environment ID {id}."));
                if let Some(old) = stored.get(&path).filter(|old| remote_id(&old.id) == id) {
                    files.insert(path, environment_file(old.contents.clone())?);
                }
            }
        }
    }
    if bytes_read > MAX_REPOSITORY_BYTES && !duplicate {
        return Err(input(REPOSITORY_SIZE_ERROR));
    }
    files.insert(MANIFEST.into(), manifest);
    issues.sort();
    Ok((files, groups, issues))
}

/// Reuse the ordinary SQL operation in the linked cache. Compare its shared
/// projection before and after, so a local-only edit or untouched hand-edited
/// file is never rewritten.
pub(crate) fn mutate<T>(
    connection: &mut Connection,
    collection_id: &str,
    operation: impl FnOnce(&mut Connection) -> Result<T, StorageError>,
) -> Result<T, StorageError> {
    mutate_scoped(connection, collection_id, None, operation).map(|(result, _)| result)
}

pub(crate) fn mutate_scoped<T>(
    connection: &mut Connection,
    collection_id: &str,
    request_id: Option<&str>,
    operation: impl FnOnce(&mut Connection) -> Result<T, StorageError>,
) -> Result<(T, Option<String>), StorageError> {
    let Some(link) = read_link(connection, collection_id)? else {
        return operation(connection).map(|result| (result, None));
    };
    let root = Path::new(&link.root);
    let _lock = lock_root(root)?;
    let watch = link.watch_stamp.as_ref().and_then(|stamp| {
        let watch = Watch::read(root).ok()?;
        (watch.stamp().as_str() == stamp).then_some(watch)
    });
    let stored = cache::stored_files(connection, collection_id, request_id)?;
    let groups = cache::groups(connection, collection_id)?;
    let project = |connection: &Connection| {
        cache::project(
            connection,
            collection_id,
            &link.manifest_id,
            &stored,
            request_id,
            Some(root),
        )
    };
    let before = project(connection)?;
    connection.execute_batch("SAVEPOINT repository_edit")?;
    let result = (|| {
        let result = operation(connection)?;
        let after = project(connection)?;
        let (edited, next_groups, edits) = plan_projection(
            root,
            &before,
            &after,
            &stored,
            &groups,
            watch.is_none()
                || link.issue.as_deref().is_some_and(|issue| {
                    issue.contains(": Duplicate request ID")
                        || issue.contains(": Folder has more than one request file")
                }),
        )?;
        cache::record_groups(connection, collection_id, &groups, &next_groups)?;
        for path in edited {
            match after.get(&path) {
                Some(file) => {
                    cache::record_file(connection, collection_id, &path, file, stored.get(&path))?;
                }
                None => cache::remove_file(connection, collection_id, &path)?,
            }
        }
        let revision = request_id
            .map(|id| cache::revision(connection, id))
            .transpose()?
            .flatten();
        // Local values for new environments are staged in the savepoint until
        // the projected files have unique stems and the shared writes are planned.
        let environment_edits = edits
            .iter()
            .filter(|edit| edit.path.starts_with("environments/"));
        values::write_new_environments(
            connection,
            collection_id,
            root,
            environment_edits
                .clone()
                .filter(|edit| edit.old.is_none())
                .map(|edit| (edit.path.as_str(), &after[&edit.path])),
        )?;
        let membership_changed = environment_edits
            .clone()
            .any(|edit| edit.old.is_none() || edit.next.is_none());
        let watch = watch.filter(|watch| {
            !membership_changed
                && !link
                    .issue
                    .as_deref()
                    .is_some_and(|issue| issue.contains(": Duplicate request key "))
                && watch.matches_before(root, &edits)
        });
        apply_edits(root, &edits, || {
            if !edits.is_empty() {
                let stamp = watch.and_then(|watch| watch.after_edit(root, &edits));
                connection.execute(
                    "UPDATE repository_links SET watch_stamp = ? WHERE collection_id = ?",
                    params![stamp, collection_id],
                )?;
            }
            connection.execute_batch("RELEASE repository_edit")?;
            Ok(())
        })?;
        Ok((result, revision))
    })();
    if result.is_err() && !connection.is_autocommit() {
        connection.execute_batch("ROLLBACK TO repository_edit; RELEASE repository_edit;")?;
    }
    result
}

fn apply_edits(
    root: &Path,
    edits: &[Edit],
    finish: impl FnOnce() -> Result<(), StorageError>,
) -> Result<(), StorageError> {
    let mut backups = Vec::new();
    let mut applied = 0;
    let result = (|| {
        // Make every group recoverable before changing the working copies.
        for (index, edit) in edits.iter().enumerate() {
            if edit.order == 0 && edit.next.is_none() {
                let (path, backup) = format::backup_group(root, &edit.path)?;
                backups.push((index, path, backup));
            }
        }
        format::sync_directories(
            root,
            backups
                .iter()
                .map(|(index, _, _)| edits[*index].path.as_str()),
        )?;
        for (index, edit) in edits.iter().enumerate() {
            edit.apply(root)?;
            applied = index + 1;
            if applied < edits.len() {
                format::sync_directories(root, [edit.path.as_str()])?;
            }
        }
        Ok::<_, StorageError>(())
    })();
    if result.is_ok() {
        if backups.is_empty() {
            return finish();
        }
        // Keep the recovery links through the manifest sync and cache commit.
        if let Err(error) = format::sync_directories(root, [MANIFEST]).and_then(|()| finish()) {
            let locations: Vec<_> = backups
                .iter()
                .map(|(_, _, backup)| backup.display().to_string())
                .collect();
            return Err(input(format!(
                "{error}\nRequest groups can be recovered from:\n{}",
                locations.join("\n")
            )));
        }
    }
    let mut failures = Vec::new();
    for (index, path, backup) in backups.into_iter().rev() {
        if result.is_err() && index < applied {
            let restored = fs::hard_link(&backup, &path)
                .map_err(io)
                .and_then(|()| format::sync_directories(root, [edits[index].path.as_str()]));
            if let Err(error) = restored {
                failures.push(format!(
                    "{} can be recovered from {} ({error})",
                    path.display(),
                    backup.display()
                ));
                continue;
            }
        }
        let _ = fs::remove_file(backup);
    }
    match result {
        Err(error) if !failures.is_empty() => {
            Err(input(format!("{error}\n{}", failures.join("\n"))))
        }
        result => result,
    }
}

pub(crate) const NEW_REQUEST: &str = "New Request";
pub(crate) const NEW_FOLDER: &str = "New Folder";

fn placeholder_key(key: &str, placeholder: &str) -> bool {
    key == placeholder
        || key
            .strip_prefix(placeholder)
            .and_then(|rest| rest.strip_prefix('-'))
            .is_some_and(|suffix| {
                (!suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
                    || Uuid::parse_str(suffix).is_ok()
            })
}

fn placeholder_group(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".json")
        .is_some_and(|stem| placeholder_key(stem, "new-folder"))
}

fn is_duplicate(issue: Option<&str>, request_id: &str) -> bool {
    let marker = format!(" [{}]: Duplicate request ID", remote_id(request_id));
    issue.is_some_and(|issue| issue.lines().any(|line| line.contains(&marker)))
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

fn plan_projection(
    root: &Path,
    before: &Files,
    after: &Files,
    stored: &StoredFiles,
    groups: &Groups,
    check_ownership: bool,
) -> Result<(Vec<String>, Groups, Vec<Edit>), StorageError> {
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
    let mut moves = Vec::new();
    let mut move_sources = HashSet::new();
    let mut request_groups: BTreeMap<&str, BTreeMap<String, Option<&format::Request>>> =
        BTreeMap::new();
    let baseline = |path: &str| stored.get(path).map(|file| file.contents.as_slice());
    let mut manifest_baseline = format::read(root, MANIFEST)?;
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
            if let (Some(old), Some(new)) = (old_request, new_request) {
                if old.parent_id != new.parent_id {
                    let source = old.parent_id.as_deref().unwrap_or_default();
                    moves.push((
                        old.id.as_str(),
                        source,
                        new.parent_id.as_deref().unwrap_or_default(),
                    ));
                    move_sources.insert(source);
                }
            }
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
        let old = if path == MANIFEST {
            manifest_baseline.take()
        } else {
            format::read(root, path)?
        };
        if old.as_deref() != baseline(path) {
            return Err(input(format!(
                "{path} has changed on disk. Refresh the repository before editing."
            )));
        }
        edits.push(Edit {
            order: 1,
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
    let previous: HashMap<_, _> = old_folders
        .iter()
        .map(|folder| (folder.id.as_str(), folder))
        .collect();
    let by_id: HashMap<_, _> = folders
        .iter()
        .map(|folder| (folder.id.as_str(), folder))
        .collect();
    if changed.iter().any(|path| path == MANIFEST) {
        for folder in previous
            .keys()
            .filter(|folder| !by_id.contains_key(*folder))
        {
            request_groups.entry(folder).or_default();
        }
        for (id, folder) in &by_id {
            if !folder.name.eq_ignore_ascii_case(NEW_FOLDER)
                && previous
                    .get(id)
                    .is_none_or(|old| old.name.eq_ignore_ascii_case(NEW_FOLDER))
            {
                request_groups.entry(id).or_default();
            }
        }
    }
    if before.is_empty() {
        request_groups.entry("").or_default();
    }

    let mut next_groups = groups.clone();
    let needs_rekey = |folder: &str, path: &str| {
        let stem = path
            .rsplit('/')
            .next()
            .and_then(|name| name.strip_suffix(".json"));
        placeholder_group(path)
            && previous
                .get(folder)
                .is_some_and(|item| item.name.eq_ignore_ascii_case(NEW_FOLDER))
            && by_id.get(folder).is_some_and(|item| {
                !item.name.eq_ignore_ascii_case(NEW_FOLDER)
                    && stem != Some(format::slug(&item.name).as_str())
            })
    };
    let needs_paths = request_groups.keys().any(|folder| {
        !folder.is_empty()
            && by_id.contains_key(folder)
            && groups
                .iter()
                .find(|(_, group)| group.folder_id == *folder)
                .is_none_or(|(path, _)| needs_rekey(folder, path))
    });
    let disk = (needs_paths || (check_ownership && !request_groups.is_empty()))
        .then(|| format::list(root, "requests"))
        .transpose()?
        .unwrap_or_default();
    let mut readable_paths = needs_paths.then(|| ReadablePaths::new(&disk, groups));
    let mut loaded = HashMap::new();
    if check_ownership {
        let touched: HashMap<_, _> = request_groups
            .values()
            .flatten()
            .map(|(id, _)| {
                let expected_folder = request(before.get(&cache::request_key(id)))
                    .map(|request| request.parent_id.as_deref().unwrap_or_default());
                (id.as_str(), expected_folder)
            })
            .collect();
        for path in &disk {
            if format::validate_readable_path(path).is_err() {
                continue;
            }
            let parsed = read_group(root, path);
            if let Ok(Some((_, group))) = &parsed {
                let folder = group.folder_id.as_deref().unwrap_or_default();
                if group.requests.keys().any(|id| {
                    touched.get(id.as_str()).is_some_and(|expected| {
                        *expected != Some(folder)
                            || group_path(groups, folder).ok().flatten() != Some(path)
                    })
                }) {
                    return Err(input(format!("{path}: An edited request has another on-disk owner. Refresh the repository before editing.")));
                }
                if !groups.contains_key(path) && request_groups.contains_key(folder) {
                    return Err(input(format!("{path}: A request group was added on disk. Refresh the repository before editing.")));
                }
            }
            if groups
                .get(path)
                .is_some_and(|group| request_groups.contains_key(group.folder_id.as_str()))
            {
                loaded.insert(path.clone(), parsed);
            }
        }
    }

    let mut move_keys = HashMap::new();
    for (id, source, _) in &moves {
        let path = group_path(groups, source)?
            .ok_or_else(|| input("The source request group was removed. Refresh the repository before moving a request."))?;
        if !loaded.contains_key(path) {
            loaded.insert(path.to_owned(), read_group(root, path));
        }
        let key = loaded
            .get(path)
            .and_then(|group| group.as_ref().ok())
            .and_then(Option::as_ref)
            .and_then(|(_, group)| group.key(id))
            .ok_or_else(|| {
                input(format!(
                    "{path} [{id}]: The source request could not be read."
                ))
            })?;
        move_keys.insert(*id, key.to_owned());
    }
    let mut destinations = HashSet::new();
    for (_, _, destination) in moves {
        destinations.insert(destination);
    }
    for (folder, requests) in request_groups {
        let definition = by_id.get(folder);
        let removed = !folder.is_empty() && definition.is_none();
        let existing_path = group_path(groups, folder)?;
        let path = if let Some(path) = existing_path {
            path.to_string()
        } else if folder.is_empty() {
            "requests/root.json".into()
        } else if let Some(item) = definition {
            let paths = readable_paths.as_mut().expect("paths initialized");
            // A group created outside Conductor could have appeared between
            // the previous refresh and the first save in this folder.
            if !check_ownership {
                for path in disk.iter().filter(|path| !groups.contains_key(*path)) {
                    if format::validate_readable_path(path).is_ok()
                        && format::read(root, path)?.is_some_and(|contents| {
                            group::folder_id(&contents)
                                .is_ok_and(|id| id.as_deref() == Some(folder))
                        })
                    {
                        return Err(input(format!("{path}: A request group was added on disk. Refresh the repository before editing.")));
                    }
                }
            }
            paths.for_folder(item, &by_id)
        } else {
            continue;
        };
        let previous = loaded
            .remove(&path)
            .unwrap_or_else(|| read_group(root, &path))?;
        if previous.is_none() && existing_path.is_some() {
            return Err(input(format!(
                "{path} was removed on disk. Refresh the repository before editing."
            )));
        }
        if let Some((_, actual)) = &previous {
            if let Some(error) = &actual.identity_issue {
                return Err(input(format!("{path}: {error}")));
            }
            if actual.folder_id.as_deref().unwrap_or_default() != folder || (removed && actual.requests.len() != requests.len()) {
                return Err(input(format!("{path} has changed on disk. Refresh the repository before editing.")));
            }
            for id in requests.keys() {
                let expected = before.get(&cache::request_key(id)).and_then(|file| match &file.entity {
                    Entity::Request(request) if request.parent_id.as_deref().unwrap_or_default() == folder => Some(file.contents.as_slice()),
                    _ => None,
                });
                let latest = actual.request_bytes(id).map_err(|error| input(format!("{path} [{id}]: {error}")))?;
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
            Some(group::edit_with_keys(
                previous
                    .as_ref()
                    .map(|(bytes, group)| (bytes.as_slice(), group)),
                (!folder.is_empty()).then_some(folder),
                &requests,
                &move_keys,
            )?)
        };
        let old = previous.map(|(bytes, _)| bytes);
        if needs_rekey(folder, &path) && old.is_some() {
            let paths = readable_paths.as_mut().expect("paths initialized");
            paths.paths.remove(folder);
            let renamed = paths.for_folder(by_id[folder], &by_id);
            let next = next.expect("renamed group");
            next_groups.remove(&path);
            next_groups.insert(
                renamed.clone(),
                RequestGroup {
                    folder_id: folder.to_string(),
                    byte_count: next.len() as u32,
                },
            );
            edits.push(Edit {
                order: 0,
                path: renamed,
                old: None,
                next: Some(next),
            });
            edits.push(Edit {
                order: 3,
                path,
                old,
                next: None,
            });
        } else if old != next {
            if let Some(bytes) = &next {
                next_groups.insert(
                    path.clone(),
                    RequestGroup {
                        folder_id: folder.to_string(),
                        byte_count: bytes.len() as u32,
                    },
                );
            } else {
                next_groups.remove(&path);
            }
            let priority = if removed || destinations.contains(folder) {
                0
            } else if move_sources.contains(folder) {
                3
            } else {
                2
            };
            edits.push(Edit {
                order: priority,
                path,
                old,
                next,
            });
        }
    }
    let mut bytes = next_groups
        .values()
        .map(|group| group.byte_count as usize)
        .sum::<usize>()
        + stored
            .values()
            .filter(|file| file.kind == "environment")
            .map(|file| file.contents.len())
            .sum::<usize>();
    for edit in edits
        .iter()
        .filter(|edit| edit.path.starts_with("environments/"))
    {
        bytes -= baseline(&edit.path).map_or(0, <[u8]>::len);
        bytes += edit.next.as_ref().map_or(0, Vec::len);
    }
    if bytes > MAX_REPOSITORY_BYTES {
        return Err(input(REPOSITORY_SIZE_ERROR));
    }

    // Remove obsolete groups before their manifest entries. Add manifest
    // entries before creating their groups. Move copies precede removals.
    edits.sort_by_key(|edit| edit.order);
    Ok((changed, next_groups, edits))
}

fn read_group(root: &Path, path: &str) -> Result<Option<(Vec<u8>, group::Group)>, StorageError> {
    format::read(root, path)?
        .map(|bytes| {
            let group = group::parse(&bytes).map_err(|error| format::at_path(path, error))?;
            Ok((bytes, group))
        })
        .transpose()
}

fn group_path<'a>(groups: &'a Groups, folder: &str) -> Result<Option<&'a str>, StorageError> {
    let mut found = groups.iter().filter(|(_, value)| value.folder_id == folder);
    let first = found.next().map(|(path, _)| path.as_str());
    if found.next().is_some() {
        return Err(input("A folder has more than one request file. Fix and refresh the repository before editing."));
    }
    Ok(first)
}

struct ReadablePaths<'a> {
    paths: HashMap<&'a str, String>,
    slugs: format::Slugs,
}

impl<'a> ReadablePaths<'a> {
    fn new(disk: &[String], groups: &'a Groups) -> Self {
        let mut slugs = format::Slugs::default();
        slugs.reserve("requests/root");
        for path in disk.iter().chain(groups.keys()) {
            let path = path.to_ascii_lowercase();
            let Some(mut directory) = path.strip_suffix(".json") else {
                continue;
            };
            slugs.reserve(directory);
            while let Some((parent, _)) = directory.rsplit_once('/') {
                if parent == "requests" {
                    break;
                }
                slugs.reserve(parent);
                directory = parent;
            }
        }
        Self {
            paths: groups
                .iter()
                .map(|(path, group)| (group.folder_id.as_str(), path.clone()))
                .collect(),
            slugs,
        }
    }

    fn for_folder(
        &mut self,
        folder: &'a format::Folder,
        folders: &HashMap<&str, &'a format::Folder>,
    ) -> String {
        if let Some(path) = self.paths.get(folder.id.as_str()) {
            return path.clone();
        }
        let parent = folder.parent_id.as_deref().and_then(|id| folders.get(id));
        let parent_path = parent.map(|parent| self.for_folder(parent, folders));
        let directory = parent_path
            .as_deref()
            .and_then(|path| path.strip_suffix(".json"))
            .filter(|path| format::can_nest_in(path))
            .unwrap_or("requests");
        let path = if folder.name.eq_ignore_ascii_case(NEW_FOLDER) {
            self.slugs
                .file(directory, &format!("new-folder-{}", folder.id))
        } else {
            self.slugs.file(directory, &folder.name)
        };
        self.paths.insert(&folder.id, path.clone());
        path
    }
}

pub(crate) enum RequestBaseline {
    Local,
    Current {
        refresh: Option<RepositoryRefresh>,
    },
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
    let root = Path::new(&link.root);
    let stamp = watch_stamp(root)?;
    let (refresh, issue) = if link.watch_stamp.as_deref() == Some(stamp.as_str()) {
        (None, link.issue)
    } else {
        let refresh = refresh_all(connection)?;
        let current = read_link(connection, collection_id)?.expect("checked linked collection");
        (Some(refresh), current.issue)
    };
    let duplicate = is_duplicate(issue.as_deref(), request_id);
    let revision = cache::revision(connection, request_id)?;
    Ok(
        if duplicate || revision.is_none() || revision.as_deref() != expected {
            RequestBaseline::Conflict { revision, refresh }
        } else {
            RequestBaseline::Current { refresh }
        },
    )
}

pub(super) fn summary_link(root: Option<String>, issue: Option<String>) -> Option<RepositoryLink> {
    root.map(|root| RepositoryLink { root, issue })
}
