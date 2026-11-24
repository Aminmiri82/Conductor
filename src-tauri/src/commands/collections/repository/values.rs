use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{de, Deserialize, Deserializer, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    commands::models::{
        SharedVariableChange, VariableChange, VariableEntry, VariableOrigin, VariableTarget,
    },
    storage::StorageError,
};

use super::{
    cache::{Entity, File, Files},
    format, input, local_id, lock_root, read_manifest, remote_id, MANIFEST,
};

pub(crate) const LOCAL: &str = "local.json";
pub(super) const IGNORE: &str = ".gitignore";
const IGNORE_RULES: [&str; 2] = ["/local.json", "/.local.json-*.tmp"];

type Entries = BTreeMap<String, LocalValue>;

#[derive(Clone, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LocalValues {
    #[serde(
        default,
        deserialize_with = "format::unique_keys",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    collection: Entries,
    #[serde(
        default,
        deserialize_with = "format::unique_keys",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    environments: BTreeMap<String, EnvironmentEntries>,
}

#[derive(Clone, Default, Deserialize, PartialEq, Serialize)]
struct EnvironmentEntries(#[serde(deserialize_with = "format::unique_keys")] Entries);

#[derive(Clone, PartialEq, Serialize)]
#[serde(untagged)]
enum LocalValue {
    Plain(String),
    Options(ValueOptions),
}

impl<'de> Deserialize<'de> for LocalValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct LocalEntry;
        impl<'de> de::Visitor<'de> for LocalEntry {
            type Value = LocalValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or an object with local value options")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(LocalValue::Plain(value.to_owned()))
            }

            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(LocalValue::Plain(value))
            }

            fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                ValueOptions::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(LocalValue::Options)
            }
        }
        deserializer.deserialize_any(LocalEntry)
    }
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ValueOptions {
    value: String,
    #[serde(default = "enabled", skip_serializing_if = "is_enabled")]
    enabled: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    sensitive: bool,
}

fn enabled() -> bool {
    true
}

fn is_enabled(value: &bool) -> bool {
    *value
}

impl LocalValue {
    fn new(value: String, enabled: bool, sensitive: bool) -> Self {
        if enabled && !sensitive {
            Self::Plain(value)
        } else {
            Self::Options(ValueOptions {
                value,
                enabled,
                sensitive,
            })
        }
    }

    fn fields(&self) -> (&str, bool, bool) {
        match self {
            Self::Plain(value) => (value, true, false),
            Self::Options(value) => (&value.value, value.enabled, value.sensitive),
        }
    }
}

impl LocalValues {
    fn entries(&self, environment: Option<&str>) -> Option<&Entries> {
        match environment {
            Some(stem) => self.environments.get(stem).map(|entries| &entries.0),
            None => Some(&self.collection),
        }
    }

    fn entries_mut(&mut self, environment: Option<&str>) -> &mut Entries {
        match environment {
            Some(stem) => &mut self.environments.entry(stem.into()).or_default().0,
            None => &mut self.collection,
        }
    }
}

fn environment_stem(path: &str) -> &str {
    path.strip_prefix("environments/")
        .and_then(|name| name.strip_suffix(".json"))
        .expect("environment files use environments/<stem>.json")
}

pub(super) fn reserve_environment_paths(
    root: &Path,
    slugs: &mut format::Slugs,
) -> Result<(), StorageError> {
    for stem in read(root)?.environments.keys() {
        slugs.reserve(&format!("environments/{stem}"));
    }
    Ok(())
}

pub(super) fn report_orphans(files: &Files, local: &LocalValues, issues: &mut Vec<String>) {
    for stem in local.environments.keys() {
        if !matches!(
            files
                .get(&format!("environments/{stem}.json"))
                .map(|file| &file.entity),
            Some(Entity::Environment(_))
        ) {
            issues.push(format!(
                ".conductor/local.json: environment key {stem:?} is orphaned. Rename the key to match an environment filename stem to use its values."
            ));
        }
    }
}

pub(super) fn ensure_ignore(root: &Path) -> Result<(), StorageError> {
    let before = format::read(root, IGNORE)?;
    let bytes = before.as_deref().unwrap_or_default();
    let text =
        std::str::from_utf8(bytes).map_err(|_| input(".conductor/.gitignore is not UTF-8."))?;
    let mut after = text.to_owned();
    for rule in IGNORE_RULES {
        if text.lines().any(|line| line == rule) {
            continue;
        }
        if !after.is_empty() && !after.ends_with('\n') {
            after.push('\n');
        }
        after.push_str(rule);
        after.push('\n');
    }
    if after != text {
        format::replace(root, IGNORE, before.as_deref(), Some(after.as_bytes()))?;
    }
    Ok(())
}

fn git(root: &Path, arguments: &[&str]) -> std::io::Result<Output> {
    Command::new("git")
        .current_dir(root)
        .args(["-c", "core.fsmonitor=false"])
        .args(arguments)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .output()
}

