use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Read, Write},
    ops::Range,
    path::{Path, PathBuf},
};

use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use serde_json::{value::RawValue, Value};
use uuid::Uuid;

use crate::{
    commands::models::{
        ApiKeyLocation, AuthConfig, AuthType, BodyField, BodyFieldType, BodyMode, FileBody,
        GraphqlBody, KeyValue, RequestBody,
    },
    storage::StorageError,
};

// Borrow the JSON tokens from the input so edits can copy untouched members,
// including their key spelling and whitespace, byte for byte.
pub(super) struct JsonMember {
    pub(super) key: String,
    pub(super) separator: Range<usize>,
    pub(super) value: Range<usize>,
}

struct Members<'a, F>(&'a [u8], F);

impl<'de, T, F: Fn(JsonMember) -> T> Visitor<'de> for Members<'de, F> {
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a JSON object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let offset = |token: &RawValue| token.get().as_ptr() as usize - self.0.as_ptr() as usize;
        let mut members = Vec::new();
        let mut previous = 1;
        while let Some((key, value)) = map.next_entry::<&RawValue, &RawValue>()? {
            let start = offset(value);
            let end = start + value.get().len();
            members.push((self.1)(JsonMember {
                key: serde_json::from_str(key.get()).map_err(serde::de::Error::custom)?,
                separator: previous..offset(key),
                value: start..end,
            }));
            previous = end;
        }
        Ok(members)
    }
}

pub(super) fn json_members<T>(
    bytes: &[u8],
    map: impl Fn(JsonMember) -> T,
) -> Result<Vec<T>, StorageError> {
    Ok(serde_json::Deserializer::from_slice(bytes).deserialize_map(Members(bytes, map))?)
}

const DUPLICATE_KEY: &str = "duplicate JSON key ";

pub(super) fn unique_keys<'de, D, T>(deserializer: D) -> Result<BTreeMap<String, T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Unique<T>(std::marker::PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for Unique<T> {
        type Value = BTreeMap<String, T>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a JSON object with unique keys")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut entries = BTreeMap::new();
            while let Some(key) = map.next_key::<String>()? {
                match entries.entry(key) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(map.next_value()?);
                    }
                    std::collections::btree_map::Entry::Occupied(entry) => {
                        return Err(duplicate_key(entry.key()));
                    }
                }
            }
            Ok(entries)
        }
    }

    deserializer.deserialize_map(Unique(std::marker::PhantomData))
}

fn duplicate_key<E: de::Error>(key: &str) -> E {
    E::custom(format_args!("{DUPLICATE_KEY}{key:?}"))
}

const CONFLICT_MARKERS: &str = "Git conflict markers";

pub(super) fn decode_json<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, StorageError> {
    serde_json::from_slice(bytes).map_err(|error| {
        if bytes.split(|byte| *byte == b'\n').any(|line| {
            let line = line.trim_ascii_start();
            line.starts_with(b"<<<<<<<") || line.starts_with(b">>>>>>>")
        }) {
            invalid(CONFLICT_MARKERS)
        } else {
            error.into()
        }
    })
}

pub(super) fn at_path(path: &str, error: StorageError) -> StorageError {
    if matches!(&error, StorageError::InvalidInput(message) if message == CONFLICT_MARKERS) {
        invalid(format!("Resolve the git conflict in {path}, then refresh."))
    } else {
        invalid(format!("{path}: {error}"))
    }
}

pub(super) fn decode_local_json<'de, T: Deserialize<'de>>(
    bytes: &'de [u8],
) -> Result<T, StorageError> {
    let path = ".conductor/local.json";
    match decode_json(bytes) {
        Ok(document) if bytes.trim_ascii_start().starts_with(b"{") => Ok(document),
        Ok(_) => Err(invalid(format!("{path}: expected a JSON object."))),
        Err(StorageError::Json(error)) => {
            // Most serde diagnostics can contain a private value. Duplicate errors
            // quote the key alone, so they can identify the edit that needs attention.
            let diagnostic = error.to_string();
            if diagnostic.starts_with(DUPLICATE_KEY) || diagnostic.starts_with("duplicate field `")
            {
                return Err(invalid(format!(
                    "{path}: invalid local values, {diagnostic}."
                )));
            }
            Err(invalid(format!(
                "{path}: invalid local values at line {}, column {}.",
                error.line(),
                error.column()
            )))
        }
        Err(error) => Err(at_path(path, error)),
    }
}

