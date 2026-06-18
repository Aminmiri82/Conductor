use std::collections::HashMap;

use crate::commands::models::{
    BodyField, FileBody, GraphqlBody, KeyValue, RequestBody, RequestDetail, ResolvedRequestPreview,
    UnresolvedVariable,
};

use super::request_auth;

pub(super) fn resolve_request_with_context(
    request: &RequestDetail,
    variables: &HashMap<String, String>,
) -> ResolvedRequestPreview {
    let mut unresolved = HashMap::<String, Vec<String>>::new();
    let (mut url, missing) = resolve_text(&request.url, variables);
    add_missing(&mut unresolved, "url", missing);
    apply_path_params(&mut url, &request.path_params, variables, &mut unresolved);

    let headers = request
        .headers
        .iter()
        .map(|header| {
            let (value, missing) = resolve_text(&header.value, variables);
            add_missing(&mut unresolved, &format!("header:{}", header.key), missing);
            KeyValue {
                key: header.key.clone(),
                value,
                enabled: header.enabled,
            }
        })
        .collect();

    let query = request
        .query
        .iter()
        .map(|query| {
            let (value, missing) = resolve_text(&query.value, variables);
            add_missing(&mut unresolved, &format!("query:{}", query.key), missing);
            KeyValue {
                key: query.key.clone(),
                value,
                enabled: query.enabled,
            }
        })
        .collect();

    let body = request
        .body
        .as_ref()
        .map(|body| resolve_body(body, variables, &mut unresolved));

    let auth_to_resolve =
        request_auth::effective_auth(request.auth.as_ref(), request.inherited_auth.as_ref());
    if let Some(auth) = auth_to_resolve.as_ref() {
        if auth.auth_type == "bearer" {
            if let Some(token) = auth.token.as_ref() {
                let (_, missing) = resolve_text(token, variables);
                add_missing(&mut unresolved, "auth:bearer", missing);
            }
        } else if auth.auth_type == "basic" {
            if let Some(username) = auth.username.as_ref() {
                let (_, missing) = resolve_text(username, variables);
                add_missing(&mut unresolved, "auth:basic.username", missing);
            }
            if let Some(password) = auth.password.as_ref() {
                let (_, missing) = resolve_text(password, variables);
                add_missing(&mut unresolved, "auth:basic.password", missing);
            }
        } else if auth.auth_type == "apikey" {
            if let Some(value) = auth.value.as_ref() {
                let (_, missing) = resolve_text(value, variables);
                add_missing(&mut unresolved, "auth:apikey.value", missing);
            }
        }
    }

    ResolvedRequestPreview {
        url,
        headers,
        query,
        body,
        unresolved_variables: unresolved
            .into_iter()
            .map(|(key, locations)| UnresolvedVariable { key, locations })
            .collect(),
    }
}
fn apply_path_params(
    url: &mut String,
    path_params: &[KeyValue],
    variables: &HashMap<String, String>,
    unresolved: &mut HashMap<String, Vec<String>>,
) {
    for param in path_params.iter().filter(|param| param.enabled) {
        let key = param.key.trim();
        if key.is_empty() {
            continue;
        }

        let (value, missing) = resolve_text(&param.value, variables);
        add_missing(unresolved, &format!("path:{}", key), missing);
        if value.is_empty() {
            continue;
        }

        *url = replace_path_param(url, key, &value);
    }
}
fn replace_path_param(url: &str, key: &str, value: &str) -> String {
    let needle: String = format!(":{}", key);
    let mut out = String::with_capacity(url.len() + value.len());
    let mut rest = url;

    while let Some(index) = rest.find(&needle) {
        let before = &rest[..index];
        let after = &rest[index + needle.len()..];
        out.push_str(before);

        let boundary = after
            .chars()
            .next()
            .map_or(true, |ch| matches!(ch, '/' | '?' | '#' | '&'));
        if boundary {
            out.push_str(value);
            rest = after;
        } else {
            out.push_str(&needle);
            rest = after;
        }
    }

    out.push_str(rest);
    out
}
fn resolve_body(
    body: &RequestBody,
    variables: &HashMap<String, String>,
    unresolved: &mut HashMap<String, Vec<String>>,
) -> RequestBody {
    let (raw, missing) = resolve_text(&body.raw, variables);
    add_missing(unresolved, "body", missing);
    RequestBody {
        mode: body.mode.clone(),
        raw,
        raw_language: body.raw_language.clone(),
        form_data: body
            .form_data
            .iter()
            .map(|field| {
                let (value, missing) = resolve_text(&field.value, variables);
                add_missing(unresolved, &format!("form:{}", field.key), missing);
                let (file_path, missing) =
                    resolve_optional_text(field.file_path.as_ref(), variables);
                add_missing(unresolved, &format!("form-file:{}", field.key), missing);
                let (content_type, missing) =
                    resolve_optional_text(field.content_type.as_ref(), variables);
                add_missing(
                    unresolved,
                    &format!("form-content-type:{}", field.key),
                    missing,
                );
                BodyField {
                    key: field.key.clone(),
                    value,
                    enabled: field.enabled,
                    field_type: field.field_type.clone(),
                    file_path,
                    content_type,
                }
            })
            .collect(),
        urlencoded: body
            .urlencoded
            .iter()
            .map(|field| {
                let (value, missing) = resolve_text(&field.value, variables);
                add_missing(unresolved, &format!("urlencoded:{}", field.key), missing);
                KeyValue {
                    key: field.key.clone(),
                    value,
                    enabled: field.enabled,
                }
            })
            .collect(),
        graphql: body.graphql.as_ref().map(|graphql| {
            let (query, missing) = resolve_text(&graphql.query, variables);
            add_missing(unresolved, "graphql:query", missing);
            let (graphql_variables, missing) = resolve_text(&graphql.variables, variables);
            add_missing(unresolved, "graphql:variables", missing);
            GraphqlBody {
                query,
                variables: graphql_variables,
            }
        }),
        file: body.file.as_ref().map(|file| {
            let (path, missing) = resolve_optional_text(file.path.as_ref(), variables);
            add_missing(unresolved, "body:file", missing);
            let (content_type, missing) =
                resolve_optional_text(file.content_type.as_ref(), variables);
            add_missing(unresolved, "body:file-content-type", missing);
            FileBody { path, content_type }
        }),
    }
}
fn resolve_optional_text(
    text: Option<&String>,
    variables: &HashMap<String, String>,
) -> (Option<String>, Vec<String>) {
    match text {
        Some(text) => {
            let (resolved, missing) = resolve_text(text, variables);
            (Some(resolved), missing)
        }
        None => (None, Vec::new()),
    }
}
pub(super) fn resolve_text(
    text: &str,
    variables: &HashMap<String, String>,
) -> (String, Vec<String>) {
    let mut output = String::with_capacity(text.len());
    let mut missing = Vec::new();
    let mut rest = text;

    while let Some(start) = rest.find("{{") {
        let (before, after_start) = rest.split_at(start);
        output.push_str(before);
        if let Some(end) = after_start.find("}}") {
            let key = after_start[2..end].trim();
            if let Some(value) = variables.get(key) {
                output.push_str(value);
            } else {
                output.push_str(&after_start[..end + 2]);
                missing.push(key.to_string());
            }
            rest = &after_start[end + 2..];
        } else {
            output.push_str(after_start);
            rest = "";
        }
    }
    output.push_str(rest);
    (output, missing)
}
fn add_missing(
    unresolved: &mut HashMap<String, Vec<String>>,
    location: &str,
    missing: Vec<String>,
) {
    for key in missing {
        unresolved
            .entry(key)
            .or_default()
            .push(location.to_string());
    }
}