pub(super) fn check_private(root: &Path, names: &[&str]) -> Result<(), StorageError> {
    let paths: Vec<_> = names
        .iter()
        .map(|name| format!(".conductor/{name}"))
        .collect();
    let path_args = paths.iter().map(String::as_str);
    if inside_git(root)? {
        let tracked_paths: Vec<_> = paths
            .iter()
            .map(|path| format!(":(icase,literal){path}"))
            .collect();
        let args: Vec<_> = ["ls-files", "--"]
            .into_iter()
            .chain(tracked_paths.iter().map(String::as_str))
            .collect();
        let tracked = git(root, &args).map_err(|_| private_error())?;
        if !tracked.status.success() || !tracked.stdout.is_empty() {
            return Err(private_error());
        }

        let args: Vec<_> = ["check-ignore", "--"]
            .into_iter()
            .chain(path_args)
            .collect();
        let ignored = git(root, &args).map_err(|_| private_error())?;
        if ignored.status.success()
            && paths.iter().all(|path| {
                ignored
                    .stdout
                    .split(|&byte| byte == b'\n')
                    .any(|line| line == path.as_bytes())
            })
        {
            return Ok(());
        }
        return Err(private_error());
    }

    // In a plain directory, recognize our own rules so an initialization
    // later still ignores these files.
    let ignore = format::read(root, IGNORE)?.ok_or_else(private_error)?;
    let ignore = std::str::from_utf8(&ignore).map_err(|_| private_error())?;
    let final_rules = ignore
        .lines()
        .rev()
        .take_while(|line| !line.starts_with('!'));
    if IGNORE_RULES
        .into_iter()
        .all(|rule| final_rules.clone().any(|line| line == rule))
    {
        Ok(())
    } else {
        // Without Git, only our two final unconditional rules establish that
        // a preceding negation or wildcard cannot expose the file.
        Err(private_error())
    }
}

fn inside_git(root: &Path) -> Result<bool, StorageError> {
    for ancestor in root.ancestors() {
        match fs::symlink_metadata(ancestor.join(".git")) {
            Ok(_) => return Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(private_error()),
        }
    }
    Ok(false)
}

pub(super) fn staging_directories(root: &Path) -> Result<Vec<PathBuf>, StorageError> {
    let git_path = |argument| -> Result<PathBuf, StorageError> {
        let result = git(root, &["rev-parse", argument]).map_err(|_| private_error())?;
        if !result.status.success() {
            return Err(private_error());
        }
        let path = std::str::from_utf8(&result.stdout).map_err(|_| private_error())?;
        let path = path.strip_suffix('\n').ok_or_else(private_error)?;
        fs::canonicalize(path).map_err(|_| private_error())
    };
    let mut directories = Vec::new();
    let worktree = if inside_git(root)? {
        let worktree = git_path("--show-toplevel")?;
        let metadata = git_path("--absolute-git-dir")?;
        // A configured Git dir inside a tracked subdirectory is unsafe.
        // The standard .git and linked-worktree metadata are excluded by Git.
        if metadata == worktree.join(".git")
            || (!metadata.starts_with(&worktree)
                && (metadata.components().any(|part| part.as_os_str() == ".git")
                    || !inside_git(&metadata)?))
        {
            directories.push(metadata);
        }
        worktree
    } else {
        root.to_path_buf()
    };
    let temp = fs::canonicalize(std::env::temp_dir()).ok();
    for candidate in worktree.parent().into_iter().chain(temp.as_deref()) {
        if !candidate.starts_with(&worktree)
            && !inside_git(candidate)?
            && !directories.iter().any(|existing| existing == candidate)
        {
            directories.push(candidate.to_path_buf());
        }
    }
    Ok(directories)
}

pub(super) fn clean_legacy_staging(root: &Path) -> Result<(), StorageError> {
    let Some(directory) = format::real_directory(root.join(".conductor"), false)? else {
        return Ok(());
    };
    let entries = fs::read_dir(&directory).map_err(super::io)?;
    for entry in entries {
        let entry = entry.map_err(super::io)?;
        let name = entry.file_name();
        let legacy = name
            .to_str()
            .and_then(|name| name.strip_prefix(".local.json-"))
            .and_then(|name| name.strip_suffix(".tmp"))
            .is_some_and(|id| Uuid::parse_str(id).is_ok());
        if legacy && entry.file_type().map_err(super::io)?.is_file() {
            fs::remove_file(entry.path()).map_err(super::io)?;
        }
    }
    Ok(())
}

fn private_error() -> StorageError {
    input("Cannot write private values. Make sure .conductor/local.json and its temporary files are ignored by Git and untracked.")
}

fn read_document(root: &Path) -> Result<(Option<Vec<u8>>, LocalValues), StorageError> {
    let bytes = format::read(root, LOCAL)?;
    let mut document = LocalValues::default();
    if let Some(contents) = &bytes {
        document = format::decode_local_json(contents)?;
        for entries in document.environments.values() {
            validate_entries(&entries.0)?;
        }
        validate_entries(&document.collection)?;
    }
    Ok((bytes, document))
}