pub(super) fn write_separator(output: &mut Vec<u8>, bytes: &[u8], needs_comma: bool) {
    match (needs_comma, bytes.iter().position(|byte| *byte == b',')) {
        (true, None) => {
            output.push(b',');
            output.extend_from_slice(bytes);
        }
        (false, Some(comma)) => {
            output.extend_from_slice(&bytes[..comma]);
            output.extend_from_slice(&bytes[comma + 1..]);
        }
        _ => output.extend_from_slice(bytes),
    }
}

pub(super) const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FOLDERS: usize = 10_000;
const MAX_FOLDER_DEPTH: usize = 64;
const MAX_READABLE_PATH: usize = 160;
const MAX_POSITION: i64 = 9_007_199_254_740_991;
pub(super) const READABLE_FORMAT: u32 = 2;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Manifest {
    pub(super) conductor_format: u32,
    pub(super) id: String,
    pub(super) name: String,
    #[serde(
        default,
        deserialize_with = "strict_auth",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) auth: Option<AuthConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) folders: Vec<Folder>,
    #[serde(
        default,
        deserialize_with = "unique_keys",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub(super) values: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub(super) secrets: BTreeSet<String>,
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
    #[serde(
        default,
        deserialize_with = "strict_auth",
        skip_serializing_if = "Option::is_none"
    )]
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
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(super) move_generation: u32,
    pub(super) method: String,
    pub(super) url: String,
    #[serde(
        default,
        deserialize_with = "strict_headers",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub(super) headers: Vec<KeyValue>,
    #[serde(
        default,
        deserialize_with = "strict_headers",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub(super) query: Vec<KeyValue>,
    #[serde(
        default,
        deserialize_with = "strict_headers",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub(super) path_params: Vec<KeyValue>,
    #[serde(
        default,
        deserialize_with = "strict_auth",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) auth: Option<AuthConfig>,
    #[serde(
        default,
        deserialize_with = "strict_body",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) body: Option<RequestBody>,
    #[serde(
        default,
        deserialize_with = "strict_script",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) pre_request_script: Option<Value>,
    #[serde(
        default,
        deserialize_with = "strict_script",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) test_script: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Environment {
    pub(super) id: String,
    pub(super) name: String,
    #[serde(
        default,
        deserialize_with = "unique_keys",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub(super) values: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub(super) secrets: BTreeSet<String>,
}

// Repository input is strict while the IPC and previously stored editor data
// retain their existing decoding rules.
#[derive(Deserialize)]
#[serde(tag = "authType", rename_all = "lowercase", deny_unknown_fields)]
enum AuthFields {
    NoAuth {},
    Bearer {
        token: Option<String>,
    },
    Basic {
        username: Option<String>,
        password: Option<String>,
    },
    ApiKey {
        key: Option<String>,
        value: Option<String>,
        #[serde(default, rename = "addTo", deserialize_with = "strict_location")]
        add_to: Option<ApiKeyLocation>,
    },
    Unsupported {},
}

#[derive(Deserialize)]
#[serde(remote = "ApiKeyLocation", rename_all = "lowercase")]
enum StrictLocation {
    Query,
    Header,
}

