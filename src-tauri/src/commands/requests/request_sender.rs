use std::time::Instant;

use chrono::Utc;
use futures_util::StreamExt;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Method, Url,
};
use rusqlite::params;
use serde_json::{json, Value};
use tauri::State;
use uuid::Uuid;

use crate::commands::models::{
    KeyValue, RequestDetail, ResolvedRequestPreview, ResponseHeader, SendRequestInput,
    SendRequestResult,
};
use crate::commands::{AppError, AppResult};
use crate::{AppState, MAX_RESPONSE_BODY_BYTES};

use super::postman_scripts::ScriptVariableScope;
use super::variables::{load_variable_context, save_script_variable};
use super::{postman_scripts, request_auth, request_body, variable_resolver};

#[tauri::command]
pub fn resolve_request(
    request: RequestDetail,
    environment_id: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<ResolvedRequestPreview> {
    Ok(state.database.with_read_connection(|connection| {
        let variables = load_variable_context(
            connection,
            state.database.secrets(),
            &request,
            environment_id.as_deref(),
        )?;
        Ok(variable_resolver::resolve_request_with_context(
            &request, &variables,
        ))
    })?)
}

#[tauri::command]
pub async fn send_request(
    input: SendRequestInput,
    state: State<'_, AppState>,
) -> AppResult<SendRequestResult> {
    let cancel_token = state.take_cancel_token();

    let variables = state.database.with_read_connection(|connection| {
        load_variable_context(
            connection,
            state.database.secrets(),
            &input.request,
            input.environment_id.as_deref(),
        )
    })?;
    let preview = variable_resolver::resolve_request_with_context(&input.request, &variables);
    if !preview.unresolved_variables.is_empty() {
        return Err(AppError::msg("request has unresolved variables"));
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

    let method = Method::from_bytes(input.request.method.as_bytes())
        .map_err(|e| AppError::msg(e.to_string()))?;
    let mut builder = state.http_client.request(method, request_url);
    let mut headers = HeaderMap::new();
    if !query_pairs.is_empty() {
        builder = builder.query(&query_pairs);
    }

    for header in preview.headers.iter().filter(|header| header.enabled) {
        if header.key.trim().is_empty() {
            continue;
        }
        let name = HeaderName::from_bytes(header.key.as_bytes())
            .map_err(|e| AppError::msg(e.to_string()))?;
        let value =
            HeaderValue::from_str(&header.value).map_err(|e| AppError::msg(e.to_string()))?;
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
    builder = request_auth::apply_auth(builder, auth_to_apply.as_ref(), &variables)
        .map_err(AppError::msg)?;

    if let Some(body) = preview.body.as_ref() {
        builder = request_body::apply_body(builder, body)
            .await
            .map_err(AppError::msg)?;
    }

    let started = Instant::now();
    let response = tokio::select! {
        biased;
        _ = cancel_token.cancelled() => return Err(AppError::Cancelled),
        response = builder.send() => response?,
    };
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

    let (body_bytes_raw, body_truncated) =
        read_body_with_limit(response, &cancel_token, MAX_RESPONSE_BODY_BYTES).await?;
    let duration_ms = started.elapsed().as_millis();
    let body_text = String::from_utf8_lossy(&body_bytes_raw).into_owned();
    let body_bytes = body_bytes_raw.len();
    let body_json = if body_truncated {
        None
    } else {
        serde_json::from_str::<Value>(&body_text).ok()
    };
    let (body, body_format) = format_response_body(&body_text, body_json.as_ref());
    let history_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let script_writes = postman_scripts::collect_postman_script_variables(
        input.request.test_script.as_ref(),
        body_json.as_ref(),
    );
    let mut updated_variables = Vec::new();
    let mut variable_warnings = Vec::new();
    if body_truncated {
        variable_warnings.push(format!(
            "Response body exceeded {} MiB and was truncated",
            MAX_RESPONSE_BODY_BYTES / (1024 * 1024)
        ));
    }

    let sanitized_request = super::history_redaction::redact_request(&input.request);

    state.database.with_connection(|connection| {
        for variable in &script_writes {
            match variable.scope {
                ScriptVariableScope::Collection => {
                    save_script_variable(
                        connection,
                        "collection",
                        Some(&input.request.collection_id),
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
                    if let Some(environment_id) = input.environment_id.as_deref() {
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
                sanitized_request.to_string(),
                json!({
                    "headers": headers,
                    "contentType": content_type,
                    "bodyBytes": body_bytes,
                    "bodyFormat": body_format,
                    "bodyTruncated": body_truncated
                })
                .to_string(),
                now
            ],
        )?;
        Ok(())
    })?;

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
        body_truncated,
        updated_variables,
        variable_warnings,
        unresolved_variables: preview.unresolved_variables,
    })
}

/// Cancel the currently in-flight `send_request` invocation, if any.
#[tauri::command]
pub fn cancel_send_request(state: State<'_, AppState>) -> AppResult<()> {
    state.cancel_current_send();
    Ok(())
}

async fn read_body_with_limit(
    response: reqwest::Response,
    cancel_token: &tokio_util::sync::CancellationToken,
    limit: usize,
) -> AppResult<(Vec<u8>, bool)> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut truncated = false;
    let mut stream = response.bytes_stream();
    loop {
        let chunk = tokio::select! {
            biased;
            _ = cancel_token.cancelled() => return Err(AppError::Cancelled),
            chunk = stream.next() => chunk,
        };
        let Some(chunk) = chunk else { break };
        let bytes = chunk?;
        if buffer.len() + bytes.len() > limit {
            let remaining = limit.saturating_sub(buffer.len());
            buffer.extend_from_slice(&bytes[..remaining]);
            truncated = true;
            break;
        }
        buffer.extend_from_slice(&bytes);
    }
    Ok((buffer, truncated))
}

fn format_response_body(body_text: &str, body_json: Option<&Value>) -> (String, String) {
    if let Some(json) = body_json {
        let pretty = serde_json::to_string_pretty(json).unwrap_or_else(|_| body_text.to_string());
        return (pretty, "json".to_string());
    }
    (body_text.to_string(), "text".to_string())
}

fn url_without_query(url: &str) -> AppResult<String> {
    let mut parsed = Url::parse(url).map_err(|error| AppError::msg(error.to_string()))?;
    parsed.set_query(None);
    Ok(parsed.to_string())
}
