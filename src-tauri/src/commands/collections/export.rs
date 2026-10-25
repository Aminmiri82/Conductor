//! Postman collection v2.1 export, the reverse of `import.rs`. Each collection
//! is one `.postman_collection.json` file holding its folders, requests,
//! auth, scripts and collection variables. Postman keeps environments in files
//! of their own, so each environment can be written beside it as a
//! `.postman_environment.json`. Values are written as stored, secrets included.

use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::Value;
use tauri::State;
use uuid::Uuid;

use super::postman_file_dialog;
use crate::commands::{
    error::CommandError,
    models::{
        ApiKeyLocation, AuthConfig, AuthType, BodyField, BodyFieldType, BodyMode, KeyValue,
        RequestBody,
    },
};
use crate::storage::StorageError;
use crate::AppState;

const SCHEMA: &str = "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

/// Asks where to save the collection, offering "<collection>.postman_collection.json",
/// and writes it there. With `include_environments`, each of its environments
/// goes next to it as "<file stem> - <environment>.postman_environment.json",
/// replacing a file of that name. Returns the written file's name, or `None`
/// when the user cancels.
#[tauri::command(async)]
#[specta::specta]
pub fn export_postman_collection(
    collection_id: String,
    include_environments: bool,
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<Option<String>, CommandError> {
    let name: String = state.database.with_read_connection(|connection| {
        Ok(connection.query_row(
            "SELECT name FROM collections WHERE id = ?",
            params![collection_id],
            |row| row.get(0),
        )?)
    })?;
    let Some(picked) = postman_file_dialog(&window)
        .set_file_name(format!(
            "{}.postman_collection.json",
            file_name_part(&name, "Collection")
        ))
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(CommandError::msg)?;
    export_file(&state, &collection_id, &path, include_environments)?;
    Ok(Some(
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
    ))
}

fn export_file(
    state: &AppState,
    collection_id: &str,
    path: &Path,
    include_environments: bool,
) -> Result<(), CommandError> {
    let (collection, environments) = state.database.with_read_connection(|connection| {
        let collection = read_collection(connection, collection_id)?;
        let environments = if include_environments {
            read_environments(connection, collection_id)?
        } else {
            Vec::new()
        };
        Ok((collection, environments))
    })?;

    write_json(path, &collection)?;
    let directory = path.parent().unwrap_or(Path::new(""));
    for (file_name, environment) in environment_files(&export_stem(path), &environments) {
        write_json(&directory.join(file_name), environment)?;
    }
    Ok(())
}

/// The chosen file's name without `.postman_collection.json`, or else
/// without `.json`, so environment files sit beside it under the same name.
fn export_stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lower = name.to_lowercase();
    let stem = [".postman_collection.json", ".json"]
        .into_iter()
        .find(|suffix| lower.ends_with(suffix))
        .map_or(name.as_str(), |suffix| &name[..name.len() - suffix.len()]);
    match stem.trim() {
        "" => "Collection".to_string(),
        stem => stem.to_string(),
    }
}