#[derive(Deserialize)]
struct Location(#[serde(with = "StrictLocation")] ApiKeyLocation);

fn strict_location<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ApiKeyLocation>, D::Error> {
    Ok(Option::<Location>::deserialize(deserializer)?.map(|location| location.0))
}

fn strict_auth<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<AuthConfig>, D::Error> {
    let Some(auth) = Option::<AuthFields>::deserialize(deserializer)? else {
        return Ok(None);
    };
    let mut config = AuthConfig {
        auth_type: AuthType::NoAuth,
        token: None,
        username: None,
        password: None,
        key: None,
        value: None,
        add_to: None,
    };
    config.auth_type = match auth {
        AuthFields::NoAuth {} => AuthType::NoAuth,
        AuthFields::Bearer { token } => {
            config.token = token;
            AuthType::Bearer
        }
        AuthFields::Basic { username, password } => {
            config.username = username;
            config.password = password;
            AuthType::Basic
        }
        AuthFields::ApiKey { key, value, add_to } => {
            config.key = key;
            config.value = value;
            config.add_to = add_to;
            AuthType::ApiKey
        }
        AuthFields::Unsupported {} => AuthType::Unsupported,
    };
    Ok(Some(config))
}

pub(super) fn project_auth(auth: Option<AuthConfig>) -> Option<AuthConfig> {
    auth.map(|mut auth| {
        if auth.auth_type != AuthType::Bearer {
            auth.token = None;
        }
        if auth.auth_type != AuthType::Basic {
            auth.username = None;
            auth.password = None;
        }
        if auth.auth_type != AuthType::ApiKey {
            auth.key = None;
            auth.value = None;
            auth.add_to = None;
        }
        auth
    })
}

#[derive(Deserialize)]
#[serde(remote = "KeyValue", deny_unknown_fields)]
struct StrictHeader {
    key: String,
    value: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

#[derive(Deserialize)]
struct Header(#[serde(with = "StrictHeader")] KeyValue);

fn strict_headers<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<KeyValue>, D::Error> {
    Ok(Vec::<Header>::deserialize(deserializer)?
        .into_iter()
        .map(|header| header.0)
        .collect())
}

#[derive(Deserialize)]
#[serde(remote = "RequestBody", rename_all = "camelCase", deny_unknown_fields)]
struct StrictBody {
    mode: BodyMode,
    #[serde(default)]
    raw: String,
    raw_language: Option<String>,
    #[serde(default, deserialize_with = "strict_form_data")]
    form_data: Vec<BodyField>,
    #[serde(default, deserialize_with = "strict_headers")]
    urlencoded: Vec<KeyValue>,
    #[serde(default, deserialize_with = "strict_graphql")]
    graphql: Option<GraphqlBody>,
    #[serde(default, deserialize_with = "strict_file")]
    file: Option<FileBody>,
}

#[derive(Deserialize)]
#[serde(remote = "BodyField", rename_all = "camelCase", deny_unknown_fields)]
struct StrictBodyField {
    key: String,
    value: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default)]
    field_type: BodyFieldType,
    file_path: Option<String>,
    content_type: Option<String>,
}

#[derive(Deserialize)]
struct FormField(#[serde(with = "StrictBodyField")] BodyField);

fn strict_form_data<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<BodyField>, D::Error> {
    Ok(Vec::<FormField>::deserialize(deserializer)?
        .into_iter()
        .map(|field| field.0)
        .collect())
}

#[derive(Deserialize)]
#[serde(remote = "GraphqlBody", deny_unknown_fields)]
struct StrictGraphql {
    #[serde(default)]
    query: String,
    #[serde(default)]
    variables: String,
}

#[derive(Deserialize)]
struct StrictGraphqlBody(#[serde(with = "StrictGraphql")] GraphqlBody);

fn strict_graphql<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<GraphqlBody>, D::Error> {
    Ok(Option::<StrictGraphqlBody>::deserialize(deserializer)?.map(|body| body.0))
}

#[derive(Deserialize)]
#[serde(remote = "FileBody", rename_all = "camelCase", deny_unknown_fields)]
struct StrictFile {
    path: Option<String>,
    content_type: Option<String>,
}

#[derive(Deserialize)]
struct StrictFileBody(#[serde(with = "StrictFile")] FileBody);

fn strict_file<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<FileBody>, D::Error> {
    Ok(Option::<StrictFileBody>::deserialize(deserializer)?.map(|body| body.0))
}

#[derive(Deserialize)]
struct Body(#[serde(with = "StrictBody")] RequestBody);

fn strict_body<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<RequestBody>, D::Error> {
    Ok(Option::<Body>::deserialize(deserializer)?.map(|body| body.0))
}

struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct JsonVisitor;

        impl<'de> Visitor<'de> for JsonVisitor {
            type Value = Value;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a JSON value")
            }

            fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }

            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
                Ok(value.into())
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
                Ok(value.into())
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
                Ok(value.into())
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
                Ok(value.into())
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
                Ok(value.into())
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
                let mut items = Vec::new();
                while let Some(UniqueValue(item)) = seq.next_element()? {
                    items.push(item);
                }
                Ok(Value::Array(items))
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
                let mut items = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    match items.entry(key) {
                        serde_json::map::Entry::Vacant(entry) => {
                            let UniqueValue(value) = map.next_value()?;
                            entry.insert(value);
                        }
                        serde_json::map::Entry::Occupied(entry) => {
                            return Err(duplicate_key(entry.key()));
                        }
                    }
                }
                Ok(Value::Object(items))
            }
        }

        deserializer.deserialize_any(JsonVisitor).map(Self)
    }
}

