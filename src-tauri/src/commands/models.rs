use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSummary {
    pub id: String,
    pub name: String,
    pub source: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentSummary {
    pub id: String,
    pub name: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionNode {
    pub id: String,
    pub collection_id: String,
    pub parent_id: Option<String>,
    pub position: i64,
    pub kind: String,
    pub name: String,
    pub request_id: Option<String>,
    pub method: Option<String>,
    pub children: Vec<CollectionNode>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KeyValue {
    pub key: String,
    pub value: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BodyField {
    pub key: String,
    pub value: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub field_type: String,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RequestBody {
    pub mode: String,
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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct GraphqlBody {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub variables: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct FileBody {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RequestBodyMode {
    None,
    Raw,
    FormData,
    UrlEncoded,
    Graphql,
    File,
    Unsupported,
}

impl RequestBody {
    pub fn body_mode(&self) -> RequestBodyMode {
        match self.mode.as_str() {
            "none" => RequestBodyMode::None,
            "raw" => RequestBodyMode::Raw,
            "formdata" => RequestBodyMode::FormData,
            "urlencoded" => RequestBodyMode::UrlEncoded,
            "graphql" => RequestBodyMode::Graphql,
            "file" => RequestBodyMode::File,
            _ => RequestBodyMode::Unsupported,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    pub auth_type: String,
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
    pub add_to: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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
    pub inherited_auth: Option<AuthConfig>,
    pub effective_auth: Option<AuthConfig>,
    pub body: Option<RequestBody>,
    pub pre_request_script: Option<Value>,
    pub test_script: Option<Value>,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableEntry {
    pub scope: String,
    pub collection_id: Option<String>,
    pub environment_id: Option<String>,
    pub key: String,
    pub value: String,
    pub initial_value: Option<String>,
    pub enabled: bool,
    pub sensitive: bool,
    #[serde(default)]
    pub variable_type: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedVariable {
    pub key: String,
    pub locations: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedRequestPreview {
    pub url: String,
    pub headers: Vec<KeyValue>,
    pub query: Vec<KeyValue>,
    pub body: Option<RequestBody>,
    pub unresolved_variables: Vec<UnresolvedVariable>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRequestInput {
    pub request: RequestDetail,
    #[serde(default)]
    pub environment_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEnvironmentInput {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameEnvironmentInput {
    pub environment_id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequestInput {
    pub collection_id: String,
    pub parent_id: Option<String>,
    pub position: i64,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequestResult {
    pub request_id: String,
    pub node_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFolderInput {
    pub collection_id: String,
    pub parent_id: Option<String>,
    pub position: i64,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateRequestInput {
    pub request_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateRequestResult {
    pub request_id: String,
    pub node_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveNodeInput {
    pub node_id: String,
    pub parent_id: Option<String>,
    pub position: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTextFileInput {
    pub path: String,
    pub contents: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseHeader {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRequestResult {
    pub history_id: String,
    pub status_code: u16,
    pub status_text: String,
    pub duration_ms: u128,
    pub headers: Vec<ResponseHeader>,
    pub body: String,
    pub body_bytes: usize,
    pub body_content_type: Option<String>,
    pub body_format: String,
    pub body_truncated: bool,
    pub updated_variables: Vec<KeyValue>,
    pub variable_warnings: Vec<String>,
    pub unresolved_variables: Vec<UnresolvedVariable>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestHistoryEntry {
    pub id: String,
    pub request_id: Option<String>,
    pub collection_id: Option<String>,
    /// Convenience alias; kept `null` for now because history rows do not
    /// snapshot the display name and the frontend can look it up on demand.
    pub name: Option<String>,
    pub method: String,
    pub url: String,
    pub status_code: Option<i64>,
    pub status_text: Option<String>,
    pub duration_ms: Option<i64>,
    pub body_bytes: Option<i64>,
    pub body_content_type: Option<String>,
    pub error: Option<String>,
    /// `created_at` mirrors the DB column. Also exposed as `executed_at` to
    /// match the frontend contract.
    pub executed_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListRequestHistoryInput {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub collection_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameCollectionInput {
    pub collection_id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestHistoryDetail {
    pub id: String,
    pub request_id: Option<String>,
    pub collection_id: Option<String>,
    pub method: String,
    pub url: String,
    pub status_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub created_at: String,
    /// Redacted request snapshot (secrets replaced with `[REDACTED]`).
    pub request_json: Option<Value>,
    /// Response metadata: headers, contentType, bodyBytes, bodyFormat, bodyTruncated.
    pub response_meta_json: Option<Value>,
}

fn enabled() -> bool {
    true
}