/// Writes to a temporary file beside `path` and renames it over `path`, so a
/// failed write never leaves a cut-off file where an older export was.
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), CommandError> {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = path.with_file_name(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    let written = (|| -> Result<(), CommandError> {
        let mut writer = BufWriter::new(File::create(&temp)?);
        serde_json::to_writer_pretty(&mut writer, value)?;
        writer.flush()?;
        drop(writer);
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

#[derive(Debug, Serialize)]
struct PostmanCollection {
    info: PostmanInfo,
    item: Vec<PostmanItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth: Option<PostmanAuth>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    variable: Vec<PostmanParam>,
}

#[derive(Debug, Serialize)]
struct PostmanInfo {
    #[serde(rename = "_postman_id")]
    id: String,
    name: String,
    schema: &'static str,
}

/// A folder always carries `item`, even empty, since that is what makes it a
/// folder on import.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum PostmanItem {
    Folder {
        name: String,
        item: Vec<PostmanItem>,
        #[serde(skip_serializing_if = "Option::is_none")]
        auth: Option<PostmanAuth>,
    },
    Request {
        name: String,
        request: Box<PostmanRequest>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        event: Vec<PostmanEvent>,
    },
}

#[derive(Debug, Serialize)]
struct PostmanRequest {
    method: String,
    header: Vec<PostmanParam>,
    url: PostmanUrl,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<PostmanBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth: Option<PostmanAuth>,
}

/// Postman reads the URL from its parts and ignores `raw`, so the parts are
/// written too.
#[derive(Debug, Serialize)]
struct PostmanUrl {
    raw: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    protocol: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    host: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    path: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    query: Vec<PostmanParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hash: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    variable: Vec<PostmanParam>,
}

/// A header, query, path, urlencoded or form row, or a collection variable.
#[derive(Debug, Default, Serialize)]
struct PostmanParam {
    key: String,
    value: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    disabled: bool,
    // Form-data rows only.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    field_type: Option<BodyFieldType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    src: Option<String>,
    #[serde(rename = "contentType", skip_serializing_if = "Option::is_none")]
    content_type: Option<String>,
}

impl From<&KeyValue> for PostmanParam {
    fn from(row: &KeyValue) -> Self {
        Self {
            key: row.key.clone(),
            value: row.value.clone(),
            disabled: !row.enabled,
            ..Self::default()
        }
    }
}

#[derive(Debug, Serialize)]
struct PostmanBody {
    mode: BodyMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    formdata: Option<Vec<PostmanParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    urlencoded: Option<Vec<PostmanParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    graphql: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<Value>,
}

#[derive(Debug, Serialize)]
struct PostmanAuth {
    #[serde(rename = "type")]
    auth_type: AuthType,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    bearer: Vec<PostmanAuthParam>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    basic: Vec<PostmanAuthParam>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    apikey: Vec<PostmanAuthParam>,
}

#[derive(Debug, Serialize)]
struct PostmanAuthParam {
    key: &'static str,
    value: String,
    #[serde(rename = "type")]
    value_type: &'static str,
}

#[derive(Debug, Serialize)]
struct PostmanEvent {
    listen: &'static str,
    script: Value,
}

#[derive(Debug, Serialize)]
struct PostmanEnvironment {
    id: String,
    name: String,
    values: Vec<PostmanEnvironmentValue>,
    #[serde(rename = "_postman_variable_scope")]
    scope: &'static str,
}

#[derive(Debug, Serialize)]
struct PostmanEnvironmentValue {
    key: String,
    value: String,
    #[serde(rename = "type")]
    value_type: &'static str,
    enabled: bool,
}

fn read_collection(
    connection: &Connection,
    collection_id: &str,
) -> Result<PostmanCollection, StorageError> {
    let (name, auth_json): (String, Option<String>) = connection.query_row(
        "SELECT name, auth_json FROM collections WHERE id = ?",
        params![collection_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let variable = read_variables(
        connection,
        "SELECT key, current_value, enabled, sensitive FROM variables
         WHERE scope = 'collection' AND collection_id = ? ORDER BY key",
        collection_id,
    )?
    .into_iter()
    .map(|(key, value, enabled, _)| PostmanParam {
        key,
        value,
        disabled: !enabled,
        ..PostmanParam::default()
    })
    .collect();

    Ok(PostmanCollection {
        info: PostmanInfo {
            id: collection_id.to_string(),
            name,
            schema: SCHEMA,
        },
        item: read_items(connection, collection_id)?,
        auth: postman_auth(auth_json.as_deref()),
        variable,
    })
}

struct NodeRow {
    id: String,
    name: String,
    auth_json: Option<String>,
    request: Option<RequestRow>,
}

struct RequestRow {
    method: String,
    url: String,
    headers_json: String,
    query_json: String,
    path_params_json: String,
    auth_json: Option<String>,
    body_json: Option<String>,
    pre_request_script_json: Option<String>,
    test_script_json: Option<String>,
}

/// Reads every node once, then nests them in sidebar order.
fn read_items(
    connection: &Connection,
    collection_id: &str,
) -> Result<Vec<PostmanItem>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT n.id, n.parent_id, n.name, n.auth_json, n.kind, r.method, r.url,
                r.headers_json, r.query_json, r.path_params_json, r.auth_json, r.body_json,
                r.pre_request_script_json, r.test_script_json
         FROM collection_nodes n
         LEFT JOIN requests r ON r.id = n.request_id
         WHERE n.collection_id = ?
         ORDER BY n.sort_order",
    )?;
    let mut children: HashMap<Option<String>, Vec<NodeRow>> = HashMap::new();
    let rows = statement.query_map(params![collection_id], |row| {
        let request = if row.get::<_, String>(4)? == "request" {
            Some(RequestRow {
                method: row.get(5)?,
                url: row.get(6)?,
                headers_json: row.get(7)?,
                query_json: row.get(8)?,
                path_params_json: row.get(9)?,
                auth_json: row.get(10)?,
                body_json: row.get(11)?,
                pre_request_script_json: row.get(12)?,
                test_script_json: row.get(13)?,
            })
        } else {
            None
        };
        Ok((
            row.get::<_, Option<String>>(1)?,
            NodeRow {
                id: row.get(0)?,
                name: row.get(2)?,
                auth_json: row.get(3)?,
                request,
            },
        ))
    })?;
    for row in rows {
        let (parent_id, node) = row?;
        children.entry(parent_id).or_default().push(node);
    }
    Ok(nest(&mut children, None))
}

fn nest(
    children: &mut HashMap<Option<String>, Vec<NodeRow>>,
    parent_id: Option<String>,
) -> Vec<PostmanItem> {
    let Some(nodes) = children.remove(&parent_id) else {
        return Vec::new();
    };
    nodes
        .into_iter()
        .map(|node| match node.request {
            Some(request) => PostmanItem::Request {
                name: node.name,
                event: events(&request),
                request: Box::new(postman_request(request)),
            },
            None => PostmanItem::Folder {
                item: nest(children, Some(node.id)),
                name: node.name,
                auth: postman_auth(node.auth_json.as_deref()),
            },
        })
        .collect()
}

fn param_rows(json: &str) -> Vec<PostmanParam> {
    serde_json::from_str::<Vec<KeyValue>>(json)
        .unwrap_or_default()
        .iter()
        .map(PostmanParam::from)
        .collect()
}

fn postman_request(request: RequestRow) -> PostmanRequest {
    PostmanRequest {
        method: request.method,
        header: param_rows(&request.headers_json),
        url: postman_url(
            &request.url,
            param_rows(&request.query_json),
            param_rows(&request.path_params_json),
        ),
        body: request
            .body_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<RequestBody>(json).ok())
            .and_then(postman_body),
        auth: postman_auth(request.auth_json.as_deref()),
    }
}

/// Splits `raw` the way Postman does: protocol, `.`-separated host, port,
/// `/`-separated path and hash. The query comes from the request's rows, which
/// also hold the disabled ones.
fn postman_url(raw: &str, query: Vec<PostmanParam>, variable: Vec<PostmanParam>) -> PostmanUrl {
    let (rest, hash) = match raw.split_once('#') {
        Some((rest, hash)) => (rest, Some(hash.to_string())),
        None => (raw, None),
    };
    let rest = rest.split_once('?').map_or(rest, |(base, _)| base);
    let (protocol, rest) = match rest.split_once("://") {
        Some((protocol, rest)) if !protocol.is_empty() && !protocol.contains('/') => {
            (Some(protocol.to_string()), rest)
        }
        _ => (None, rest),
    };
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, Some(path)),
        None => (rest, None),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if is_port(port) => (host, Some(port.to_string())),
        _ => (authority, None),
    };

    PostmanUrl {
        raw: raw.to_string(),
        protocol,
        host: if host.is_empty() {
            Vec::new()
        } else {
            split_outside_variables(host, '.')
        },
        port,
        path: path.map_or_else(Vec::new, |path| split_outside_variables(path, '/')),
        query,
        hash,
        variable,
    }
}