fn strict_script<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Ok(Option::<UniqueValue>::deserialize(deserializer)?.map(|value| value.0))
}

fn default_enabled() -> bool {
    true
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

pub(super) fn parse_manifest(bytes: &[u8]) -> Result<Manifest, StorageError> {
    let manifest = decode_json(bytes)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub(super) fn parse_request(bytes: &[u8]) -> Result<Request, StorageError> {
    let mut request = decode_json(bytes)?;
    validate_request(&mut request)?;
    Ok(request)
}

pub(super) fn parse_environment(bytes: &[u8]) -> Result<Environment, StorageError> {
    let environment = decode_json(bytes)?;
    validate_environment(&environment)?;
    Ok(environment)
}

pub(super) fn validate_manifest(manifest: &Manifest) -> Result<(), StorageError> {
    if manifest.conductor_format != READABLE_FORMAT {
        return Err(invalid(format!(
            "unsupported conductorFormat {}",
            manifest.conductor_format
        )));
    }
    validate_identity(&manifest.id, &manifest.name)?;
    validate_shared(&manifest.values, &manifest.secrets)?;
    validate_auth(manifest.auth.as_ref())
        .map_err(|error| invalid(format!("Collection {}: {error}", manifest.name)))?;
    if manifest.folders.len() > MAX_FOLDERS {
        return Err(invalid("collection has too many folders"));
    }

    let mut parents = HashMap::with_capacity(manifest.folders.len());
    for folder in &manifest.folders {
        validate_identity(&folder.id, &folder.name)?;
        validate_position(folder.position)?;
        validate_auth(folder.auth.as_ref())
            .map_err(|error| invalid(format!("Folder {}: {error}", folder.name)))?;
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
        if let Some(reference) = header_credential(header) {
            if !reference.is_empty() && variable_reference(reference).is_none() {
                return Err(invalid(format!(
                    "{} must use a variable reference, for example {{{{token}}}}.",
                    header.key.trim()
                )));
            }
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
    validate_shared(&environment.values, &environment.secrets)
}

fn validate_shared(
    values: &BTreeMap<String, String>,
    secrets: &BTreeSet<String>,
) -> Result<(), StorageError> {
    if values
        .keys()
        .chain(secrets)
        .any(|key| !valid_variable_key(key))
    {
        return Err(invalid("variable keys must be nonempty and trimmed"));
    }
    if let Some(key) = secrets.iter().find(|key| values.contains_key(*key)) {
        return Err(invalid(format!(
            "the required secret {key} cannot also have a shared value"
        )));
    }
    Ok(())
}

pub(super) fn valid_variable_key(key: &str) -> bool {
    !key.is_empty() && key == key.trim() && !key.chars().any(char::is_control)
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
    for (field, value) in auth_credentials(auth) {
        if !value.is_empty() && variable_reference(value).is_none() {
            return Err(invalid(format!(
                "auth {field} must use a variable reference, for example {{{{{field}}}}}."
            )));
        }
    }
    Ok(())
}

pub(super) fn auth_credentials(auth: &AuthConfig) -> impl Iterator<Item = (&'static str, &str)> {
    [
        ("token", &auth.token),
        ("username", &auth.username),
        ("password", &auth.password),
        ("value", &auth.value),
    ]
    .into_iter()
    .filter_map(|(field, value)| Some((field, value.as_deref()?.trim())))
}

pub(super) fn header_credential(header: &KeyValue) -> Option<&str> {
    let key = header.key.trim();
    let value = header.value.trim();
    if key.eq_ignore_ascii_case("Authorization") || key.eq_ignore_ascii_case("Proxy-Authorization")
    {
        return Some(
            value
                .split_once(char::is_whitespace)
                .filter(|(scheme, _)| {
                    scheme.eq_ignore_ascii_case("Bearer") || scheme.eq_ignore_ascii_case("Basic")
                })
                .map_or(value, |(_, credential)| credential.trim()),
        );
    }
    [
        "Cookie",
        "Cookie2",
        "Set-Cookie",
        "Set-Cookie2",
        "API-Key",
        "X-API-Key",
        "X-API-Token",
        "X-Access-Token",
        "X-Auth-Token",
        "X-Auth-Key",
        "X-Session-Token",
        "X-Authorization",
        "X-CSRF-Token",
        "X-XSRF-Token",
        "X-Amz-Security-Token",
        "Private-Token",
        "Job-Token",
        "X-Shopify-Access-Token",
        "X-Goog-Api-Key",
        "Ocp-Apim-Subscription-Key",
        "Cf-Access-Client-Id",
        "Cf-Access-Client-Secret",
        "Cf-Access-Jwt-Assertion",
    ]
    .iter()
    .any(|name| name.eq_ignore_ascii_case(key))
    .then_some(value)
}

pub(super) fn variable_reference(value: &str) -> Option<&str> {
    let key = value.strip_prefix("{{")?.strip_suffix("}}")?.trim();
    (!key.contains(['{', '}']) && valid_variable_key(key)).then_some(key)
}

pub(super) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, StorageError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(super) fn slug(name: &str) -> String {
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
    if slug.is_empty() {
        "item".into()
    } else {
        slug.into()
    }
}

#[derive(Default)]
pub(super) struct Slugs {
    occupied: HashSet<String>,
    suffixes: HashMap<String, usize>,
}

impl Slugs {
    pub(super) fn reserve(&mut self, key: &str) {
        self.occupied.insert(key.to_ascii_lowercase());
    }

    pub(super) fn claim(&mut self, prefix: &str, name: &str) -> String {
        let slug = slug(name);
        let base = format!("{prefix}{slug}");
        if (prefix.is_empty() || !reserved_device_name(&slug)) && self.occupied.insert(base.clone())
        {
            return base;
        }
        self.claim_suffix(base)
    }

    pub(super) fn keep_key(&mut self, key: &str) -> String {
        if self.occupied.insert(key.to_owned()) {
            return key.to_owned();
        }
        self.claim_suffix(key.to_owned())
    }

    fn claim_suffix(&mut self, base: String) -> String {
        let number = self.suffixes.entry(base.clone()).or_insert(2);
        loop {
            let candidate = format!("{base}-{number}");
            *number += 1;
            if self.occupied.insert(candidate.clone()) {
                return candidate;
            }
        }
    }

    pub(super) fn file(&mut self, directory: &str, name: &str) -> String {
        format!("{}.json", self.claim(&format!("{directory}/"), name))
    }
}

pub(super) fn can_nest_in(directory: &str) -> bool {
    // Leave room for a 48-byte slug, a collision suffix, and the extension.
    directory.len() <= MAX_READABLE_PATH - 64 && directory.split('/').all(portable_stem)
}

pub(super) fn validate_readable_path(path: &str) -> Result<(), StorageError> {
    if path.len() > MAX_READABLE_PATH
        || !path
            .strip_suffix(".json")
            .is_some_and(|stem| stem.split('/').all(portable_stem))
    {
        return Err(invalid(format!(
            "{path} must use a short lowercase slug for each directory and JSON filename"
        )));
    }
    Ok(())
}

pub(super) fn list(root: &Path, subdir: &str) -> Result<Vec<String>, StorageError> {
    let Some(path) = directory(root, subdir, false)? else {
        return Ok(Vec::new());
    };
    let mut paths = Vec::new();
    list_files(&path, subdir, 0, &mut 0, &mut paths)?;
    paths.sort_unstable();
    Ok(paths)
}

fn list_files(
    path: &Path,
    relative: &str,
    depth: usize,
    directories: &mut usize,
    paths: &mut Vec<String>,
) -> Result<(), StorageError> {
    for entry in fs::read_dir(path).map_err(|error| io_error(path, error))? {
        let entry = entry.map_err(|error| io_error(path, error))?;
        if let Some(name) = entry.file_name().to_str() {
            if name.ends_with(".json")
                || (Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("json")))
            {
                paths.push(format!("{relative}/{name}"));
                if paths.len() > MAX_FOLDERS + 2 {
                    return Err(invalid("repository has too many JSON files"));
                }
            } else if relative == "requests" || relative.starts_with("requests/") {
                let child = entry.path();
                let Some(meta) = metadata(&child)? else {
                    continue;
                };
                if is_link(&meta) || (meta.is_dir() && !portable_stem(name)) {
                    paths.push(format!("{relative}/{name}"));
                    continue;
                }
                if meta.is_dir() {
                    *directories += 1;
                    if depth >= MAX_FOLDER_DEPTH - 1 || *directories > MAX_FOLDERS {
                        return Err(invalid("too many nested request directories"));
                    }
                    list_files(
                        &child,
                        &format!("{relative}/{name}"),
                        depth + 1,
                        directories,
                        paths,
                    )?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    pub(super) static FILE_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
    #[cfg(test)]
    FILE_READS.with(|reads| reads.set(reads.get() + 1));
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
    if next == expected {
        return check();
    }
    let path = file_path(root, relative, next.is_some())?
        .ok_or_else(|| invalid(format!("the directory for {relative} is missing")))?;
    let Some(bytes) = next else {
        check()?;
        return fs::remove_file(&path).map_err(|error| io_error(&path, error));
    };
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(invalid(format!("{relative} is larger than 16 MiB")));
    }

    let local = relative == super::values::LOCAL;
    let prefix = if local { ".local.json-" } else { ".conductor-" };
    let temporary_name = format!("{prefix}{}.tmp", Uuid::new_v4());
    let parent = path.parent().expect("repository files have a parent");
    let directories = if local {
        super::values::check_private(root, &[relative, temporary_name.as_str()])?;
        super::values::staging_directories(root)?
    } else {
        vec![parent.to_path_buf()]
    };
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if local {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut last_error = None;
    for directory in directories {
        let temporary = directory.join(&temporary_name);
        let mut file = match options.open(&temporary) {
            Ok(file) => file,
            Err(error) => {
                last_error = Some(io_error(&temporary, error));
                continue;
            }
        };
        let written = file.write_all(bytes).and_then(|()| file.sync_all());
        drop(file);
        let result = (|| {
            written.map_err(|error| io_error(&temporary, error))?;
            if local {
                super::values::check_private(root, &[relative, temporary_name.as_str()])?;
            }
            check()?;
            // The first install must not overwrite a file created after check.
            let installed = if expected.is_none() {
                fs::hard_link(&temporary, &path)
            } else {
                fs::rename(&temporary, &path)
            };
            match installed {
                Err(error) if local && error.kind() == io::ErrorKind::CrossesDevices => {
                    Ok(Some(io_error(&path, error)))
                }
                result => result
                    .map(|()| None)
                    .map_err(|error| io_error(&path, error)),
            }
        })();
        let _ = fs::remove_file(temporary);
        if let Some(cross_device) = result? {
            last_error = Some(cross_device);
        } else {
            return Ok(());
        }
    }
    Err(if local {
        invalid(format!(
            "Cannot atomically save private values from outside the working tree.{}",
            last_error.map_or_else(String::new, |error| format!(" {error}"))
        ))
    } else {
        last_error.expect("shared files stage in their own directory")
    })
}

pub(super) fn backup_group(
    root: &Path,
    relative: &str,
) -> Result<(PathBuf, PathBuf), StorageError> {
    let path = file_path(root, relative, false)?
        .ok_or_else(|| invalid(format!("the directory for {relative} is missing")))?;
    let name = path
        .file_name()
        .expect("group path has a name")
        .to_string_lossy();
    let backup = path.with_file_name(format!(".{name}.conductor-deleted-{}.tmp", Uuid::new_v4()));
    fs::hard_link(&path, &backup).map_err(|error| io_error(&path, error))?;
    Ok((path, backup))
}

pub(super) fn sync_directories<'a>(
    root: &Path,
    paths: impl IntoIterator<Item = &'a str>,
) -> Result<(), StorageError> {
    #[cfg(unix)]
    {
        let mut synced = HashSet::new();
        for path in paths {
            for directory in root.join(".conductor").join(path).ancestors().skip(1) {
                if !synced.insert(directory.to_owned()) {
                    break;
                }
                File::open(directory)
                    .and_then(|file| file.sync_all())
                    .map_err(|error| io_error(directory, error))?;
                if directory == root {
                    break;
                }
            }
        }
    }
    #[cfg(not(unix))]
    let _ = (root, paths);
    Ok(())
}

fn file_path(root: &Path, relative: &str, create: bool) -> Result<Option<PathBuf>, StorageError> {
    if matches!(
        relative,
        "collection.json" | super::values::LOCAL | super::values::IGNORE
    ) {
        return Ok(real_directory(root.join(".conductor"), false)?.map(|path| path.join(relative)));
    }
    let (subdir, remainder) = relative
        .split_once('/')
        .ok_or_else(|| invalid("invalid repository file path"))?;
    let (nested, name) = remainder.rsplit_once('/').unwrap_or(("", remainder));
    if !name.ends_with(".json")
        || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        || name.chars().any(char::is_control)
        || reserved_device_name(name)
    {
        return Err(invalid("invalid repository filename"));
    }
    let parents = nested.split('/').filter(|_| !nested.is_empty());
    if (remainder.contains('/') && (nested.is_empty() || subdir != "requests"))
        || parents.clone().count() >= MAX_FOLDER_DEPTH
        || !parents.clone().all(portable_stem)
    {
        return Err(invalid("invalid request directory"));
    }
    let Some(mut path) = directory(root, subdir, create)? else {
        return Ok(None);
    };
    for parent in parents {
        let Some(child) = real_directory(path.join(parent), create)? else {
            return Ok(None);
        };
        path = child;
    }
    Ok(Some(path.join(name)))
}

pub(super) fn slug_key(key: &str) -> bool {
    key.split('-').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    })
}

fn portable_stem(name: &str) -> bool {
    slug_key(name) && !reserved_device_name(name)
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

pub(super) fn real_directory(path: PathBuf, create: bool) -> Result<Option<PathBuf>, StorageError> {
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
        let mut request = request();
        let auth = serde_json::json!({"authType": "bearer", "token": "{{token}}"});
        request.auth = Some(serde_json::from_value(auth.clone()).unwrap());
        let bytes = encode(&request).unwrap();

        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap()["auth"],
            auth
        );
        assert_eq!(request.name, "Health check");
        assert_eq!(request.url, "{{baseUrl}}/health");
        assert!(bytes.ends_with(b"\n"));
        assert_eq!(encode(&parse_request(&bytes).unwrap()).unwrap(), bytes);
    }

    #[test]
    fn environment_values_and_secret_keys_are_sorted_for_stable_diffs() {
        let input = format!(
            r#"{{"name":"Development","id":"{REQUEST_ID}","values":{{"z":"last","a":"first"}},"secrets":["y-secret","b-secret"]}}"#
        );
        let environment = parse_environment(input.as_bytes()).unwrap();
        let bytes = encode(&environment).unwrap();
        let json = std::str::from_utf8(&bytes).unwrap();

        assert!(json.find("\"a\"").unwrap() < json.find("\"z\"").unwrap());
        assert!(json.find("b-secret").unwrap() < json.find("y-secret").unwrap());
        assert_eq!(encode(&parse_environment(&bytes).unwrap()).unwrap(), bytes);
    }

    #[test]
    fn manifest_rejects_bad_folder_graphs_and_future_versions() {
        let input = format!(
            r#"{{"conductorFormat":2,"id":"{REQUEST_ID}","name":"API","folders":[
                {{"id":"{FOLDER_ID}","name":"Nested","parentId":"{FOLDER_ID}"}}]}}"#
        );
        let error = parse_manifest(input.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("cycle"));
        let input = input.replace("conductorFormat\":2", "conductorFormat\":3");
        let error = parse_manifest(input.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("unsupported conductorFormat 3"));
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
    fn cookie_and_vendor_credential_headers_require_variable_references() {
        let mut request = request();
        for key in ["Cookie", "cOoKiE", "X-Goog-Api-Key", "X-Amz-Security-Token"] {
            request.headers = vec![KeyValue {
                key: key.into(),
                value: "session=fixture-token".into(),
                enabled: false,
            }];
            assert!(validate_request(&mut request)
                .unwrap_err()
                .to_string()
                .contains(key));
            request.headers[0].value = "{{credential}}".into();
            validate_request(&mut request).unwrap();
        }
    }

    #[test]
    fn repository_auth_reports_the_expected_field_for_each_auth_type() {
        let mut value = serde_json::to_value(request()).unwrap();
        for (auth, expected) in [
            (
                serde_json::json!({"authType":"bearer","value":"{{token}}"}),
                "token",
            ),
            (
                serde_json::json!({"authType":"basic","token":"{{token}}"}),
                "username",
            ),
            (
                serde_json::json!({"authType":"apikey","token":"{{token}}"}),
                "value",
            ),
        ] {
            value["auth"] = auth;
            let error = parse_request(&serde_json::to_vec(&value).unwrap())
                .unwrap_err()
                .to_string();
            assert!(error.contains("unknown field"), "{error}");
            assert!(
                error.split_once("expected").unwrap().1.contains(expected),
                "{error}"
            );
        }
    }

    #[test]
    fn repository_requests_report_duplicate_keys_inside_scripts() {
        let mut bytes = encode(&request()).unwrap();
        let object_end = bytes.iter().rposition(|byte| *byte == b'}').unwrap();
        bytes.splice(
            object_end..object_end,
            br#", "testScript": [{"nested":{"item":1,"it\u0065m":2}}]"#
                .iter()
                .copied(),
        );
        let error = parse_request(&bytes).unwrap_err().to_string();
        assert!(error.contains(r#"duplicate JSON key "item""#), "{error}");
    }

    #[test]
    fn repository_auth_and_headers_report_unknown_fields() {
        let request = request();
        let mut value = serde_json::to_value(request).unwrap();
        value["auth"] = serde_json::json!({"authType":"bearer","tokne":"{{token}}"});
        let parse = |value: &Value| parse_request(&serde_json::to_vec(value).unwrap());
        assert!(parse(&value).unwrap_err().to_string().contains("tokne"));
        value["auth"] = serde_json::json!({"authType":"beare","token":"{{token}}"});
        assert!(parse(&value).unwrap_err().to_string().contains("beare"));
        value["auth"] = serde_json::json!({"authType":"apikey","key":"key","value":"{{token}}","addTo":"qurey"});
        assert!(parse(&value).unwrap_err().to_string().contains("qurey"));
        value["auth"] = serde_json::json!({"authType":"unsupported"});
        assert!(parse(&value).is_ok());
        value["auth"] = serde_json::json!({"authType":"bearer","token":"{{token}}"});
        value["headers"] =
            serde_json::json!([{"key":"Accept","value":"application/json","enabld":true}]);
        assert!(parse(&value).unwrap_err().to_string().contains("enabld"));
        value["headers"] = serde_json::json!([{"key":"Accept","value":"application/json"}]);
        assert!(parse(&value).unwrap().headers[0].enabled);
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
        let relative = Slugs::default().file("requests", "../../CON | Health ☀");

        assert_eq!(relative, "requests/con-health.json");
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
    fn readable_groups_reserve_root_and_report_nonportable_directories() {
        let Repository(root) = &Repository::new();
        let mut slugs = Slugs::default();
        slugs.reserve("requests/root");
        assert_eq!(slugs.file("requests", "ROOT"), "requests/root-2.json");
        assert_eq!(slugs.file("requests", "CON"), "requests/con-2.json");
        assert_eq!(slugs.file("requests", "Lpt3"), "requests/lpt3-2.json");

        let path = "requests/users/admin.json";
        replace(root, path, None, Some(b"{}")).unwrap();
        assert_eq!(list(root, "requests").unwrap(), [path]);
        let requests = root.join(".conductor/requests");
        fs::rename(requests.join("users"), requests.join("renaming")).unwrap();
        fs::rename(requests.join("renaming"), requests.join("Users")).unwrap();
        let paths = list(root, "requests").unwrap();
        assert_eq!(paths, ["requests/Users"]);
        assert!(validate_readable_path(&paths[0])
            .unwrap_err()
            .to_string()
            .contains("requests/Users"));
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
