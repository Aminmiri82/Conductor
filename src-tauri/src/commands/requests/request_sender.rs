use std::time::Instant;

use chrono::Utc;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Method, Url,
};
use rusqlite::params;
use serde_json::{json, Value};
use tauri::State;

use crate::commands::models::{
    KeyValue, RequestDetail, ResolvedRequestPreview, ResponseHeader, SendRequestInput,
    SendRequestResult,
};
use crate::AppState;

use super::postman_scripts::ScriptVariableScope;
use super::variables::{load_variable_context, save_script_variable};
use super::{postman_scripts, request_auth, request_body, variable_resolver};

#[tauri::command]
pub fn resolve_request(
    request: RequestDetail,
    environment_id: Option<crate::commands::models::EntityId>,
    state: State<'_, AppState>,
) -> Result<ResolvedRequestPreview, String> {
    state
        .database
        .with_read_connection(|connection| {
            let variables = load_variable_context(connection, &request, environment_id)?;
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
        .with_read_connection(|connection| {
            load_variable_context(connection, &input.request, input.environment_id)
        })
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
        request_body::insert_default_content_type(&mut headers, body);
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
    let mut history_id = 0;
    let now = Utc::now().to_rfc3339();
    let script_writes = postman_scripts::collect_postman_script_variables(
        input.request.test_script.as_ref(),
        body_json.as_ref(),
    );
    let mut updated_variables = Vec::new();
    let mut variable_warnings = Vec::new();

    state
        .database
        .with_connection(|connection| {
            for variable in &script_writes {
                match variable.scope {
                    ScriptVariableScope::Collection => {
                        save_script_variable(
                            connection,
                            "collection",
                            Some(input.request.collection_id),
                            None,
                            &variable.key,
                            &variable.value,
                            &now,
                        )?;
                        updated_variables.push(KeyValue {
                            key: variable.key.clone(),
                            value: variable.value.clone(),
                            enabled: true,
                        });
                    }
                    ScriptVariableScope::Environment => {
                        if let Some(environment_id) = input.environment_id {
                            save_script_variable(
                                connection,
                                "environment",
                                None,
                                Some(environment_id),
                                &variable.key,
                                &variable.value,
                                &now,
                            )?;
                            updated_variables.push(KeyValue {
                                key: variable.key.clone(),
                                value: variable.value.clone(),
                                enabled: true,
                            });
                        } else {
                            variable_warnings.push(format!(
                                "Skipped environment variable '{}' because no environment is active",
                                variable.key
                            ));
                        }
                    }
                }
            }

            connection.execute(
                "INSERT INTO request_history
                 (request_id, collection_id, method, url, status_code, duration_ms, request_json,
                  response_meta_json, response_body_path, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?)",
                params![
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
            history_id = connection.last_insert_rowid();
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
        variable_warnings,
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
