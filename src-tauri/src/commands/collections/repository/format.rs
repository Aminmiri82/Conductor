use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    commands::models::{AuthConfig, KeyValue, RequestBody},
    storage::StorageError,
};

const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FOLDERS: usize = 10_000;
const MAX_FOLDER_DEPTH: usize = 64;
const MAX_POSITION: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Manifest {
    pub(super) conductor_format: u32,
    pub(super) id: String,
    pub(super) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) auth: Option<AuthConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) folders: Vec<Folder>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Folder {
    pub(super) id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) parent_id: Option<String>,
    pub(super) name: String,
    #[serde(default)]
    pub(super) position: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) auth: Option<AuthConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    pub(super) id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) parent_id: Option<String>,
    pub(super) name: String,
    #[serde(default)]
    pub(super) position: i64,
    pub(super) method: String,
    pub(super) url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) headers: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) query: Vec<KeyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) path_params: Vec<KeyValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) auth: Option<AuthConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) body: Option<RequestBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) pre_request_script: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) test_script: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Environment {
    pub(super) id: String,
    pub(super) name: String,
    #[serde(default)]
    pub(super) defaults: BTreeMap<String, String>,
}

pub(super) fn parse_manifest(bytes: &[u8]) -> Result<Manifest, StorageError> {
    let manifest = serde_json::from_slice(bytes)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub(super) fn parse_request(bytes: &[u8]) -> Result<Request, StorageError> {
    let mut request = serde_json::from_slice(bytes)?;
    validate_request(&mut request)?;
    Ok(request)
}

pub(super) fn parse_environment(bytes: &[u8]) -> Result<Environment, StorageError> {
    let environment = serde_json::from_slice(bytes)?;
    validate_environment(&environment)?;
    Ok(environment)
}

pub(super) fn validate_manifest(manifest: &Manifest) -> Result<(), StorageError> {
    if manifest.conductor_format != 1 {
        return Err(invalid(format!(
            "unsupported conductorFormat {}",
            manifest.conductor_format
        )));
    }
    validate_identity(&manifest.id, &manifest.name)?;
    validate_auth(manifest.auth.as_ref())?;
    if manifest.folders.len() > MAX_FOLDERS {
        return Err(invalid("collection has too many folders"));
    }

    let mut parents = HashMap::with_capacity(manifest.folders.len());
    for folder in &manifest.folders {
        validate_identity(&folder.id, &folder.name)?;
        validate_position(folder.position)?;
        validate_auth(folder.auth.as_ref())?;
        if parents
            .insert(folder.id.as_str(), folder.parent_id.as_deref())
            .is_some()
        {
            return Err(invalid(format!("duplicate folder ID {}", folder.id)));
        }
    }
    for folder in &manifest.folders {
        let mut parent = folder.parent_id.as_deref();
        for _ in 1..MAX_FOLDER_DEPTH {
            let Some(id) = parent else {
                break;
            };
            parent = *parents
                .get(id)
                .ok_or_else(|| invalid(format!("folder {} has no parent {id}", folder.id)))?;
        }
        if parent.is_some() {
            return Err(invalid(format!(
                "folder {} has a parent cycle or exceeds {MAX_FOLDER_DEPTH} levels",
                folder.id
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_request(request: &mut Request) -> Result<(), StorageError> {
    validate_identity(&request.id, &request.name)?;
    validate_position(request.position)?;
    if let Some(parent_id) = &request.parent_id {
        validate_id(parent_id)?;
    }
    reqwest::Method::from_bytes(request.method.as_bytes())
        .map_err(|_| invalid("request method must be an HTTP token"))?;
    validate_auth(request.auth.as_ref())?;
    for header in &request.headers {
        let value = header.value.trim();
        let key = header.key.trim();
        let reference = if key.eq_ignore_ascii_case("Authorization")
            || key.eq_ignore_ascii_case("Proxy-Authorization")
        {
            value
                .split_once(char::is_whitespace)
                .filter(|(scheme, _)| {
                    scheme.eq_ignore_ascii_case("Bearer") || scheme.eq_ignore_ascii_case("Basic")
                })
                .map_or(value, |(_, credential)| credential.trim())
        } else if ["X-API-Key", "API-Key", "X-Auth-Token"]
            .iter()
            .any(|name| name.eq_ignore_ascii_case(key))
        {
            value
        } else {
            continue;
        };
        if !reference.is_empty() && !is_variable_reference(reference) {
            return Err(invalid(format!("{key} must use a variable reference")));
        }
    }
    if let Some(body) = &mut request.body {
        for field in &mut body.form_data {
            field.file_path = None;
        }
        if let Some(file) = &mut body.file {
            file.path = None;
        }
    }
    for script in [
        request.pre_request_script.as_mut(),
        request.test_script.as_mut(),
    ]
    .into_iter()
    .flatten()
    {
        script.sort_all_objects();
    }
    Ok(())
}

pub(super) fn validate_environment(environment: &Environment) -> Result<(), StorageError> {
    validate_identity(&environment.id, &environment.name)?;
    if environment
        .defaults
        .keys()
        .any(|key| key.is_empty() || key != key.trim() || key.chars().any(char::is_control))
    {
        return Err(invalid("environment keys must be nonempty and trimmed"));
    }
    Ok(())
}

fn validate_identity(id: &str, name: &str) -> Result<(), StorageError> {
    validate_id(id)?;
    if name.trim().is_empty() {
        return Err(invalid("name cannot be blank"));
    }
    Ok(())
}

pub(super) fn validate_id(id: &str) -> Result<(), StorageError> {
    if !Uuid::parse_str(id).is_ok_and(|uuid| {
        let mut buffer = Uuid::encode_buffer();
        uuid.hyphenated().encode_lower(&mut buffer) == id
    }) {
        return Err(invalid("IDs must be lowercase hyphenated UUIDs"));
    }
    Ok(())
}

fn validate_position(position: i64) -> Result<(), StorageError> {
    if !(0..=MAX_POSITION).contains(&position) {
        return Err(invalid("position must be a nonnegative safe integer"));
    }
    Ok(())
}

fn validate_auth(auth: Option<&AuthConfig>) -> Result<(), StorageError> {
    let Some(auth) = auth else { return Ok(()) };
    for (field, value) in [
        ("token", &auth.token),
        ("username", &auth.username),
        ("password", &auth.password),
        ("value", &auth.value),
    ] {
        let value = value.as_deref().unwrap_or_default().trim();
        if !value.is_empty() && !is_variable_reference(value) {
            return Err(invalid(format!(
                "auth {field} must use a variable reference"
            )));
        }
    }
    Ok(())
}

fn is_variable_reference(value: &str) -> bool {
    value
        .strip_prefix("{{")
        .and_then(|value| value.strip_suffix("}}"))
        .is_some_and(|key| {
            !key.trim().is_empty()
                && !key.contains(['{', '}'])
                && !key.chars().any(char::is_control)
        })
}

pub(super) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, StorageError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(super) fn file_name(subdir: &str, name: &str, id: &str) -> String {
    let mut slug = String::new();
    for word in name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
    {
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.extend(
            word.bytes()
                .take(48 - slug.len())
                .map(|byte| char::from(byte.to_ascii_lowercase())),
        );
        if slug.len() == 48 {
            break;
        }
    }
    let slug = slug.trim_end_matches('-');
    let slug = if slug.is_empty() { "item" } else { slug };
    format!("{subdir}/{slug}-{id}.json")
}

pub(super) fn list(root: &Path, subdir: &str) -> Result<Vec<String>, StorageError> {
    let Some(path) = directory(root, subdir, false)? else {
        return Ok(Vec::new());
    };
    let mut paths = Vec::new();
    for entry in fs::read_dir(&path).map_err(|error| io_error(&path, error))? {
        let entry = entry.map_err(|error| io_error(&path, error))?;
        if let Some(name) = entry.file_name().to_str() {
            if name.ends_with(".json") {
                paths.push(format!("{subdir}/{name}"));
                if paths.len() > MAX_FOLDERS + 1 {
                    return Err(invalid(format!("{subdir} has too many JSON files")));
                }
            }
        }
    }
    paths.sort_unstable();
    Ok(paths)
}

pub(super) fn read(root: &Path, relative: &str) -> Result<Option<Vec<u8>>, StorageError> {
    let Some(path) = file_path(root, relative, false)? else {
        return Ok(None);
    };
    let Some(metadata) = metadata(&path)? else {
        return Ok(None);
    };
    if !metadata.is_file() || is_link(&metadata) {
        return Err(invalid(format!(
            "{} must be a regular file",
            path.display()
        )));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(invalid(format!("{} is larger than 16 MiB", path.display())));
    }
    let file = File::open(&path).map_err(|error| io_error(&path, error))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error(&path, error))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(invalid(format!("{} is larger than 16 MiB", path.display())));
    }
    Ok(Some(bytes))
}

pub(super) fn replace(
    root: &Path,
    relative: &str,
    expected: Option<&[u8]>,
    next: Option<&[u8]>,
) -> Result<(), StorageError> {
    let check = || {
        if read(root, relative)?.as_deref() != expected {
            return Err(invalid(format!(
                "{relative} changed on disk. Refresh and try again."
            )));
        }
        Ok(())
    };
    check()?;
    if next == expected {
        return Ok(());
    }
    let path = file_path(root, relative, next.is_some())?
        .ok_or_else(|| invalid(format!("the directory for {relative} is missing")))?;
    let Some(bytes) = next else {
        return fs::remove_file(&path).map_err(|error| io_error(&path, error));
    };
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(invalid(format!("{relative} is larger than 16 MiB")));
    }

    let temporary = path.with_file_name(format!(".conductor-{}.tmp", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| io_error(&temporary, error))?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    let result = written
        .map_err(|error| io_error(&temporary, error))
        .and_then(|()| check())
        .and_then(|()| {
            // A hard link installs a new file only if the name is still free.
            // Both entries live in the same directory and contain the same bytes.
            if expected.is_none() {
                fs::hard_link(&temporary, &path)
            } else {
                fs::rename(&temporary, &path)
            }
            .map_err(|error| io_error(&path, error))
        });
    let _ = fs::remove_file(temporary);
    result
}

fn file_path(root: &Path, relative: &str, create: bool) -> Result<Option<PathBuf>, StorageError> {
    if relative == "collection.json" {
        return Ok(real_directory(root.join(".conductor"), false)?.map(|path| path.join(relative)));
    }
    let (subdir, name) = relative
        .split_once('/')
        .ok_or_else(|| invalid("invalid repository file path"))?;
    if !name.ends_with(".json")
        || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        || name.chars().any(char::is_control)
        || reserved_device_name(name)
    {
        return Err(invalid("invalid repository filename"));
    }
    Ok(directory(root, subdir, create)?.map(|path| path.join(name)))
}

fn reserved_device_name(name: &str) -> bool {
    let stem = name
        .split_once('.')
        .map_or(name, |(stem, _)| stem)
        .trim_end()
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "AUX" | "CON" | "CONIN$" | "CONOUT$" | "NUL" | "PRN"
    ) || stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"))
        .is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
}

fn directory(root: &Path, subdir: &str, create: bool) -> Result<Option<PathBuf>, StorageError> {
    if !matches!(subdir, "requests" | "environments") {
        return Err(invalid("invalid repository subdirectory"));
    }
    let Some(root) = real_directory(root.join(".conductor"), false)? else {
        return Ok(None);
    };
    real_directory(root.join(subdir), create)
}

fn real_directory(path: PathBuf, create: bool) -> Result<Option<PathBuf>, StorageError> {
    let mut current = metadata(&path)?;
    if current.is_none() && create {
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(io_error(&path, error)),
        }
        current = metadata(&path)?;
    }
    match current {
        Some(metadata) if metadata.is_dir() && !is_link(&metadata) => Ok(Some(path)),
        Some(_) => Err(invalid(format!(
            "{} must be a real directory",
            path.display()
        ))),
        None => Ok(None),
    }
}

fn metadata(path: &Path) -> Result<Option<Metadata>, StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(path, error)),
    }
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Directory junctions and other reparse points can redirect I/O too.
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn io_error(path: &Path, error: io::Error) -> StorageError {
    invalid(format!("{}: {error}", path.display()))
}

fn invalid(message: impl Into<String>) -> StorageError {
    StorageError::InvalidInput(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const REQUEST_ID: &str = "cc09ff96-52a8-411a-9c1b-188801196f11";
    const FOLDER_ID: &str = "84c0986c-7aea-4208-aca0-2183f58c4b5e";

    fn request() -> Request {
        let input = format!(
            r#"{{"id":"{REQUEST_ID}","name":"Health check","position":1024,
                "method":"GET","url":"{{{{baseUrl}}}}/health"}}"#
        );
        parse_request(input.as_bytes()).unwrap()
    }

    struct Repository(PathBuf);

    impl Repository {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("conductor-format-{}", Uuid::new_v4()));
            fs::create_dir_all(root.join(".conductor")).unwrap();
            Self(root)
        }
    }

    impl Drop for Repository {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn hand_edited_request_round_trips_after_its_first_canonical_save() {
        let request = request();
        let bytes = encode(&request).unwrap();

        assert_eq!(request.name, "Health check");
        assert_eq!(request.url, "{{baseUrl}}/health");
        assert!(bytes.ends_with(b"\n"));
        assert_eq!(encode(&parse_request(&bytes).unwrap()).unwrap(), bytes);
    }

    #[test]
    fn environment_defaults_are_sorted_for_stable_diffs() {
        let input = format!(
            r#"{{"name":"Development","id":"{REQUEST_ID}","defaults":{{"z":"last","a":"first"}}}}"#
        );
        let environment = parse_environment(input.as_bytes()).unwrap();
        let bytes = encode(&environment).unwrap();
        let json = std::str::from_utf8(&bytes).unwrap();

        assert!(json.find("\"a\"").unwrap() < json.find("\"z\"").unwrap());
        assert_eq!(encode(&parse_environment(&bytes).unwrap()).unwrap(), bytes);
    }

    #[test]
    fn manifest_rejects_bad_folder_graphs_and_future_versions() {
        let input = format!(
            r#"{{"conductorFormat":1,"id":"{REQUEST_ID}","name":"API","folders":[
                {{"id":"{FOLDER_ID}","name":"Nested","parentId":"{FOLDER_ID}"}}]}}"#
        );
        let error = parse_manifest(input.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("cycle"));
        let input = input.replace("conductorFormat\":1", "conductorFormat\":2");
        let error = parse_manifest(input.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("unsupported conductorFormat 2"));
    }

    #[test]
    fn bad_request_does_not_prevent_reading_valid_siblings() {
        let Repository(root) = &Repository::new();
        let good = "requests/valid.json";
        let bad = "requests/invalid.json";
        replace(root, good, None, Some(&encode(&request()).unwrap())).unwrap();
        replace(root, bad, None, Some(b"{ invalid")).unwrap();

        assert_eq!(list(root, "requests").unwrap(), [bad, good]);
        assert!(parse_request(&read(root, bad).unwrap().unwrap()).is_err());
        assert!(parse_request(&read(root, good).unwrap().unwrap()).is_ok());
    }

    #[test]
    fn literal_credentials_cannot_be_encoded_as_a_shared_request() {
        let mut request = request();
        request.auth =
            Some(serde_json::from_str(r#"{"authType":"bearer","token":"literal-token"}"#).unwrap());
        assert!(validate_request(&mut request).is_err());
        request.auth.as_mut().unwrap().token = Some("{{token}}".into());
        request.headers.push(KeyValue {
            key: "Authorization".into(),
            value: "Bearer literal-token".into(),
            enabled: true,
        });
        assert!(validate_request(&mut request).is_err());
        request.headers[0].value = "Bearer {{token}}".into();
        validate_request(&mut request).unwrap();
    }

    #[test]
    fn strips_upload_paths_even_from_inconsistent_hand_edits() {
        let mut request = request();
        request.body = Some(
            serde_json::from_str(
                r#"{"mode":"formdata","formData":[{"key":"file","value":"","filePath":"/private/form.txt"}],
                    "file":{"path":"/private/body.txt"}}"#,
            )
            .unwrap(),
        );
        validate_request(&mut request).unwrap();
        let json = String::from_utf8(encode(&request).unwrap()).unwrap();

        assert!(!json.contains("/private/"));
    }

    #[test]
    fn request_name_cannot_choose_a_path_outside_its_directory() {
        let Repository(root) = &Repository::new();
        let relative = file_name("requests", "../../CON | Health ☀", REQUEST_ID);

        assert_eq!(relative, format!("requests/con-health-{REQUEST_ID}.json"));
        replace(root, &relative, None, Some(b"new")).unwrap();
        for path in [
            "../escape.json",
            "requests/../escape.json",
            "requests/..\\escape.json",
            "requests/wanted.json:alternate.json",
            "requests/aux.backup.json",
            "requests/Com3.json",
        ] {
            assert!(replace(root, path, None, Some(b"unsafe")).is_err());
        }
        assert_eq!(
            read(root, &relative).unwrap().as_deref(),
            Some(b"new".as_slice())
        );
    }

    #[test]
    fn stale_replacements_and_deletions_keep_the_external_edit() {
        let Repository(root) = &Repository::new();
        let path = "requests/request.json";
        replace(root, path, None, Some(b"old")).unwrap();
        fs::write(root.join(".conductor").join(path), b"changed").unwrap();

        assert!(replace(root, path, Some(b"old"), Some(b"app edit")).is_err());
        assert!(replace(root, path, None, Some(b"new request")).is_err());
        assert!(replace(root, path, Some(b"old"), None).is_err());
        assert_eq!(
            read(root, path).unwrap().as_deref(),
            Some(b"changed".as_slice())
        );
        replace(root, path, Some(b"changed"), Some(b"app edit")).unwrap();
        replace(root, path, Some(b"app edit"), None).unwrap();
        assert!(read(root, path).unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn refusing_symlink_files_and_directories_keeps_external_files_intact() {
        use std::os::unix::fs::symlink;

        let Repository(root) = &Repository::new();
        let external = root.join("outside");
        fs::create_dir(&external).unwrap();
        fs::write(external.join("target.json"), b"keep").unwrap();

        let requests = root.join(".conductor/requests");
        symlink(&external, &requests).unwrap();
        assert!(list(root, "requests").is_err());
        assert!(replace(root, "requests/target.json", None, Some(b"unsafe")).is_err());
        fs::remove_file(&requests).unwrap();
        fs::create_dir(&requests).unwrap();
        symlink(external.join("target.json"), requests.join("linked.json")).unwrap();
        assert_eq!(list(root, "requests").unwrap(), ["requests/linked.json"]);
        assert!(read(root, "requests/linked.json").is_err());
        assert!(replace(root, "requests/linked.json", None, Some(b"unsafe")).is_err());
        assert_eq!(fs::read(external.join("target.json")).unwrap(), b"keep");
    }
}