/// Digits, or one whole `{{variable}}`.
fn is_port(text: &str) -> bool {
    (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
        || (text.starts_with("{{") && text.ends_with("}}") && !text[2..].contains("{{"))
}

/// Splits on `separator`, except inside `{{...}}`, so `{{base.url}}` stays
/// one host part.
fn split_outside_variables(text: &str, separator: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut part = String::new();
    let mut depth = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(char) = chars.next() {
        match char {
            '{' if chars.next_if_eq(&'{').is_some() => {
                depth += 1;
                part.push_str("{{");
            }
            '}' if depth > 0 && chars.next_if_eq(&'}').is_some() => {
                depth -= 1;
                part.push_str("}}");
            }
            _ if char == separator && depth == 0 => parts.push(std::mem::take(&mut part)),
            _ => part.push(char),
        }
    }
    parts.push(part);
    parts
}

/// `None` for no body, and for a mode Postman would not send either.
fn postman_body(body: RequestBody) -> Option<PostmanBody> {
    let mut mapped = PostmanBody {
        mode: body.mode,
        raw: None,
        options: None,
        formdata: None,
        urlencoded: None,
        graphql: None,
        file: None,
    };
    match body.mode {
        BodyMode::None | BodyMode::Unsupported => return None,
        BodyMode::Raw => {
            mapped.raw = Some(body.raw);
            mapped.options = body
                .raw_language
                .map(|language| serde_json::json!({ "raw": { "language": language } }));
        }
        BodyMode::FormData => {
            mapped.formdata = Some(body.form_data.into_iter().map(form_field).collect());
        }
        BodyMode::UrlEncoded => {
            mapped.urlencoded = Some(body.urlencoded.iter().map(PostmanParam::from).collect());
        }
        BodyMode::Graphql => {
            let graphql = body.graphql.unwrap_or_default();
            mapped.graphql = Some(serde_json::json!({
                "query": graphql.query,
                "variables": graphql.variables,
            }));
        }
        BodyMode::File => {
            let file = body.file.unwrap_or_default();
            mapped.file = Some(serde_json::json!({
                "src": file.path,
                "contentType": file.content_type,
            }));
        }
    }
    Some(mapped)
}

fn form_field(field: BodyField) -> PostmanParam {
    let is_file = field.field_type == BodyFieldType::File;
    PostmanParam {
        key: field.key,
        value: if is_file { String::new() } else { field.value },
        disabled: !field.enabled,
        field_type: Some(field.field_type),
        src: if is_file { field.file_path } else { None },
        content_type: field.content_type,
    }
}

/// `None` inherits from above, as on import. A type Conductor cannot apply
/// sends no auth, so it goes out as Postman's explicit `noauth`.
fn postman_auth(auth_json: Option<&str>) -> Option<PostmanAuth> {
    let auth: AuthConfig = serde_json::from_str(auth_json?).ok()?;
    let param = |key, value: Option<String>| PostmanAuthParam {
        key,
        value: value.unwrap_or_default(),
        value_type: "string",
    };
    let mut mapped = PostmanAuth {
        auth_type: auth.auth_type,
        bearer: Vec::new(),
        basic: Vec::new(),
        apikey: Vec::new(),
    };
    match auth.auth_type {
        AuthType::Bearer => mapped.bearer = vec![param("token", auth.token)],
        AuthType::Basic => {
            mapped.basic = vec![
                param("username", auth.username),
                param("password", auth.password),
            ];
        }
        AuthType::ApiKey => {
            let location = match auth.add_to {
                Some(ApiKeyLocation::Query) => "query",
                _ => "header",
            };
            mapped.apikey = vec![
                param("key", auth.key),
                param("value", auth.value),
                param("in", Some(location.to_string())),
            ];
        }
        AuthType::NoAuth => {}
        AuthType::Unsupported => mapped.auth_type = AuthType::NoAuth,
    }
    Some(mapped)
}

/// Scripts are stored as Postman's own `script` object, so they go back as is.
fn events(request: &RequestRow) -> Vec<PostmanEvent> {
    [
        ("prerequest", &request.pre_request_script_json),
        ("test", &request.test_script_json),
    ]
    .into_iter()
    .filter_map(|(listen, json)| {
        let script: Value = serde_json::from_str(json.as_deref()?).ok()?;
        (!script.is_null()).then_some(PostmanEvent { listen, script })
    })
    .collect()
}

/// `(key, value, enabled, sensitive)`, from a query that selects them in that
/// order with the owner's id as its one parameter.
fn read_variables(
    connection: &Connection,
    sql: &str,
    owner_id: &str,
) -> Result<Vec<(String, String, bool, bool)>, StorageError> {
    let mut statement = connection.prepare_cached(sql)?;
    let rows = statement.query_map(params![owner_id], |row| {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get::<_, i64>(2)? != 0,
            row.get::<_, i64>(3)? != 0,
        ))
    })?;
    rows.collect::<Result<_, _>>().map_err(StorageError::from)
}