fn validate_entries(entries: &Entries) -> Result<(), StorageError> {
    if entries.keys().any(|key| !format::valid_variable_key(key)) {
        return Err(input(
            ".conductor/local.json: variable keys must be nonempty and trimmed.",
        ));
    }
    Ok(())
}

pub(super) fn read(root: &Path) -> Result<LocalValues, StorageError> {
    Ok(read_document(root)?.1)
}

fn write(
    root: &Path,
    bytes: Option<&[u8]>,
    before: &LocalValues,
    after: &LocalValues,
) -> Result<(), StorageError> {
    if before == after {
        return Ok(());
    }
    let next = if let Some(bytes) = bytes {
        update_json(bytes, before, after)?
    } else {
        format::encode(after)?
    };
    format::replace(root, LOCAL, bytes, Some(&next))?;
    format::sync_directories(root, [LOCAL])
}

fn update_json<T: Serialize>(bytes: &[u8], before: &T, after: &T) -> Result<Vec<u8>, StorageError> {
    let mut next = Vec::with_capacity(bytes.len());
    next.extend_from_slice(&bytes[..bytes.len() - bytes.trim_ascii_start().len()]);
    merge_json(
        &mut next,
        bytes.trim_ascii(),
        &serde_json::to_value(before)?,
        &serde_json::to_value(after)?,
        0,
    )?;
    next.extend_from_slice(&bytes[bytes.trim_ascii_end().len()..]);
    Ok(next)
}

fn merge_json(
    output: &mut Vec<u8>,
    bytes: &[u8],
    before: &Value,
    after: &Value,
    depth: usize,
) -> Result<(), StorageError> {
    let (Some(before), Some(after)) = (before.as_object(), after.as_object()) else {
        return write_json(output, after, bytes.contains(&b'\n'), depth);
    };
    let members = format::json_members(bytes, std::convert::identity)?;
    let multiline = bytes.contains(&b'\n');
    output.push(b'{');
    let mut count = 0;
    for member in &members {
        let previous = before.get(&member.key);
        let next = after.get(&member.key);
        if previous == next || next.is_some() {
            format::write_separator(output, &bytes[member.separator.clone()], count > 0);
            count += 1;
        }
        if previous == next {
            output.extend_from_slice(&bytes[member.separator.end..member.value.end]);
        } else if let Some(next) = next {
            output.extend_from_slice(&bytes[member.separator.end..member.value.start]);
            merge_json(
                output,
                &bytes[member.value.clone()],
                previous.unwrap_or(&Value::Null),
                next,
                depth + 1,
            )?;
        }
    }
    let existing: BTreeSet<_> = members.iter().map(|member| member.key.as_str()).collect();
    for (key, next) in after
        .iter()
        .filter(|(key, _)| !existing.contains(key.as_str()))
    {
        if count > 0 {
            output.push(b',');
        }
        count += 1;
        if multiline {
            output.push(b'\n');
            output.extend(std::iter::repeat_n(b' ', (depth + 1) * 2));
        }
        serde_json::to_writer(&mut *output, key)?;
        output.extend_from_slice(if multiline { b": " } else { b":" });
        write_json(output, next, multiline, depth + 1)?;
    }
    output.extend_from_slice(&bytes[members.last().map_or(1, |member| member.value.end)..]);
    Ok(())
}

fn write_json(
    output: &mut Vec<u8>,
    value: &Value,
    multiline: bool,
    depth: usize,
) -> Result<(), StorageError> {
    if multiline {
        let bytes = serde_json::to_vec_pretty(value)?;
        for (index, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
            if index > 0 {
                output.push(b'\n');
                output.extend(std::iter::repeat_n(b' ', depth * 2));
            }
            output.extend_from_slice(line);
        }
    } else {
        serde_json::to_writer(output, value)?;
    }
    Ok(())
}

pub(super) fn write_local(root: &Path, values: &LocalValues) -> Result<(), StorageError> {
    let (bytes, before) = read_document(root)?;
    let mut local = before.clone();
    insert_missing(&mut local.collection, &values.collection);
    for (stem, values) in &values.environments {
        if !values.0.is_empty() {
            insert_missing(local.entries_mut(Some(stem)), &values.0);
        }
    }
    write(root, bytes.as_deref(), &before, &local)
}

fn insert_missing(entries: &mut Entries, incoming: &Entries) {
    for (key, value) in incoming {
        if let std::collections::btree_map::Entry::Vacant(entry) = entries.entry(key.clone()) {
            entry.insert(value.clone());
        }
    }
}

#[derive(Clone, Default, Serialize)]
pub(crate) struct Shared {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) values: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub(crate) secrets: BTreeSet<String>,
}

pub(crate) fn projection(
    connection: &Connection,
    collection_id: &str,
    scope: &str,
) -> Result<Shared, StorageError> {
    let mut shared = Shared::default();
    let mut statement = connection.prepare_cached(
        "SELECT key, shared_value, required_secret FROM repository_variables
         WHERE collection_id = ? AND scope_id = ? AND (shared_value IS NOT NULL OR required_secret)",
    )?;
    let mut rows = statement.query(params![collection_id, scope])?;
    while let Some(row) = rows.next()? {
        let key = row.get(0)?;
        if row.get(2)? {
            shared.secrets.insert(key);
        } else {
            shared.values.insert(key, row.get(1)?);
        }
    }
    Ok(shared)
}

