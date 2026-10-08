use std::time::Instant;

use chrono::Utc;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Method, Url,
};
use serde_json::Value;
use tauri::State;
use uuid::Uuid;

use crate::commands::{
    error::CommandError,
    models::{
        AuthConfig, KeyValue, RequestDetail, ResolvedRequestPreview, ResponseBodyFormat,
        ResponseHeader, SendRequestInput, SendRequestResult, VariableTarget,
    },
    variables::{load_variable_context, save_script_variable, VariableContext},
};
use crate::{storage::StorageError, AppState};

use super::postman_scripts::ScriptVariableScope;
use super::{auth, body, postman_scripts, resolver, response_files};

#[tauri::command(async)]
#[specta::specta]
pub fn resolve_request(
    request: RequestDetail,
    environment_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<ResolvedRequestPreview, CommandError> {
    let (variables, inherited_auth) = state.database.with_read_connection(|connection| {
        load_send_context(connection, &request, environment_id.as_deref())
    })?;
    let resolved = resolver::resolve_request_with_context(&request, inherited_auth, &variables);
    Ok(ResolvedRequestPreview {
        inherited_auth: resolved.inherited_auth,
        unresolved_variables: resolved.unresolved_variables,
        variable_values: resolved.variable_values,
    })
}
#[tauri::command]
#[specta::specta]
pub async fn send_request(
    input: SendRequestInput,
    state: State<'_, AppState>,
) -> Result<SendRequestResult, CommandError> {
    let (variables, inherited_auth) = state.database.with_read_connection(|connection| {
        load_send_context(connection, &input.request, input.environment_id.as_deref())
    })?;
    let resolved =
        resolver::resolve_request_with_context(&input.request, inherited_auth, &variables);
    if !resolved.unresolved_variables.is_empty() {
        return Err("request has unresolved variables".into());
    }

    let query_pairs = resolved
        .query
        .iter()
        .filter(|query| query.enabled && !query.key.is_empty())
        .map(|query| (query.key.clone(), query.value.clone()))
        .collect::<Vec<_>>();
    let request_url = if resolved.query.is_empty() {
        resolved.url.clone()
    } else {
        url_without_query(&resolved.url)?
    };

    let method = Method::from_bytes(input.request.method.as_bytes()).map_err(CommandError::msg)?;
    let mut builder = state.http_client.request(method, request_url);
    if !query_pairs.is_empty() {
        builder = builder.query(&query_pairs);
    }

    let mut headers = build_headers(&resolved.headers)?;
    if let Some(body) = resolved.body.as_ref() {
        body::insert_default_content_type(&mut headers, body);
    }
    builder = builder.headers(headers);

    let auth_to_apply = auth::effective_auth(
        input.request.auth.as_ref(),
        resolved.inherited_auth.as_ref(),
    );
    builder = auth::apply_auth(builder, auth_to_apply.as_ref(), &variables)?;

    if let Some(body) = resolved.body.as_ref() {
        builder = body::apply_body(builder, body).await?;
    }

    let started = Instant::now();
    let response = builder.send().await?;
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
    let content_disposition = response_headers
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok());
    let raw_body = response.bytes().await?;
    let body_bytes = raw_body.len();
    let history_id = Uuid::new_v4().to_string();
    let (body, body_format, body_json) =
        if response_files::is_binary_body(&raw_body, content_type.as_deref()) {
            let body_format = ResponseBodyFormat::Binary;
            state
                .binary_bodies
                .insert(&input.request.id, &history_id, raw_body);
            (String::new(), body_format, None)
        } else {
            let body_text = String::from_utf8_lossy(&raw_body);
            let body_json = serde_json::from_str::<Value>(&body_text).ok();
            let (body, body_format) = format_response_body(&body_text, body_json.as_ref());
            (body, body_format, body_json)
        };
    let download_file_name = response_files::download_file_name(
        content_disposition,
        content_type.as_deref(),
        body_format,
    );
    let now = Utc::now().to_rfc3339();
    let script_writes = postman_scripts::collect_postman_script_variables(
        input.request.test_script.as_ref(),
        body_json.as_ref(),
    );
    let mut updated_variables = Vec::new();
    let mut variable_warnings = Vec::new();

    state.database.with_connection(|connection| {
        for variable in &script_writes {
            match variable.scope {
                ScriptVariableScope::Collection => {
                    let target = VariableTarget::Collection {
                        collection_id: input.request.collection_id.clone(),
                    };
                    save_script_variable(
                        connection,
                        &target,
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
                        let target = VariableTarget::Environment {
                            environment_id: environment_id.to_string(),
                        };
                        save_script_variable(
                            connection,
                            &target,
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

        Ok(())
    })?;

    Ok(SendRequestResult {
        history_id,
        status_code: status.as_u16(),
        duration_ms,
        headers,
        body,
        body_bytes,
        body_content_type: content_type,
        body_format,
        download_file_name,
        updated_variables,
        variable_warnings,
    })
}
/// Folder auth is read from the database rather than taken from the client's
/// copy of the request, which goes stale when the request is moved.
fn load_send_context(
    connection: &rusqlite::Connection,
    request: &RequestDetail,
    environment_id: Option<&str>,
) -> Result<(VariableContext, Option<AuthConfig>), StorageError> {
    Ok((
        load_variable_context(connection, request, environment_id)?,
        auth::inherited_auth_for_request(connection, request)?,
    ))
}
fn format_response_body(
    body_text: &str,
    body_json: Option<&Value>,
) -> (String, ResponseBodyFormat) {
    if let Some(json) = body_json {
        let pretty = serde_json::to_string_pretty(json).unwrap_or_else(|_| body_text.to_string());
        return (pretty, ResponseBodyFormat::Json);
    }
    (body_text.to_string(), ResponseBodyFormat::Text)
}
/// Repeated names are all sent (two `Cookie` rows send two headers).
fn build_headers(rows: &[KeyValue]) -> Result<HeaderMap, CommandError> {
    let mut headers = HeaderMap::new();
    for header in rows.iter().filter(|header| header.enabled) {
        if header.key.trim().is_empty() {
            continue;
        }
        let name = HeaderName::from_bytes(header.key.as_bytes())?;
        let value = HeaderValue::from_str(&header.value)?;
        headers.append(name, value);
    }
    Ok(headers)
}
fn url_without_query(url: &str) -> Result<String, CommandError> {
    let mut parsed = Url::parse(url).map_err(CommandError::msg)?;
    parsed.set_query(None);
    Ok(parsed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_header_rows_are_all_sent() {
        let row = |key: &str, value: &str| KeyValue {
            key: key.to_string(),
            value: value.to_string(),
            enabled: true,
        };

        let headers = build_headers(&[
            row("Accept", "text/html"),
            row("accept", "application/json"),
        ])
        .expect("valid headers");

        let accept = headers.get_all("accept").iter().collect::<Vec<_>>();
        assert_eq!(accept, ["text/html", "application/json"]);
    }

    #[test]
    fn pretty_json_body_keeps_the_key_order_the_server_sent() {
        let raw = r#"{"schemaVersion":1,"generatedAt":"now","products":[{"productId":"a","manufacturer":"ST","breakPrice1":2.9}]}"#;
        let json = serde_json::from_str::<Value>(raw).ok();

        let (body, format) = format_response_body(raw, json.as_ref());

        assert_eq!(format, ResponseBodyFormat::Json);
        let keys = [
            "schemaVersion",
            "generatedAt",
            "products",
            "productId",
            "manufacturer",
            "breakPrice1",
        ];
        let positions = keys.map(|key| body.find(&format!("\"{key}\"")).expect("key present"));
        assert!(positions.is_sorted(), "keys out of server order:\n{body}");
    }
}
