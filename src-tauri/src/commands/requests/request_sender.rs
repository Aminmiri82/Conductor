use std::time::Instant;

use chrono::Utc;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE},
    Method, Url,
};
use rusqlite::params;
use serde_json::{json, Value};
use tauri::State;
use uuid::Uuid;

use crate::commands::models::{
    RequestDetail, ResolvedRequestPreview, ResponseHeader, SendRequestInput, SendRequestResult,
};
use crate::AppState;

use super::variables::load_variable_context;
use super::{postman_scripts, request_auth, request_body, variable_resolver};

#[tauri::command]
pub fn resolve_request(
    request: RequestDetail,
    state: State<'_, AppState>,
) -> Result<ResolvedRequestPreview, String> {
    state
        .database
        .with_read_connection(|connection| {
            let variables = load_variable_context(connection, &request)?;
            Ok(variable_resolver::resolve_request_with_context(
                &request, &variables,
            ))
        })
        .map_err(|error| error.to_string())
}
#[tauri::command]
pub async fn send_request(
    input: SendRequestInput,
    state: State<'_, AppState>,
) -> Result<SendRequestResult, String> {
    let variables = state
        .database
        .with_read_connection(|connection| load_variable_context(connection, &input.request))
        .map_err(|error| error.to_string())?;
    let preview = variable_resolver::resolve_request_with_context(&input.request, &variables);
    if !preview.unresolved_variables.is_empty() {
        return Err("request has unresolved variables".to_string());
    }

    let query_pairs = preview
        .query
        .iter()
        .filter(|query| query.enabled && !query.key.is_empty())
        .map(|query| (query.key.clone(), query.value.clone()))
        .collect::<Vec<_>>();
    let request_url = if preview.query.is_empty() {
        preview.url.clone()
    } else {
        url_without_query(&preview.url)?
    };

    let method = Method::from_bytes(input.request.method.as_bytes()).map_err(|e| e.to_string())?;
    let mut builder = state.http_client.request(method, request_url);
    let mut headers = HeaderMap::new();
    if !query_pairs.is_empty() {
        builder = builder.query(&query_pairs);
    }

    for header in preview.headers.iter().filter(|header| header.enabled) {
        if header.key.trim().is_empty() {
            continue;
        }
        let name = HeaderName::from_bytes(header.key.as_bytes()).map_err(|e| e.to_string())?;
        let value = HeaderValue::from_str(&header.value).map_err(|e| e.to_string())?;
        headers.insert(name, value);
    }
    if let Some(body) = preview.body.as_ref() {
        if body.mode == "raw"
            && body.raw_language.as_deref() == Some("json")
            && !headers.contains_key(CONTENT_TYPE)
        {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        }
    }
    builder = builder.headers(headers);

    let auth_to_apply = request_auth::effective_auth(
        input.request.auth.as_ref(),
        input.request.inherited_auth.as_ref(),
    );
    builder = request_auth::apply_auth(builder, auth_to_apply.as_ref(), &variables)?;

    if let Some(body) = preview.body.as_ref() {
        builder = request_body::apply_body(builder, body).await?;
    }

    let started = Instant::now();
    let response = builder.send().await.map_err(|error| error.to_string())?;
    let duration_ms = started.elapsed().as_millis();
    let status = response.status();
    let response_headers = response.headers().clone();
    let content_type = response_headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string);
    let headers = response_headers
        .iter()
        .map(|(key, value)| ResponseHeader {
            key: key.to_string(),
            value: value.to_str().unwrap_or_default().to_string(),
        })
        .collect::<Vec<_>>();
    let body_text = response.text().await.map_err(|error| error.to_string())?;
    let body_bytes = body_text.len();
    let body_json = serde_json::from_str::<Value>(&body_text).ok();
    let (body, body_format) = format_response_body(&body_text, body_json.as_ref());
    let history_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let updated_variables = postman_scripts::collect_postman_script_variables(
        input.request.test_script.as_ref(),
        body_json.as_ref(),
    );

    state
        .database
        .with_connection(|connection| {
            for variable in &updated_variables {
                connection.execute(
                    "INSERT OR REPLACE INTO variables
                     (scope_kind, scope_id, key, value, enabled, sensitive, created_at, updated_at)
                     VALUES ('collection', ?, ?, ?, 1, 0, ?, ?)",
                    params![
                        input.request.collection_id,
                        variable.key,
                        variable.value,
                        now,
                        now
                    ],
                )?;
            }

            connection.execute(
                "INSERT INTO request_history
                 (id, request_id, collection_id, method, url, status_code, duration_ms, request_json,
                  response_meta_json, response_body_path, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?)",
                params![
                    history_id,
                    input.request.id,
                    input.request.collection_id,
                    input.request.method,
                    preview.url,
                    status.as_u16() as i64,
                    duration_ms as i64,
                    json!(input.request).to_string(),
                    json!({
                        "headers": headers,
                        "contentType": content_type,
                        "bodyBytes": body_bytes,
                        "bodyFormat": body_format
                    })
                    .to_string(),
                    now
                ],
            )?;
            Ok(())
        })
        .map_err(|error| error.to_string())?;

    Ok(SendRequestResult {
        history_id,
        status_code: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or_default().to_string(),
        duration_ms,
        headers,
        body,
        body_bytes,
        body_content_type: content_type,
        body_format,
        updated_variables,
        unresolved_variables: preview.unresolved_variables,
    })
}
fn format_response_body(body_text: &str, body_json: Option<&Value>) -> (String, String) {
    if let Some(json) = body_json {
        let pretty = serde_json::to_string_pretty(json).unwrap_or_else(|_| body_text.to_string());
        return (pretty, "json".to_string());
    }
    (body_text.to_string(), "text".to_string())
}
fn url_without_query(url: &str) -> Result<String, String> {
    let mut parsed = Url::parse(url).map_err(|error| error.to_string())?;
    parsed.set_query(None);
    Ok(parsed.to_string())
}