fn read_environments(
    connection: &Connection,
    collection_id: &str,
) -> Result<Vec<PostmanEnvironment>, StorageError> {
    let mut statement = connection
        .prepare("SELECT id, name FROM environments WHERE collection_id = ? ORDER BY name")?;
    let environments = statement
        .query_map(params![collection_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    environments
        .into_iter()
        .map(|(id, name)| {
            let values = read_variables(
                connection,
                "SELECT key, current_value, enabled, sensitive FROM variables
                 WHERE scope = 'environment' AND environment_id = ? ORDER BY key",
                &id,
            )?
            .into_iter()
            .map(|(key, value, enabled, sensitive)| PostmanEnvironmentValue {
                key,
                value,
                value_type: if sensitive { "secret" } else { "default" },
                enabled,
            })
            .collect();
            Ok(PostmanEnvironment {
                id,
                name,
                values,
                scope: "environment",
            })
        })
        .collect()
}

/// One file name per environment, unique even when two names clean up to the
/// same text. `stem` is the collection file's name without its extension.
fn environment_files<'a>(
    stem: &str,
    environments: &'a [PostmanEnvironment],
) -> Vec<(PathBuf, &'a PostmanEnvironment)> {
    let mut used = HashSet::new();
    environments
        .iter()
        .map(|environment| {
            let stem = format!(
                "{stem} - {}",
                file_name_part(&environment.name, "Environment")
            );
            let mut name = stem.clone();
            let mut copy = 1;
            while !used.insert(name.to_lowercase()) {
                copy += 1;
                name = format!("{stem} {copy}");
            }
            (
                PathBuf::from(format!("{name}.postman_environment.json")),
                environment,
            )
        })
        .collect()
}

/// Replaces what file systems reject in a name, and trims what Windows drops.
fn file_name_part(name: &str, fallback: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|char| match char {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            char if char.is_control() => '-',
            char => char,
        })
        .collect();
    match cleaned.trim().trim_end_matches('.').trim_end() {
        "" => fallback.to_string(),
        trimmed => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use serde_json::{json, Value};

    use super::*;
    use crate::commands::collections::import::store_collection;
    use crate::commands::models::VariableTarget;
    use crate::{fixtures, storage::test_connection};

    fn import(connection: &mut Connection, collection: Value) -> String {
        let collection = serde_json::from_value(collection).expect("collection should parse");
        store_collection(connection, &collection, "now").expect("collection should import")
    }

    fn export(connection: &Connection, collection_id: &str) -> Value {
        serde_json::to_value(read_collection(connection, collection_id).unwrap()).unwrap()
    }

    /// Everything a collection stores apart from ids and timestamps, in tree
    /// order, so two collections compare equal when they hold the same thing.
    fn stored(connection: &Connection, collection_id: &str) -> Vec<Vec<Option<String>>> {
        let mut rows = Vec::new();
        let mut collection = connection
            .prepare("SELECT name, auth_json FROM collections WHERE id = ?")
            .unwrap();
        rows.extend(
            collection
                .query_map([collection_id], |row| Ok(vec![row.get(0)?, row.get(1)?]))
                .unwrap()
                .map(Result::unwrap),
        );
        let mut variables = connection
            .prepare(
                "SELECT key, current_value, CAST(enabled AS TEXT) FROM variables
                 WHERE collection_id = ? ORDER BY key",
            )
            .unwrap();
        rows.extend(
            variables
                .query_map([collection_id], |row| {
                    Ok(vec![row.get(0)?, row.get(1)?, row.get(2)?])
                })
                .unwrap()
                .map(Result::unwrap),
        );
        let mut nodes = connection
            .prepare(
                "SELECT n.name, p.name, n.kind, n.auth_json, r.method, r.url, r.headers_json,
                        r.query_json, r.path_params_json, r.auth_json, r.body_json,
                        r.pre_request_script_json, r.test_script_json
                 FROM collection_nodes n
                 LEFT JOIN collection_nodes p ON p.id = n.parent_id
                 LEFT JOIN requests r ON r.id = n.request_id
                 WHERE n.collection_id = ?
                 ORDER BY n.sort_order, n.name",
            )
            .unwrap();
        rows.extend(
            nodes
                .query_map([collection_id], |row| {
                    (0..13).map(|index| row.get(index)).collect()
                })
                .unwrap()
                .map(Result::unwrap),
        );
        rows
    }

    #[test]
    fn a_collection_exported_and_imported_again_stores_the_same_requests_auth_scripts_and_variables(
    ) {
        let mut connection = test_connection();
        let script = json!({ "type": "text/javascript", "exec": ["pm.environment.set(\"token\", pm.response.json().token);"] });
        let original = import(
            &mut connection,
            json!({
                "info": { "name": "Shop" },
                "auth": { "type": "bearer", "bearer": [{ "key": "token", "value": "{{token}}" }] },
                "variable": [
                    { "key": "baseUrl", "value": "https://shop.test" },
                    { "key": "legacy", "value": "old", "disabled": true }
                ],
                "item": [
                    {
                        "name": "Orders",
                        "auth": { "type": "basic", "basic": [{ "key": "username", "value": "ada" }, { "key": "password", "value": "s3cret" }] },
                        "item": [
                            { "name": "Empty", "item": [] },
                            {
                                "name": "Get order",
                                "event": [{ "listen": "test", "script": script }, { "listen": "prerequest", "script": script }],
                                "request": {
                                    "method": "GET",
                                    "header": [{ "key": "Accept", "value": "application/json" }, { "key": "X-Debug", "value": "1", "disabled": true }],
                                    "url": {
                                        "raw": "{{baseUrl}}/orders/:id?expand=items",
                                        "query": [{ "key": "expand", "value": "items" }, { "key": "page", "value": "2", "disabled": true }],
                                        "variable": [{ "key": "id", "value": "42" }]
                                    },
                                    "auth": { "type": "apikey", "apikey": [{ "key": "key", "value": "X-Key" }, { "key": "value", "value": "abc" }, { "key": "in", "value": "query" }] }
                                }
                            },
                            {
                                "name": "Create order",
                                "request": {
                                    "method": "POST",
                                    "url": "{{baseUrl}}/orders",
                                    "body": { "mode": "raw", "raw": "{\"sku\":1}", "options": { "raw": { "language": "json" } } },
                                    "auth": { "type": "noauth" }
                                }
                            }
                        ]
                    },
                    {
                        "name": "Login",
                        "request": {
                            "method": "POST",
                            "url": "{{baseUrl}}/login",
                            "body": { "mode": "urlencoded", "urlencoded": [{ "key": "user", "value": "ada" }, { "key": "otp", "value": "", "disabled": true }] }
                        }
                    },
                    {
                        "name": "Upload",
                        "request": {
                            "method": "PUT",
                            "url": "{{baseUrl}}/files",
                            "body": { "mode": "formdata", "formdata": [{ "key": "note", "value": "hi", "type": "text" }, { "key": "file", "type": "file", "src": "/tmp/a.png", "contentType": "image/png" }] }
                        }
                    },
                    {
                        "name": "Search",
                        "request": {
                            "method": "POST",
                            "url": "{{baseUrl}}/graphql",
                            "body": { "mode": "graphql", "graphql": { "query": "{ orders { id } }", "variables": "{\"first\":5}" } }
                        }
                    }
                ]
            }),
        );

        let exported = export(&connection, &original);
        let copy = import(&mut connection, exported);

        assert_eq!(stored(&connection, &copy), stored(&connection, &original));
    }

    #[test]
    fn urls_are_split_into_the_parts_postman_reads_instead_of_raw() {
        let url = postman_url(
            "https://{{host}}:8080/users/:id?expand=1#top",
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(url.protocol.as_deref(), Some("https"));
        assert_eq!(url.host, ["{{host}}"]);
        assert_eq!(url.port.as_deref(), Some("8080"));
        assert_eq!(url.path, ["users", ":id"]);
        assert_eq!(url.hash.as_deref(), Some("top"));

        let url = postman_url("{{base.url}}/v1/{{version.name}}", Vec::new(), Vec::new());
        assert_eq!(url.protocol, None);
        assert_eq!(url.host, ["{{base.url}}"]);
        assert_eq!(url.path, ["v1", "{{version.name}}"]);

        let url = postman_url("api.shop.test:{{port}}", Vec::new(), Vec::new());
        assert_eq!(url.host, ["api", "shop", "test"]);
        assert_eq!(url.port.as_deref(), Some("{{port}}"));
        assert!(url.path.is_empty());
    }

    #[test]
    fn environments_are_written_beside_the_collection_under_its_file_name_with_their_secret_values()
    {
        let app = fixtures::TestApp::new();
        let collection_id = app.seed(|connection| {
            let collection_id = fixtures::collection(connection);
            let environment = fixtures::environment(connection, &collection_id);
            let target = VariableTarget::Environment {
                environment_id: environment,
            };
            fixtures::variable(connection, &target, "host", "staging.test");
            crate::storage::upsert_variable(
                connection,
                &crate::storage::NewVariable {
                    target: &target,
                    key: "token",
                    value: "s3cret",
                    enabled: false,
                    sensitive: true,
                },
                "",
            )
            .unwrap();
            collection_id
        });
        let directory = std::env::temp_dir().join(format!("conductor-export-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();

        let result = export_file(
            &app.state,
            &collection_id,
            &directory.join("Shop v2.postman_collection.json"),
            true,
        );
        let read = |name: &str| -> Option<Value> {
            serde_json::from_slice(&std::fs::read(directory.join(name)).ok()?).ok()
        };
        let collection = read("Shop v2.postman_collection.json");
        let environment = read("Shop v2 - Environment.postman_environment.json");
        std::fs::remove_dir_all(&directory).unwrap();
        result.unwrap();

        assert_eq!(collection.unwrap()["info"]["name"], "Collection");
        let environment = environment.expect("environment file should be written");
        assert_eq!(environment["name"], "Environment");
        assert_eq!(
            environment["values"],
            json!([
                { "key": "host", "value": "staging.test", "type": "default", "enabled": true },
                { "key": "token", "value": "s3cret", "type": "secret", "enabled": false }
            ])
        );
    }

    #[test]
    fn environment_file_names_stay_unique_and_valid() {
        let environment = |name: &str| PostmanEnvironment {
            id: String::new(),
            name: name.to_string(),
            values: Vec::new(),
            scope: "environment",
        };
        let environments = [
            environment("Prod/EU"),
            environment("Prod:EU"),
            environment("..."),
        ];

        let names = environment_files("Shop", &environments)
            .into_iter()
            .map(|(path, _)| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            [
                "Shop - Prod-EU.postman_environment.json",
                "Shop - Prod-EU 2.postman_environment.json",
                "Shop - Environment.postman_environment.json",
            ]
        );
    }

    #[test]
    fn environment_files_take_the_chosen_file_name_without_its_extension() {
        let stem = |name: &str| export_stem(Path::new("/exports").join(name).as_path());
        assert_eq!(stem("Shop v2.postman_collection.json"), "Shop v2");
        assert_eq!(stem("Shop.Postman_Collection.JSON"), "Shop");
        assert_eq!(stem("shop.json"), "shop");
        assert_eq!(stem("shop.backup"), "shop.backup");
        assert_eq!(stem(".json"), "Collection");
    }

    #[test]
    fn a_failed_export_leaves_the_file_it_would_replace_untouched() {
        let directory = std::env::temp_dir().join(format!("conductor-export-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("Shop.postman_collection.json");
        std::fs::write(&path, "previous export").unwrap();
        // JSON object keys must be strings, so this fails partway through writing.
        let unwritable = std::collections::BTreeMap::from([((1, 2), "value")]);

        let result = write_json(&path, &unwritable);
        let contents = std::fs::read_to_string(&path).unwrap();
        let files = std::fs::read_dir(&directory).unwrap().count();
        std::fs::remove_dir_all(&directory).unwrap();

        assert!(result.is_err());
        assert_eq!(contents, "previous export");
        assert_eq!(files, 1, "the temporary file should be removed");
    }
}