#[derive(Clone, Copy)]
struct EnvironmentScope<'a> {
    id: &'a str,
    stem: &'a str,
}

fn shared_scopes(
    files: &Files,
) -> impl Iterator<
    Item = (
        Option<EnvironmentScope<'_>>,
        &BTreeMap<String, String>,
        &BTreeSet<String>,
    ),
> {
    files.iter().filter_map(|(path, file)| match &file.entity {
        Entity::Manifest(file) => Some((None, &file.values, &file.secrets)),
        Entity::Environment(file) => Some((
            Some(EnvironmentScope {
                id: &file.id,
                stem: environment_stem(path),
            }),
            &file.values,
            &file.secrets,
        )),
        Entity::Request(_) => None,
    })
}

pub(super) fn cache(
    connection: &Connection,
    collection_id: &str,
    files: &Files,
    local: &LocalValues,
) -> Result<bool, StorageError> {
    let mut updated = connection.execute(
        "DELETE FROM repository_variables WHERE collection_id = ?1 AND scope_id != ?1
         AND scope_id NOT IN (SELECT id FROM environments WHERE collection_id = ?1 AND archived = 0)",
        [collection_id],
    )? > 0;
    for (environment, values, secrets) in shared_scopes(files) {
        let scope = environment
            .map(|environment| local_id(collection_id, environment.id))
            .unwrap_or_else(|| collection_id.to_owned());
        updated |= cache_scope(
            connection,
            collection_id,
            &scope,
            values,
            secrets,
            local.entries(environment.map(|environment| environment.stem)),
        )?;
    }
    Ok(updated)
}

