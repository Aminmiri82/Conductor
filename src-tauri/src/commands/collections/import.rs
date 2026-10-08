//! Postman collection v2/v2.1 import. The file is read once into the typed
//! structs below (unknown fields ignored, missing ones defaulted), mapped to
//! the same models the editor saves, and written in one transaction.

use chrono::Utc;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;
use tauri::State;
use uuid::Uuid;

use super::tree::sort_order_at;
use crate::commands::{
    error::CommandError,
    models::{
        ApiKeyLocation, AuthConfig, AuthType, BodyField, BodyFieldType, BodyMode, FileBody,
        GraphqlBody, KeyValue, RequestBody, VariableTarget,
    },
    postman::{
        flag_or_false, lenient_enum, lenient_list, null_default, object_or_text, stringified, text,
        text_or_empty, FromText,
    },
};
use crate::storage::{
    insert_node, insert_request, upsert_variable, NewNode, NewRequest, NewVariable, NodeKind,
    StorageError,
};
use crate::AppState;

#[tauri::command(async)]
#[specta::specta]
pub fn import_postman_collection(
    postman_json: String,
    state: State<'_, AppState>,
) -> Result<String, CommandError> {
    let collection: PostmanCollection = serde_json::from_str(&postman_json)?;
    let now = Utc::now().to_rfc3339();

    Ok(state
        .database
        .with_connection(|connection| store_collection(connection, &collection, &now))?)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanCollection {
    #[serde(deserialize_with = "null_default")]
    info: PostmanInfo,
    #[serde(deserialize_with = "lenient_list")]
    item: Vec<PostmanItem>,
    #[serde(deserialize_with = "lenient_list")]
    variable: Vec<PostmanVariable>,
    auth: Option<PostmanAuth>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanInfo {
    #[serde(deserialize_with = "text")]
    name: Option<String>,
}

/// A folder when `item` is present, otherwise a request.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanItem {
    #[serde(deserialize_with = "text")]
    name: Option<String>,
    item: Option<Vec<PostmanItem>>,
    #[serde(deserialize_with = "object_or_text")]
    request: Option<PostmanRequest>,
    auth: Option<PostmanAuth>,
    #[serde(deserialize_with = "lenient_list")]
    event: Vec<PostmanEvent>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanRequest {
    #[serde(deserialize_with = "text")]
    method: Option<String>,
    #[serde(deserialize_with = "object_or_text")]
    url: Option<PostmanUrl>,
    #[serde(deserialize_with = "lenient_list")]
    header: Vec<PostmanParam>,
    body: Option<PostmanBody>,
    auth: Option<PostmanAuth>,
}

impl FromText for PostmanRequest {
    fn from_text(url: &str) -> Self {
        Self {
            url: Some(PostmanUrl::from_text(url)),
            ..Self::default()
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanUrl {
    #[serde(deserialize_with = "text_or_empty")]
    raw: String,
    #[serde(deserialize_with = "lenient_list")]
    query: Vec<PostmanParam>,
}

impl FromText for PostmanUrl {
    fn from_text(raw: &str) -> Self {
        Self {
            raw: raw.to_string(),
            query: Vec::new(),
        }
    }
}

/// A header, query, urlencoded or form row. Rows without a key are dropped.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanParam {
    #[serde(deserialize_with = "text")]
    key: Option<String>,
    #[serde(deserialize_with = "text_or_empty")]
    value: String,
    #[serde(deserialize_with = "flag_or_false")]
    disabled: bool,
    // Form-data rows only.
    #[serde(rename = "type", deserialize_with = "lenient_enum")]
    field_type: Option<BodyFieldType>,
    #[serde(rename = "contentType", deserialize_with = "text")]
    content_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanBody {
    #[serde(deserialize_with = "lenient_enum")]
    mode: Option<BodyMode>,
    #[serde(deserialize_with = "text_or_empty")]
    raw: String,
    #[serde(deserialize_with = "null_default")]
    options: PostmanBodyOptions,
    #[serde(deserialize_with = "lenient_list")]
    formdata: Vec<PostmanParam>,
    #[serde(deserialize_with = "lenient_list")]
    urlencoded: Vec<PostmanParam>,
    #[serde(deserialize_with = "null_default")]
    graphql: PostmanGraphql,
    #[serde(deserialize_with = "null_default")]
    file: PostmanFile,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanBodyOptions {
    #[serde(deserialize_with = "null_default")]
    raw: PostmanRawOptions,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanRawOptions {
    #[serde(deserialize_with = "text")]
    language: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanGraphql {
    #[serde(deserialize_with = "text_or_empty")]
    query: String,
    #[serde(deserialize_with = "text_or_empty")]
    variables: String,
}

/// Postman's `src` (a path on the exporter's machine) is deliberately not read.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanFile {
    #[serde(rename = "contentType", deserialize_with = "text")]
    content_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanAuth {
    #[serde(rename = "type", deserialize_with = "lenient_enum")]
    auth_type: Option<AuthType>,
    #[serde(deserialize_with = "lenient_list")]
    bearer: Vec<PostmanAuthParam>,
    #[serde(deserialize_with = "lenient_list")]
    basic: Vec<PostmanAuthParam>,
    #[serde(deserialize_with = "lenient_list")]
    apikey: Vec<PostmanAuthParam>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanAuthParam {
    #[serde(deserialize_with = "text")]
    key: Option<String>,
    #[serde(deserialize_with = "text_or_empty")]
    value: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanEvent {
    #[serde(deserialize_with = "text")]
    listen: Option<String>,
    /// Stored as the file has it: the editor and `postman_scripts` read its
    /// `exec` lines.
    script: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PostmanVariable {
    #[serde(deserialize_with = "text")]
    key: Option<String>,
    #[serde(deserialize_with = "stringified")]
    value: String,
    #[serde(deserialize_with = "flag_or_false")]
    disabled: bool,
}

/// Returns the new collection's id.
fn store_collection(
    connection: &mut Connection,
    collection: &PostmanCollection,
    now: &str,
) -> Result<String, StorageError> {
    let collection_id = Uuid::new_v4().to_string();
    let tx = connection.transaction()?;
    tx.execute(
        "INSERT INTO collections (id, name, auth_json, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?)",
        rusqlite::params![
            collection_id,
            collection
                .info
                .name
                .as_deref()
                .unwrap_or("Imported collection"),
            auth_json(collection.auth.as_ref())?,
            now,
            now
        ],
    )?;

    let target = VariableTarget::Collection {
        collection_id: collection_id.clone(),
    };
    for variable in &collection.variable {
        let Some(key) = variable.key.as_deref() else {
            continue;
        };
        upsert_variable(
            &tx,
            &NewVariable {
                target: &target,
                key,
                value: &variable.value,
                enabled: !variable.disabled,
                sensitive: false,
            },
            now,
        )?;
    }

    store_items(&tx, &collection_id, None, &collection.item, now)?;
    tx.commit()?;
    Ok(collection_id)
}

fn store_items(
    connection: &Connection,
    collection_id: &str,
    parent_id: Option<&str>,
    items: &[PostmanItem],
    now: &str,
) -> Result<(), StorageError> {
    for (position, item) in items.iter().enumerate() {
        let name = item.name.as_deref().unwrap_or("Untitled");
        let node_id = Uuid::new_v4().to_string();
        let sort_order = sort_order_at(position);

        if let Some(children) = &item.item {
            insert_node(
                connection,
                &NewNode {
                    id: &node_id,
                    collection_id,
                    parent_id,
                    sort_order,
                    kind: NodeKind::Folder,
                    name,
                    auth_json: auth_json(item.auth.as_ref())?.as_deref(),
                },
                now,
            )?;
            store_items(connection, collection_id, Some(&node_id), children, now)?;
        } else if let Some(request) = &item.request {
            let request_id = Uuid::new_v4().to_string();
            let (pre_request_script, test_script) = scripts(&item.event)?;
            let url = request.url.as_ref();
            let query = url.map(|url| key_values(&url.query)).unwrap_or_default();
            let body = request
                .body
                .as_ref()
                .map(|body| serde_json::to_string(&request_body(body)))
                .transpose()?;

            insert_request(
                connection,
                &NewRequest {
                    method: &request.method.as_deref().unwrap_or("GET").to_uppercase(),
                    url: url.map(|url| url.raw.as_str()).unwrap_or_default(),
                    headers_json: &serde_json::to_string(&key_values(&request.header))?,
                    query_json: &serde_json::to_string(&query)?,
                    auth_json: auth_json(request.auth.as_ref())?.as_deref(),
                    body_json: body.as_deref(),
                    pre_request_script_json: pre_request_script.as_deref(),
                    test_script_json: test_script.as_deref(),
                    ..NewRequest::blank(&request_id, collection_id)
                },
                now,
            )?;
            insert_node(
                connection,
                &NewNode {
                    id: &node_id,
                    collection_id,
                    parent_id,
                    sort_order,
                    kind: NodeKind::Request(&request_id),
                    name,
                    auth_json: None,
                },
                now,
            )?;
        }
    }

    Ok(())
}

fn key_values(params: &[PostmanParam]) -> Vec<KeyValue> {
    params
        .iter()
        .filter_map(|param| {
            Some(KeyValue {
                key: param.key.clone()?,
                value: param.value.clone(),
                enabled: !param.disabled,
            })
        })
        .collect()
}

fn request_body(body: &PostmanBody) -> RequestBody {
    let mode = body.mode.unwrap_or(BodyMode::None);
    let mut mapped = RequestBody {
        mode,
        raw: String::new(),
        raw_language: None,
        form_data: Vec::new(),
        urlencoded: Vec::new(),
        graphql: None,
        file: None,
    };
    match mode {
        BodyMode::Raw => {
            mapped.raw = body.raw.clone();
            mapped.raw_language = body.options.raw.language.clone();
        }
        BodyMode::FormData => {
            mapped.form_data = body
                .formdata
                .iter()
                .filter_map(|field| {
                    let field_type = field.field_type.unwrap_or_default();
                    Some(BodyField {
                        key: field.key.clone()?,
                        // A file row's value is the exporter's path; the user picks the file again.
                        value: if field_type == BodyFieldType::File {
                            String::new()
                        } else {
                            field.value.clone()
                        },
                        enabled: !field.disabled,
                        field_type,
                        file_path: None,
                        content_type: field.content_type.clone(),
                    })
                })
                .collect();
        }
        BodyMode::UrlEncoded => mapped.urlencoded = key_values(&body.urlencoded),
        BodyMode::Graphql => {
            mapped.graphql = Some(GraphqlBody {
                query: body.graphql.query.clone(),
                variables: body.graphql.variables.clone(),
            });
        }
        BodyMode::File => {
            mapped.file = Some(FileBody {
                path: None,
                content_type: body.file.content_type.clone(),
            });
        }
        BodyMode::None | BodyMode::Unsupported => {}
    }
    mapped
}

/// `None` (no `auth` key, or `null`) inherits from the folder or collection
/// above; `noauth` is an explicit "send none".
fn auth_config(auth: &PostmanAuth) -> AuthConfig {
    let auth_type = auth.auth_type.unwrap_or(AuthType::NoAuth);
    let param = |params: &[PostmanAuthParam], key: &str| {
        params
            .iter()
            .find(|param| param.key.as_deref() == Some(key))
            .map(|param| param.value.clone())
    };
    let mut mapped = AuthConfig {
        auth_type,
        token: None,
        username: None,
        password: None,
        key: None,
        value: None,
        add_to: None,
    };
    match auth_type {
        AuthType::Bearer => mapped.token = param(&auth.bearer, "token"),
        AuthType::Basic => {
            mapped.username = Some(param(&auth.basic, "username").unwrap_or_default());
            mapped.password = Some(param(&auth.basic, "password").unwrap_or_default());
        }
        AuthType::ApiKey => {
            mapped.key =
                Some(param(&auth.apikey, "key").unwrap_or_else(|| "Authorization".to_string()));
            mapped.value = Some(param(&auth.apikey, "value").unwrap_or_default());
            mapped.add_to = Some(match param(&auth.apikey, "in").as_deref() {
                Some("query") => ApiKeyLocation::Query,
                _ => ApiKeyLocation::Header,
            });
        }
        AuthType::NoAuth | AuthType::Unsupported => {}
    }
    mapped
}

fn auth_json(auth: Option<&PostmanAuth>) -> Result<Option<String>, serde_json::Error> {
    auth.map(|auth| serde_json::to_string(&auth_config(auth)))
        .transpose()
}

/// The last `prerequest` and `test` event win, as in Postman.
fn scripts(events: &[PostmanEvent]) -> Result<(Option<String>, Option<String>), serde_json::Error> {
    let mut pre_request = None;
    let mut test = None;
    for event in events {
        match event.listen.as_deref() {
            Some("prerequest") => pre_request = event.script.as_ref(),
            Some("test") => test = event.script.as_ref(),
            _ => {}
        }
    }
    Ok((
        pre_request.map(serde_json::to_string).transpose()?,
        test.map(serde_json::to_string).transpose()?,
    ))
}

#[cfg(test)]
mod tests {
    use rusqlite::{params, Connection};
    use serde_json::json;

    use super::*;
    use crate::commands::collections::tree::read_tree;
    use crate::commands::models::CollectionNodeKind;

    fn import(connection: &mut Connection, collection: Value) -> String {
        let collection = serde_json::from_value(collection).expect("collection should parse");
        store_collection(connection, &collection, "now").expect("collection should import")
    }

    fn imported(collection: Value) -> (Connection, String) {
        let mut connection = crate::storage::test_connection();
        let collection_id = import(&mut connection, collection);
        (connection, collection_id)
    }

    struct StoredRequest {
        method: String,
        url: String,
        headers_json: String,
        query_json: String,
        auth_json: Option<String>,
        body_json: Option<String>,
        test_script_json: Option<String>,
    }

    fn stored_request(connection: &Connection, name: &str) -> StoredRequest {
        connection
            .query_row(
                "SELECT r.method, r.url, r.headers_json, r.query_json, r.auth_json, r.body_json,
                        r.test_script_json
                 FROM requests r JOIN collection_nodes n ON n.request_id = r.id
                 WHERE n.name = ?",
                params![name],
                |row| {
                    Ok(StoredRequest {
                        method: row.get(0)?,
                        url: row.get(1)?,
                        headers_json: row.get(2)?,
                        query_json: row.get(3)?,
                        auth_json: row.get(4)?,
                        body_json: row.get(5)?,
                        test_script_json: row.get(6)?,
                    })
                },
            )
            .unwrap_or_else(|error| panic!("request {name:?} should be stored: {error}"))
    }

    fn stored_auth(auth_json: Option<&str>) -> AuthConfig {
        serde_json::from_str(auth_json.expect("auth should be stored")).unwrap()
    }

    #[test]
    fn postman_graphql_body_preserves_query_and_variables() {
        let body: PostmanBody = serde_json::from_value(json!({
            "mode": "graphql",
            "graphql": {
                "query": "query Viewer($id: ID!) { viewer(id: $id) { name } }",
                "variables": "{\"id\":\"123\"}"
            }
        }))
        .unwrap();

        let body = request_body(&body);

        assert_eq!(body.mode, BodyMode::Graphql);
        let graphql = body.graphql.expect("graphql body should import");
        assert_eq!(
            graphql.query,
            "query Viewer($id: ID!) { viewer(id: $id) { name } }"
        );
        assert_eq!(graphql.variables, "{\"id\":\"123\"}");
    }

    #[test]
    fn postman_modes_and_auth_types_we_cannot_send_import_as_unsupported() {
        let body: PostmanBody = serde_json::from_value(json!({ "mode": "binary" })).unwrap();
        let auth: PostmanAuth =
            serde_json::from_value(json!({ "type": "oauth2", "oauth2": [] })).unwrap();

        assert_eq!(request_body(&body).mode, BodyMode::Unsupported);
        assert_eq!(auth_config(&auth).auth_type, AuthType::Unsupported);
    }

    #[test]
    fn postman_file_body_does_not_preserve_imported_path() {
        let body: PostmanBody = serde_json::from_value(json!({
            "mode": "file",
            "file": {
                "src": "/Users/example/private.bin",
                "contentType": "application/octet-stream"
            }
        }))
        .unwrap();

        let body = request_body(&body);

        assert_eq!(body.mode, BodyMode::File);
        let file = body.file.expect("file body should import");
        assert!(file.path.is_none());
        assert_eq!(
            file.content_type.as_deref(),
            Some("application/octet-stream")
        );
    }

    #[test]
    fn imports_a_request_whose_url_is_a_plain_string() {
        let (connection, _) = imported(json!({
            "info": { "name": "API" },
            "item": [{
                "name": "Create user",
                "request": { "method": "post", "url": "https://api.test/users?active=true" }
            }]
        }));

        let request = stored_request(&connection, "Create user");

        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "https://api.test/users?active=true");
    }

    #[test]
    fn imports_a_request_written_as_just_a_url() {
        let (connection, _) = imported(json!({
            "item": [{ "name": "Ping", "request": "https://api.test/ping" }]
        }));

        let request = stored_request(&connection, "Ping");

        assert_eq!(request.method, "GET");
        assert_eq!(request.url, "https://api.test/ping");
    }

    #[test]
    fn imports_query_rows_and_disabled_flags_from_an_object_url() {
        let (connection, _) = imported(json!({
            "item": [{
                "name": "Search",
                "request": {
                    "url": {
                        "raw": "https://api.test/search?q=rust&page=2",
                        "query": [
                            { "key": "q", "value": "rust" },
                            { "key": "page", "value": "2", "disabled": true },
                            { "value": "no key, dropped" }
                        ]
                    },
                    "header": [{ "key": "Accept", "value": "application/json" }]
                }
            }]
        }));

        let request = stored_request(&connection, "Search");

        let query: Vec<KeyValue> = serde_json::from_str(&request.query_json).unwrap();
        let rows: Vec<_> = query
            .iter()
            .map(|row| (row.key.as_str(), row.value.as_str(), row.enabled))
            .collect();
        assert_eq!(rows, [("q", "rust", true), ("page", "2", false)]);
        let headers: Vec<KeyValue> = serde_json::from_str(&request.headers_json).unwrap();
        assert_eq!(headers[0].key, "Accept");
    }

    #[test]
    fn a_folders_auth_is_stored_on_the_folder() {
        let (connection, collection_id) = imported(json!({
            "item": [{
                "name": "Private",
                "auth": {
                    "type": "bearer",
                    "bearer": [{ "key": "token", "value": "{{token}}", "type": "string" }]
                },
                "item": [{ "name": "Me", "request": { "url": "https://api.test/me" } }]
            }]
        }));

        let folder_auth: Option<String> = connection
            .query_row(
                "SELECT auth_json FROM collection_nodes
                 WHERE collection_id = ? AND kind = 'folder'",
                params![collection_id],
                |row| row.get(0),
            )
            .unwrap();
        let auth = stored_auth(folder_auth.as_deref());
        assert_eq!(auth.auth_type, AuthType::Bearer);
        assert_eq!(auth.token.as_deref(), Some("{{token}}"));
        assert!(
            stored_request(&connection, "Me").auth_json.is_none(),
            "the request inherits the folder's auth instead of copying it"
        );
    }

    #[test]
    fn imports_basic_and_apikey_auth_with_postmans_defaults() {
        let (connection, _) = imported(json!({
            "item": [
                {
                    "name": "Basic",
                    "request": {
                        "url": "https://api.test",
                        "auth": { "type": "basic", "basic": [{ "key": "username", "value": "yara" }] }
                    }
                },
                {
                    "name": "Key",
                    "request": {
                        "url": "https://api.test",
                        "auth": { "type": "apikey", "apikey": [{ "key": "value", "value": "s3cret" }] }
                    }
                }
            ]
        }));

        let basic = stored_auth(stored_request(&connection, "Basic").auth_json.as_deref());
        assert_eq!(basic.username.as_deref(), Some("yara"));
        assert_eq!(basic.password.as_deref(), Some(""));
        let key = stored_auth(stored_request(&connection, "Key").auth_json.as_deref());
        assert_eq!(key.key.as_deref(), Some("Authorization"));
        assert_eq!(key.value.as_deref(), Some("s3cret"));
        assert_eq!(key.add_to, Some(ApiKeyLocation::Header));
    }

    #[test]
    fn null_auth_inherits_while_noauth_stays_an_explicit_choice() {
        let (connection, _) = imported(json!({
            "item": [
                { "name": "Inherit", "request": { "url": "https://api.test", "auth": null } },
                { "name": "Missing", "request": { "url": "https://api.test" } },
                {
                    "name": "None",
                    "request": { "url": "https://api.test", "auth": { "type": "noauth" } }
                }
            ]
        }));

        assert!(stored_request(&connection, "Inherit").auth_json.is_none());
        assert!(stored_request(&connection, "Missing").auth_json.is_none());
        let none = stored_auth(stored_request(&connection, "None").auth_json.as_deref());
        assert_eq!(none.auth_type, AuthType::NoAuth);
    }

    #[test]
    fn unknown_fields_are_ignored_and_missing_ones_defaulted() {
        let (connection, collection_id) = imported(json!({
            "info": { "_postman_id": "abc", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" },
            "protocolProfileBehavior": { "followRedirects": false },
            "item": [{
                "id": "1234",
                "response": [{ "name": "example", "code": 200 }],
                "request": {
                    "description": "ignored",
                    "header": "Accept: */*",
                    "body": null,
                    "url": { "host": ["api", "test"], "protocol": "https" }
                }
            }]
        }));

        let tree = read_tree(&connection, &collection_id).unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "Untitled");
        let request = stored_request(&connection, "Untitled");
        assert_eq!(request.method, "GET");
        assert_eq!(request.url, "");
        assert_eq!(request.headers_json, "[]");
        assert!(request.body_json.is_none());
        let name: String = connection
            .query_row(
                "SELECT name FROM collections WHERE id = ?",
                params![collection_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, "Imported collection");
    }

    #[test]
    fn collection_variables_keep_number_values_and_disabled_flags() {
        let (connection, collection_id) = imported(json!({
            "variable": [
                { "key": "port", "value": 8080 },
                { "key": "host", "value": "localhost", "disabled": true },
                { "value": "no key" }
            ]
        }));

        let rows = connection
            .prepare(
                "SELECT key, current_value, enabled FROM variables
                 WHERE collection_id = ? AND scope = 'collection' ORDER BY key",
            )
            .unwrap()
            .query_map(params![collection_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            rows,
            [
                ("host".to_string(), "localhost".to_string(), false),
                ("port".to_string(), "8080".to_string(), true)
            ]
        );
    }

    #[test]
    fn test_scripts_are_stored_exactly_as_the_file_has_them() {
        let script = json!({
            "id": "abc",
            "type": "text/javascript",
            "exec": ["var jsonData = pm.response.json();", "pm.environment.set(\"token\", jsonData.token);"]
        });
        let (connection, _) = imported(json!({
            "item": [{
                "name": "Login",
                "request": { "url": "https://api.test/login" },
                "event": [
                    { "listen": "test", "script": script },
                    { "listen": "unknown", "script": { "exec": ["ignored"] } }
                ]
            }]
        }));

        let stored = stored_request(&connection, "Login")
            .test_script_json
            .unwrap();

        assert_eq!(serde_json::from_str::<Value>(&stored).unwrap(), script);
    }

    #[test]
    fn nested_folders_and_requests_keep_their_order() {
        let (connection, collection_id) = imported(json!({
            "item": [
                { "name": "First", "request": { "url": "https://api.test/1" } },
                {
                    "name": "Folder",
                    "item": [
                        { "name": "Inner folder", "item": [] },
                        { "name": "Nested", "request": { "url": "https://api.test/2" } }
                    ]
                },
                { "name": "Last", "request": { "url": "https://api.test/3" } }
            ]
        }));

        let tree = read_tree(&connection, &collection_id).unwrap();

        let top: Vec<_> = tree.iter().map(|node| node.name.as_str()).collect();
        assert_eq!(top, ["First", "Folder", "Last"]);
        let CollectionNodeKind::Folder { children } = &tree[1].kind else {
            panic!("the second node is a folder");
        };
        let nested: Vec<_> = children
            .iter()
            .map(|node| {
                (
                    node.name.as_str(),
                    matches!(node.kind, CollectionNodeKind::Folder { .. }),
                )
            })
            .collect();
        assert_eq!(nested, [("Inner folder", true), ("Nested", false)]);
        assert!(matches!(
            &tree[0].kind,
            CollectionNodeKind::Request { method, .. } if method == "GET"
        ));
    }
}
