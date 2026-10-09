use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::stored_enum::stored_enum;

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentSummary {
    pub id: String,
    /// The collection that owns this environment.
    pub collection_id: String,
    pub name: String,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CollectionNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub position: i64,
    pub name: String,
    #[serde(flatten)]
    pub kind: CollectionNodeKind,
}

/// Only a folder has children and only a request has a request behind it, so
/// neither side needs a null for the other's fields.
#[derive(Debug, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CollectionNodeKind {
    Folder {
        children: Vec<CollectionNode>,
    },
    #[serde(rename_all = "camelCase")]
    Request {
        request_id: String,
        method: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KeyValue {
    pub key: String,
    pub value: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BodyField {
    pub key: String,
    pub value: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub field_type: BodyFieldType,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RequestBody {
    pub mode: BodyMode,
    #[serde(default)]
    pub raw: String,
    #[serde(default)]
    pub raw_language: Option<String>,
    #[serde(default)]
    pub form_data: Vec<BodyField>,
    #[serde(default)]
    pub urlencoded: Vec<KeyValue>,
    #[serde(default)]
    pub graphql: Option<GraphqlBody>,
    #[serde(default)]
    pub file: Option<FileBody>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlBody {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub variables: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FileBody {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
}

stored_enum! {
    /// The strings are what `body_json` holds, including rows saved before this
    /// was an enum.
    pub enum BodyMode, fallback Unsupported {
        None,
        Raw,
        FormData,
        UrlEncoded,
        Graphql,
        File,
        /// A Postman mode we cannot send (such as `binary`); it sends no body.
        Unsupported,
    }
}

stored_enum! {
    #[derive(Default)]
    pub enum BodyFieldType, fallback Text {
        File,
        /// Also what a missing or unknown stored type reads as.
        #[default]
        Text,
    }
}

stored_enum! {
    /// The strings are what `auth_json` holds, which Postman import also writes.
    pub enum AuthType, fallback Unsupported {
        /// An explicit "send none", unlike no auth at all, which inherits.
        NoAuth,
        Bearer,
        Basic,
        ApiKey,
        /// An imported type we cannot apply (such as `oauth2`); it adds nothing
        /// to the request but still stops inheritance.
        Unsupported,
    }
}

stored_enum! {
    pub enum ApiKeyLocation, fallback Header {
        Query,
        /// Also what any other stored value reads as, matching how it is sent.
        Header,
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    pub auth_type: AuthType,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub add_to: Option<ApiKeyLocation>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RequestDetail {
    pub id: String,
    pub collection_id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: Vec<KeyValue>,
    pub query: Vec<KeyValue>,
    pub path_params: Vec<KeyValue>,
    pub auth: Option<AuthConfig>,
    pub body: Option<RequestBody>,
    #[specta(type = Option<specta_typescript::Unknown>)]
    pub pre_request_script: Option<Value>,
    #[specta(type = Option<specta_typescript::Unknown>)]
    pub test_script: Option<Value>,
}

/// Which set of variables a command reads or writes. The ids it holds are the
/// only way to name a collection or environment, so a scope without its id
/// cannot be expressed.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(tag = "scope", rename_all = "lowercase")]
pub enum VariableTarget {
    Global,
    #[serde(rename_all = "camelCase")]
    Collection {
        collection_id: String,
    },
    #[serde(rename_all = "camelCase")]
    Environment {
        environment_id: String,
    },
}

impl VariableTarget {
    /// The `variables` columns that identify this target's rows.
    pub fn columns(&self) -> (&'static str, Option<&str>, Option<&str>) {
        match self {
            Self::Global => ("global", None, None),
            Self::Collection { collection_id } => ("collection", Some(collection_id), None),
            Self::Environment { environment_id } => ("environment", None, Some(environment_id)),
        }
    }
}

/// A variable as the editors show it. Which target it belongs to is the
/// target it was listed from or is saved to.
#[derive(Debug, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VariableEntry {
    pub key: String,
    pub value: String,
    pub enabled: bool,
    pub sensitive: bool,
}

/// What one editor changed in one target: keys to set and keys to remove.
/// Keys it did not touch are not sent, so saving never overwrites a value
/// changed elsewhere (a script, another window) in the meantime.
#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VariableChange {
    pub target: VariableTarget,
    pub upserts: Vec<VariableEntry>,
    pub deletes: Vec<String>,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedRequestPreview {
    /// From the request's folders and collection in the database, so it
    /// follows moves without reloading the request.
    pub inherited_auth: Option<AuthConfig>,
    /// Keys only, in the order they first appear.
    pub unresolved_variables: Vec<String>,
    /// Every variable the request uses that has a value. `None` for a
    /// sensitive variable, whose value stays in Rust.
    pub variable_values: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SendRequestInput {
    pub request: RequestDetail,
    #[serde(default)]
    pub environment_id: Option<String>,
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateEnvironmentInput {
    pub collection_id: String,
    pub name: String,
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateEnvironmentInput {
    pub environment_id: String,
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RenameEnvironmentInput {
    pub environment_id: String,
    pub name: String,
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequestInput {
    pub collection_id: String,
    pub parent_id: Option<String>,
    pub position: i64,
    pub name: String,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequestResult {
    pub request_id: String,
    pub node_id: String,
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateFolderInput {
    pub collection_id: String,
    pub parent_id: Option<String>,
    pub position: i64,
    pub name: String,
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateRequestInput {
    pub request_id: String,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateRequestResult {
    pub request_id: String,
    pub node_id: String,
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MoveNodeInput {
    pub node_id: String,
    pub parent_id: Option<String>,
    pub position: i64,
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveTextFileInput {
    pub path: String,
    pub contents: String,
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SaveResponseBodyInput {
    pub history_id: String,
    pub path: String,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResponseHeader {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SendRequestResult {
    pub history_id: String,
    pub status_code: u16,
    pub duration_ms: u128,
    pub headers: Vec<ResponseHeader>,
    /// Pretty-printed for JSON, as received for text, empty for binary.
    pub body: String,
    pub body_bytes: usize,
    pub body_content_type: Option<String>,
    pub body_format: ResponseBodyFormat,
    /// What the save dialog suggests, from `Content-Disposition` when sent.
    pub download_file_name: String,
    pub updated_variables: Vec<KeyValue>,
    pub variable_warnings: Vec<String>,
}

/// Binary bodies stay as raw bytes in Rust; see `save_response_body`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ResponseBodyFormat {
    Json,
    Text,
    Binary,
}

fn enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies_and_auth_stored_before_the_enums_still_load() {
        let body: RequestBody = serde_json::from_str(
            r#"{"mode":"formdata","raw":"","rawLanguage":null,"formData":[
                {"key":"avatar","value":"","enabled":true,"fieldType":"file","filePath":null,"contentType":null},
                {"key":"name","value":"x","enabled":true,"fieldType":""},
                {"key":"legacy","value":"y"}
            ],"urlencoded":[],"graphql":null,"file":null}"#,
        )
        .unwrap();
        assert_eq!(body.mode, BodyMode::FormData);
        let types: Vec<_> = body
            .form_data
            .iter()
            .map(|field| field.field_type)
            .collect();
        assert_eq!(
            types,
            [
                BodyFieldType::File,
                BodyFieldType::Text,
                BodyFieldType::Text
            ]
        );

        let auth: AuthConfig =
            serde_json::from_str(r#"{"authType":"apikey","key":"k","value":"v","addTo":"query"}"#)
                .unwrap();
        assert_eq!(auth.auth_type, AuthType::ApiKey);
        assert_eq!(auth.add_to, Some(ApiKeyLocation::Query));
    }

    #[test]
    fn imported_modes_and_auth_types_we_cannot_apply_load_instead_of_failing_the_request() {
        let body: RequestBody = serde_json::from_str(r#"{"mode":"binary"}"#).unwrap();
        assert_eq!(body.mode, BodyMode::Unsupported);

        let auth: AuthConfig =
            serde_json::from_str(r#"{"authType":"oauth2","addTo":"cookie"}"#).unwrap();
        assert_eq!(auth.auth_type, AuthType::Unsupported);
        assert_eq!(auth.add_to, Some(ApiKeyLocation::Header));
    }

    #[test]
    fn enums_are_stored_as_the_strings_older_rows_hold() {
        let modes = [
            BodyMode::None,
            BodyMode::Raw,
            BodyMode::FormData,
            BodyMode::UrlEncoded,
            BodyMode::Graphql,
            BodyMode::File,
        ];
        let auth_types = [
            AuthType::NoAuth,
            AuthType::Bearer,
            AuthType::Basic,
            AuthType::ApiKey,
        ];

        assert_eq!(
            serde_json::to_string(&modes).unwrap(),
            r#"["none","raw","formdata","urlencoded","graphql","file"]"#
        );
        assert_eq!(
            serde_json::to_string(&auth_types).unwrap(),
            r#"["noauth","bearer","basic","apikey"]"#
        );
        assert_eq!(
            serde_json::to_string(&[ApiKeyLocation::Header, ApiKeyLocation::Query]).unwrap(),
            r#"["header","query"]"#
        );
        assert_eq!(
            serde_json::to_string(&[BodyFieldType::Text, BodyFieldType::File]).unwrap(),
            r#"["text","file"]"#
        );
    }
}