fn cache_scope(
    connection: &Connection,
    collection_id: &str,
    scope: &str,
    values: &BTreeMap<String, String>,
    secrets: &BTreeSet<String>,
    local: Option<&Entries>,
) -> Result<bool, StorageError> {
    let local_keys = local.into_iter().flat_map(BTreeMap::keys);
    let keys: BTreeSet<_> = values.keys().chain(secrets).chain(local_keys).collect();
    let mut existing = connection.prepare_cached(
        "SELECT key FROM repository_variables WHERE collection_id = ? AND scope_id = ?",
    )?;
    let old_keys = existing
        .query_map(params![collection_id, scope], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut changed = false;
    let mut remove = connection.prepare_cached(
        "DELETE FROM repository_variables WHERE collection_id = ? AND scope_id = ? AND key = ?",
    )?;
    for key in old_keys.iter().filter(|key| !keys.contains(key)) {
        changed |= remove.execute(params![collection_id, scope, key])? > 0;
    }
    let mut upsert = connection.prepare_cached(
        "INSERT INTO repository_variables
          (collection_id, scope_id, key, value, enabled, sensitive, local, shared_value, required_secret)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(collection_id, scope_id, key) DO UPDATE SET value = excluded.value,
           enabled = excluded.enabled, sensitive = excluded.sensitive, local = excluded.local,
           shared_value = excluded.shared_value, required_secret = excluded.required_secret
         WHERE value IS NOT excluded.value OR enabled IS NOT excluded.enabled
           OR sensitive IS NOT excluded.sensitive OR local IS NOT excluded.local
           OR shared_value IS NOT excluded.shared_value OR required_secret IS NOT excluded.required_secret",
    )?;
    for key in keys {
        let shared = values.get(key).map(String::as_str);
        let required = secrets.contains(key);
        let value = local.and_then(|local| local.get(key));
        let (effective, enabled, sensitive) = match value {
            Some(value) => {
                let (value, enabled, sensitive) = value.fields();
                (Some(value), enabled, sensitive || required)
            }
            None => (shared, shared.is_some(), required),
        };
        changed |= upsert.execute(params![
            collection_id,
            scope,
            key,
            effective,
            enabled,
            sensitive,
            value.is_some(),
            shared,
            required
        ])? > 0;
    }
    Ok(changed)
}

pub(super) fn cached_local(
    connection: &Connection,
    collection_id: &str,
    files: &Files,
) -> Result<LocalValues, StorageError> {
    let mut statement = connection.prepare_cached(
        "SELECT v.scope_id, f.path, v.key, v.value, v.enabled, v.sensitive
         FROM repository_variables v LEFT JOIN repository_files f
           ON f.collection_id = v.collection_id AND f.kind = 'environment' AND f.entity_id = v.scope_id
         WHERE v.collection_id = ? AND v.local",
    )?;
    let mut local = LocalValues::default();
    let mut rows = statement.query([collection_id])?;
    while let Some(row) = rows.next()? {
        let scope: String = row.get(0)?;
        let path: Option<String> = row.get(1)?;
        let stem = if scope == collection_id {
            None
        } else {
            let Some(path) = path.as_deref() else {
                continue;
            };
            if !matches!(
                files.get(path).map(|file| &file.entity),
                Some(Entity::Environment(file)) if file.id == remote_id(&scope)
            ) {
                continue;
            }
            Some(environment_stem(path))
        };
        local.entries_mut(stem).insert(
            row.get(2)?,
            LocalValue::new(row.get(3)?, row.get(4)?, row.get(5)?),
        );
    }
    Ok(local)
}

fn cached_entries(
    connection: &Connection,
    collection_id: &str,
    scope: &str,
) -> Result<Entries, StorageError> {
    let mut statement = connection.prepare_cached(
        "SELECT key, value, enabled, sensitive FROM repository_variables
         WHERE collection_id = ? AND scope_id = ? AND local",
    )?;
    let rows = statement.query_map(params![collection_id, scope], |row| {
        Ok((
            row.get(0)?,
            LocalValue::new(row.get(1)?, row.get(2)?, row.get(3)?),
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub(super) fn write_new_environments<'a>(
    connection: &Connection,
    collection_id: &str,
    root: &Path,
    files: impl Iterator<Item = (&'a str, &'a File)>,
) -> Result<(), StorageError> {
    let mut values = LocalValues::default();
    for (path, file) in files {
        let scope = local_id(collection_id, file.entity.id());
        let entries = cached_entries(connection, collection_id, &scope)?;
        if !entries.is_empty() {
            values
                .environments
                .insert(environment_stem(path).into(), EnvironmentEntries(entries));
        }
    }
    if values.environments.is_empty() {
        return Ok(());
    }
    let (bytes, before) = read_document(root)?;
    let mut after = before.clone();
    for (stem, entries) in values.environments {
        if after.environments.insert(stem, entries).is_some() {
            return Err(input(
                "local.json changed while creating the environment. Retry the operation.",
            ));
        }
    }
    write(root, bytes.as_deref(), &before, &after)
}

pub(crate) struct Target {
    collection_id: String,
    scope_id: String,
    root: PathBuf,
    manifest_id: String,
}

pub(crate) fn target(
    connection: &Connection,
    target: &VariableTarget,
) -> Result<Option<Target>, StorageError> {
    let (_, collection_id, environment_id) = target.columns();
    let scope = collection_id.or(environment_id);
    let Some(scope) = scope else { return Ok(None) };
    Ok(connection
        .prepare_cached(
            "SELECT root, collection_id, manifest_id FROM repository_links
             WHERE collection_id = COALESCE(?1, (SELECT collection_id FROM environments WHERE id = ?2))",
        )?
        .query_row(params![collection_id, environment_id], |row| {
            Ok(Target {
                root: PathBuf::from(row.get::<_, String>(0)?),
                collection_id: row.get(1)?,
                scope_id: scope.into(),
                manifest_id: row.get(2)?,
            })
        })
        .optional()?)
}

pub(crate) fn list(
    connection: &Connection,
    target: &Target,
) -> Result<Vec<VariableEntry>, StorageError> {
    let mut statement = connection.prepare_cached(
        "SELECT key, value, enabled, sensitive, local, shared_value IS NOT NULL, required_secret
         FROM repository_variables WHERE collection_id = ?1 AND scope_id = ?2
           AND NOT EXISTS (SELECT 1 FROM environments WHERE id = ?2 AND archived)
         ORDER BY key",
    )?;
    let rows = statement.query_map(params![target.collection_id, target.scope_id], |row| {
        let value = row.get::<_, Option<String>>(1)?.unwrap_or_default();
        let enabled: bool = row.get(2)?;
        let required = row.get(6)?;
        let origin = if required && (!enabled || value.is_empty()) {
            VariableOrigin::MissingSecret
        } else if row.get(4)? {
            VariableOrigin::Local
        } else {
            VariableOrigin::Shared
        };
        Ok(VariableEntry {
            key: row.get(0)?,
            value,
            enabled,
            sensitive: row.get(3)?,
            origin: Some(origin),
            has_shared_value: Some(row.get(5)?),
            required_secret: Some(required),
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

impl Target {
    fn environment_key<'a>(
        &self,
        paths: &'a BTreeMap<String, String>,
    ) -> Result<Option<&'a str>, StorageError> {
        if self.scope_id == self.collection_id {
            return Ok(None);
        }
        let (path, _, _) = self.read_environment(paths)?;
        Ok(Some(environment_stem(path)))
    }

    fn read_environment<'a>(
        &self,
        paths: &'a BTreeMap<String, String>,
    ) -> Result<(&'a str, Vec<u8>, format::Environment), StorageError> {
        let stale = || {
            input("The environment file changed. Refresh the repository before editing its variables.")
        };
        let path = paths.get(&self.scope_id).ok_or_else(stale)?;
        let bytes = format::read(&self.root, path)?.ok_or_else(stale)?;
        let environment =
            format::parse_environment(&bytes).map_err(|error| format::at_path(path, error))?;
        if environment.id != remote_id(&self.scope_id) {
            return Err(stale());
        }
        Ok((path, bytes, environment))
    }
}

fn shared_file(
    connection: &Connection,
    target: &Target,
) -> Result<(String, Vec<u8>, Shared), StorageError> {
    let manifest = read_manifest(&target.root, Some(&target.manifest_id))?;
    if target.scope_id == target.collection_id {
        let Entity::Manifest(entity) = manifest.entity else {
            unreachable!("read_manifest returns a manifest");
        };
        return Ok((
            MANIFEST.into(),
            manifest.contents,
            Shared {
                values: entity.values,
                secrets: entity.secrets,
            },
        ));
    }
    let paths = environment_paths(connection, &target.collection_id)?;
    let (path, bytes, environment) = target.read_environment(&paths)?;
    Ok((
        path.into(),
        bytes,
        Shared {
            values: environment.values,
            secrets: environment.secrets,
        },
    ))
}

pub(crate) fn read_shared(
    connection: &Connection,
    target: &Target,
    key: &str,
) -> Result<String, StorageError> {
    let (_, _, mut shared) = shared_file(connection, target)?;
    shared
        .values
        .remove(key)
        .ok_or_else(|| input(format!("No shared value for {key:?}.")))
}

pub(crate) fn edit_shared(
    connection: &Connection,
    target: &Target,
    key: &str,
    change: SharedVariableChange,
) -> Result<(), StorageError> {
    let key = key.trim();
    if !format::valid_variable_key(key) {
        return Err(input("Enter a variable name."));
    }
    let _lock = lock_root(&target.root)?;
    let (path, bytes, before) = shared_file(connection, target)?;
    let mut after = before.clone();
    match change {
        SharedVariableChange::Value { previous, value } => {
            if before.secrets.contains(key) {
                return Err(input(format!(
                    "The required secret {key:?} can only have a local value."
                )));
            }
            if before.values.get(key) != Some(&previous) {
                return Err(input(format!("The shared value for {key:?} changed on disk. Reopen the editor and try again.")));
            }
            if previous == value {
                return Ok(());
            }
            after.values.insert(key.into(), value);
        }
        SharedVariableChange::RequiredSecret => {
            if before.values.contains_key(key) {
                return Err(input(format!(
                    "The key {key:?} already has a shared value. Choose a different name."
                )));
            }
            if !after.secrets.insert(key.into()) {
                return Ok(());
            }
        }
    }
    let next = update_json(&bytes, &before, &after)?;
    format::replace(&target.root, &path, Some(&bytes), Some(&next))?;
    format::sync_directories(&target.root, [path.as_str()])
}

fn environment_paths(
    connection: &Connection,
    collection_id: &str,
) -> Result<BTreeMap<String, String>, StorageError> {
    let mut statement = connection.prepare_cached(
        "SELECT entity_id, path FROM repository_files WHERE collection_id = ? AND kind = 'environment'",
    )?;
    let rows = statement.query_map([collection_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn update<'a>(
    connection: &Connection,
    target: &Target,
    targets: impl Iterator<Item = &'a Target>,
    operation: impl FnOnce(&mut LocalValues, &BTreeMap<&str, Option<&str>>) -> Result<(), StorageError>,
) -> Result<(), StorageError> {
    let _lock = lock_root(&target.root)?;
    read_manifest(&target.root, Some(&target.manifest_id))?;
    let (bytes, before) = read_document(&target.root)?;
    let mut local = before.clone();
    let paths = environment_paths(connection, &target.collection_id)?;
    let mut stems = BTreeMap::new();
    for target in targets {
        if let std::collections::btree_map::Entry::Vacant(entry) =
            stems.entry(target.scope_id.as_str())
        {
            entry.insert(target.environment_key(&paths)?);
        }
    }
    operation(&mut local, &stems)?;
    local
        .environments
        .retain(|stem, entries| !entries.0.is_empty() || before.environments.contains_key(stem));
    write(&target.root, bytes.as_deref(), &before, &local)?;
    // Leave the watch stamp stale so the next refresh still notices a
    // concurrent shared edit or unrelated changes read from local.json.
    for (scope, stem) in std::iter::once((target.collection_id.as_str(), None)).chain(
        paths
            .iter()
            .map(|(scope, path)| (scope.as_str(), Some(environment_stem(path)))),
    ) {
        let shared = projection(connection, &target.collection_id, scope)?;
        cache_scope(
            connection,
            &target.collection_id,
            scope,
            &shared.values,
            &shared.secrets,
            local.entries(stem),
        )?;
    }
    Ok(())
}

pub(crate) fn edit(
    connection: &Connection,
    changes: &[(Target, &VariableChange)],
) -> Result<(), StorageError> {
    let mut by_root: BTreeMap<&Path, Vec<_>> = BTreeMap::new();
    for (target, change) in changes {
        by_root
            .entry(&target.root)
            .or_default()
            .push((target, *change));
    }
    // A move across roots has two destination files. Apply additions before
    // removing any sources. A normal editor batch uses one atomic replacement.
    if by_root.len() > 1 && changes.iter().any(|(_, change)| !change.deletes.is_empty()) {
        for changes in by_root.values() {
            if changes.iter().any(|(_, change)| !change.upserts.is_empty()) {
                edit_file(connection, changes, false)?;
            }
        }
    }
    for changes in by_root.values() {
        edit_file(connection, changes, true)?;
    }
    Ok(())
}

fn edit_file(
    connection: &Connection,
    changes: &[(&Target, &VariableChange)],
    apply_deletes: bool,
) -> Result<(), StorageError> {
    let targets = changes.iter().map(|(target, _)| *target);
    update(connection, changes[0].0, targets, |local, stems| {
        for (target, change) in changes {
            let required = projection(connection, &target.collection_id, &target.scope_id)?.secrets;
            let baseline: BTreeMap<_, _> = change
                .baseline
                .iter()
                .flatten()
                .map(|row| (row.key.as_str(), row))
                .collect();
            let entries = local.entries_mut(stems[target.scope_id.as_str()]);
            if apply_deletes {
                for key in &change.deletes {
                    entries.remove(key);
                }
            }
            for variable in &change.upserts {
                let key = variable.key.trim();
                if key.is_empty() {
                    continue;
                }
                let (mut value, mut enabled, mut sensitive) = (
                    variable.value.as_str(),
                    variable.enabled,
                    variable.sensitive,
                );
                if let Some(prior) = baseline.get(key) {
                    if let Some(latest) = entries.get(key) {
                        let (latest_value, latest_enabled, latest_sensitive) = latest.fields();
                        if value == prior.value {
                            value = latest_value;
                        }
                        if enabled == prior.enabled {
                            enabled = latest_enabled;
                        }
                        if sensitive == prior.sensitive {
                            sensitive = latest_sensitive;
                        }
                    } else if prior.origin == Some(VariableOrigin::Local)
                        && value == prior.value
                        && !change.deletes.iter().any(|removed| removed == key)
                    {
                        return Err(input(format!(
                            "Local variable {key:?} was removed on disk. Refresh the repository before editing it."
                        )));
                    }
                }
                entries.insert(
                    key.into(),
                    LocalValue::new(
                        value.to_owned(),
                        enabled,
                        sensitive || required.contains(key),
                    ),
                );
            }
            validate_entries(entries)?;
        }
        Ok(())
    })
}

pub(crate) fn set_script_values(
    connection: &Connection,
    writes: &[(Target, &str, &str)],
) -> Result<(), StorageError> {
    let Some((collection, _, _)) = writes.first() else {
        return Ok(());
    };
    let targets = writes.iter().map(|(target, _, _)| target);
    update(connection, collection, targets, |local, stems| {
        for (target, key, value) in writes {
            let required = projection(connection, &target.collection_id, &target.scope_id)?.secrets;
            let entries = local.entries_mut(stems[target.scope_id.as_str()]);
            let sensitive =
                required.contains(*key) || entries.get(*key).is_some_and(|prior| prior.fields().2);
            entries.insert(
                (*key).into(),
                LocalValue::new((*value).into(), true, sensitive),
            );
            validate_entries(entries)?;
        }
        Ok(())
    })
}

pub(crate) fn stage_environment_copy(
    connection: &Connection,
    source: &Target,
    destination: &str,
) -> Result<(), StorageError> {
    let paths = environment_paths(connection, &source.collection_id)?;
    let stem = source.environment_key(&paths)?;
    let local = read(&source.root)?;
    let shared = projection(connection, &source.collection_id, &source.scope_id)?;
    cache_scope(
        connection,
        &source.collection_id,
        destination,
        &shared.values,
        &shared.secrets,
        local.entries(stem),
    )?;
    Ok(())
}

pub(crate) fn stage_environment_import(
    connection: &Connection,
    target: &Target,
    variables: impl Iterator<Item = VariableEntry>,
) -> Result<(), StorageError> {
    let mut entries = Entries::new();
    for variable in variables {
        entries.insert(
            variable.key,
            LocalValue::new(variable.value, variable.enabled, variable.sensitive),
        );
    }
    validate_entries(&entries)?;
    let shared = Shared::default();
    cache_scope(
        connection,
        &target.collection_id,
        &target.scope_id,
        &shared.values,
        &shared.secrets,
        Some(&entries),
    )?;
    Ok(())
}

fn credential_keys(files: &Files) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for file in files.values() {
        match &file.entity {
            Entity::Manifest(manifest) => {
                keys.extend(
                    manifest
                        .auth
                        .iter()
                        .chain(
                            manifest
                                .folders
                                .iter()
                                .filter_map(|folder| folder.auth.as_ref()),
                        )
                        .flat_map(auth_keys),
                );
            }
            Entity::Request(request) => {
                keys.extend(request.auth.iter().flat_map(auth_keys));
                keys.extend(
                    request
                        .headers
                        .iter()
                        .filter_map(format::header_credential)
                        .filter_map(format::variable_reference),
                );
            }
            Entity::Environment(_) => {}
        }
    }
    keys.into_iter().map(str::to_owned).collect()
}

fn auth_keys(auth: &crate::commands::models::AuthConfig) -> impl Iterator<Item = &str> {
    format::auth_credentials(auth).filter_map(|(_, value)| format::variable_reference(value))
}

fn sensitive_key(key: &str) -> bool {
    let key: String = key
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    [
        "token",
        "password",
        "secret",
        "apikey",
        "privatekey",
        "credential",
        "authorization",
        "cookie",
        "connectionstring",
    ]
    .iter()
    .any(|suffix| key.ends_with(suffix))
}

fn source_values(
    connection: &Connection,
    source: &str,
    environment: Option<&str>,
) -> Result<Entries, StorageError> {
    let mut statement = connection.prepare_cached(
        "SELECT v.key, v.current_value, v.enabled, v.sensitive FROM variables v
         LEFT JOIN environments e ON v.environment_id = e.id
         WHERE (v.scope = 'collection' AND ?2 IS NULL AND v.collection_id = ?1)
            OR (v.scope = 'environment' AND e.collection_id = ?1 AND (e.id = ?2
                 OR (length(e.id) = 32 AND e.id = replace(?2, '-', ''))))",
    )?;
    let rows = statement.query_map(params![source, environment], |row| {
        Ok((
            row.get(0)?,
            LocalValue::new(row.get(1)?, row.get(2)?, row.get(3)?),
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn share(
    source: Entries,
    credentials: &BTreeSet<String>,
    values: &mut BTreeMap<String, String>,
    secrets: &mut BTreeSet<String>,
) {
    for (key, value) in source {
        let (value, enabled, sensitive) = value.fields();
        if sensitive || credentials.contains(&key) || sensitive_key(&key) {
            secrets.insert(key);
        } else if enabled {
            values.insert(key, value.into());
        }
    }
}

pub(super) fn share_source(
    connection: &Connection,
    source: &str,
    files: &mut Files,
) -> Result<(), StorageError> {
    let credentials = credential_keys(files);
    for file in files.values_mut() {
        match &mut file.entity {
            Entity::Manifest(manifest) => {
                share(
                    source_values(connection, source, None)?,
                    &credentials,
                    &mut manifest.values,
                    &mut manifest.secrets,
                );
                format::validate_manifest(manifest)?;
            }
            Entity::Environment(environment) => {
                share(
                    source_values(connection, source, Some(&environment.id))?,
                    &credentials,
                    &mut environment.values,
                    &mut environment.secrets,
                );
                format::validate_environment(environment)?;
            }
            Entity::Request(_) => continue,
        }
        file.contents = format::encode(&file.entity)?;
    }
    Ok(())
}

pub(super) fn publish_local(
    connection: &Connection,
    source: &str,
    files: &Files,
) -> Result<LocalValues, StorageError> {
    let mut local = LocalValues::default();
    for (environment, values, secrets) in shared_scopes(files) {
        let entries = local.entries_mut(environment.map(|environment| environment.stem));
        for (key, value) in source_values(
            connection,
            source,
            environment.map(|environment| environment.id),
        )? {
            let (current, enabled, sensitive) = value.fields();
            if !enabled || sensitive || values.get(&key).is_none_or(|shared| shared != current) {
                let required = secrets.contains(&key);
                entries.insert(
                    key,
                    LocalValue::new(current.into(), enabled, sensitive || required),
                );
            }
        }
        validate_entries(entries)?;
    }
    Ok(local)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_git_metadata_inside_the_worktree_is_not_used_for_private_staging() {
        let root = super::super::tests::Scratch::new();
        let metadata = root.0.join("tracked-metadata");
        assert!(git(
            &root.0,
            &[
                "init",
                "--quiet",
                "--separate-git-dir",
                metadata.to_str().unwrap()
            ]
        )
        .unwrap()
        .status
        .success());

        let candidates = staging_directories(&root.0).unwrap();
        assert!(!candidates.is_empty());
        assert!(candidates.iter().all(|path| !path.starts_with(&root.0)));
    }

    #[test]
    fn git_ignores_private_values_and_refuses_an_explicit_unignore() {
        let root =
            std::env::temp_dir().join(format!("conductor-git-ignore-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join(".conductor")).unwrap();
        let result = (|| {
            assert!(git(&root, &["init", "--quiet"]).unwrap().status.success());
            ensure_ignore(&root)?;
            let local_path = root.join(".conductor/local.json");
            let ignore_path = root.join(".conductor/.gitignore");
            let before = br#"{"collection":{"token":"fixture-token"}}"#;
            format::replace(&root, LOCAL, None, Some(before))?;
            let status = git(
                &root,
                &["status", "--short", "--ignored", "--untracked-files=all"],
            )
            .unwrap();
            assert!(status.status.success());
            assert!(String::from_utf8_lossy(&status.stdout).contains("!! .conductor/local.json"));

            let mut rules = fs::read(&ignore_path).unwrap();
            rules.extend_from_slice(b"!/local.json\n");
            fs::write(&ignore_path, rules).unwrap();
            let error = format::replace(&root, LOCAL, Some(before), Some(b"{}")).unwrap_err();
            assert!(error.to_string().contains("ignored by Git and untracked"));
            assert_eq!(fs::read(local_path).unwrap(), before);
            Ok::<_, StorageError>(())
        })();
        fs::remove_dir_all(root).unwrap();
        result.unwrap();
    }
}
