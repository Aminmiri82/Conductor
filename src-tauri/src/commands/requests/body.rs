use reqwest::header::{HeaderValue, CONTENT_TYPE};
use serde_json::{json, Value};
use tokio_util::io::ReaderStream;

use crate::commands::{
    error::CommandError,
    models::{BodyFieldType, BodyMode, GraphqlBody, RequestBody},
};

pub(super) async fn apply_body(
    mut builder: reqwest::RequestBuilder,
    body: &RequestBody,
) -> Result<reqwest::RequestBuilder, CommandError> {
    match body.mode {
        BodyMode::Raw => {
            builder = builder.body(body.raw.clone());
        }
        BodyMode::UrlEncoded => {
            let pairs = body
                .urlencoded
                .iter()
                .filter(|field| field.enabled && !field.key.is_empty())
                .map(|field| (field.key.clone(), field.value.clone()))
                .collect::<Vec<_>>();
            builder = builder.form(&pairs);
        }
        BodyMode::FormData => {
            let mut form = reqwest::multipart::Form::new();
            for field in body
                .form_data
                .iter()
                .filter(|field| field.enabled && !field.key.is_empty())
            {
                if field.field_type == BodyFieldType::File {
                    if let Some(path) = field.file_path.as_ref().filter(|path| !path.is_empty()) {
                        let file_name = std::path::Path::new(path)
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("upload")
                            .to_string();
                        let part = reqwest::multipart::Part::file(path)
                            .await?
                            .file_name(file_name);
                        let part = if let Some(content_type) = field
                            .content_type
                            .as_ref()
                            .filter(|value| !value.is_empty())
                        {
                            part.mime_str(content_type)?
                        } else {
                            part
                        };
                        form = form.part(field.key.clone(), part);
                    }
                } else {
                    form = form.text(field.key.clone(), field.value.clone());
                }
            }
            builder = builder.multipart(form);
        }
        BodyMode::Graphql => {
            builder = builder.json(&graphql_payload(body.graphql.as_ref())?);
        }
        BodyMode::File => {
            let file = body.file.as_ref().ok_or("no binary file selected")?;
            let path = file
                .path
                .as_ref()
                .filter(|path| !path.is_empty())
                .ok_or("no binary file selected")?;
            let stream = ReaderStream::new(tokio::fs::File::open(path).await?);
            builder = builder.body(reqwest::Body::wrap_stream(stream));
        }
        _ => {}
    }
    Ok(builder)
}
pub(super) fn default_content_type(body: &RequestBody) -> Option<HeaderValue> {
    match body.mode {
        BodyMode::Raw if body.raw_language.as_deref() == Some("json") => {
            Some(HeaderValue::from_static("application/json"))
        }
        BodyMode::Graphql => Some(HeaderValue::from_static("application/json")),
        BodyMode::File => body
            .file
            .as_ref()
            .and_then(|file| file.content_type.as_ref())
            .filter(|content_type| !content_type.is_empty())
            .and_then(|content_type| HeaderValue::from_str(content_type).ok()),
        _ => None,
    }
}
pub(super) fn graphql_payload(graphql: Option<&GraphqlBody>) -> Result<Value, String> {
    let query = graphql
        .map(|graphql| graphql.query.as_str())
        .unwrap_or_default();
    let variables = graphql
        .map(|graphql| graphql.variables.trim())
        .filter(|variables| !variables.is_empty())
        .map(serde_json::from_str::<Value>)
        .transpose()
        .map_err(|error| format!("invalid GraphQL variables JSON: {error}"))?
        .unwrap_or_else(|| json!({}));

    if !variables.is_object() {
        return Err("GraphQL variables must be a JSON object".to_string());
    }

    Ok(json!({
        "query": query,
        "variables": variables
    }))
}
/// Chosen file paths belong to this machine and this session, so they are
/// never stored; the user picks the file again after a restart.
pub(super) fn strip_file_paths(body: &mut RequestBody) {
    for field in &mut body.form_data {
        if field.field_type == BodyFieldType::File {
            field.file_path = None;
        }
    }
    if let Some(file) = &mut body.file {
        file.path = None;
    }
}

pub(super) fn insert_default_content_type(
    headers: &mut reqwest::header::HeaderMap,
    body: &RequestBody,
) {
    if !headers.contains_key(CONTENT_TYPE) {
        if let Some(content_type) = default_content_type(body) {
            headers.insert(CONTENT_TYPE, content_type);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphql_payload_uses_empty_variables_object_by_default() {
        let payload = graphql_payload(Some(&GraphqlBody {
            query: "query Viewer { viewer { id } }".to_string(),
            variables: String::new(),
        }))
        .expect("payload should be valid");

        assert_eq!(payload["query"], "query Viewer { viewer { id } }");
        assert_eq!(payload["variables"], json!({}));
    }

    #[test]
    fn graphql_payload_rejects_non_object_variables() {
        let error = graphql_payload(Some(&GraphqlBody {
            query: String::new(),
            variables: "[]".to_string(),
        }))
        .expect_err("variables arrays should be rejected");

        assert_eq!(error, "GraphQL variables must be a JSON object");
    }
}
