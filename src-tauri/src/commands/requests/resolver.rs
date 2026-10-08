use std::collections::BTreeMap;

use crate::commands::models::{
    AuthConfig, AuthType, BodyField, BodyFieldType, BodyMode, FileBody, GraphqlBody, KeyValue,
    RequestBody, RequestDetail,
};

use super::auth;
use crate::commands::variables::VariableContext;

pub(super) struct ResolvedRequest {
    pub url: String,
    pub headers: Vec<KeyValue>,
    pub query: Vec<KeyValue>,
    pub body: Option<RequestBody>,
    pub inherited_auth: Option<AuthConfig>,
    pub unresolved_variables: Vec<String>,
    /// `None` for a sensitive variable, whose value must not leave Rust.
    pub variable_values: BTreeMap<String, Option<String>>,
}

/// What resolving one piece of text looked up.
#[derive(Default)]
pub(super) struct Lookups {
    missing: Vec<String>,
    found: Vec<(String, Option<String>)>,
}

/// The variables the parts of a request that count looked up, each once.
#[derive(Default)]
struct References {
    unresolved: Vec<String>,
    values: BTreeMap<String, Option<String>>,
}

impl References {
    fn add(&mut self, lookups: Lookups) {
        for key in lookups.missing {
            if !self.unresolved.contains(&key) {
                self.unresolved.push(key);
            }
        }
        self.values.extend(lookups.found);
    }
}

/// Only enabled rows and the active body mode count as unresolved: those are
/// the only parts a send uses, so anything else must not block it.
pub(super) fn resolve_request_with_context(
    request: &RequestDetail,
    inherited_auth: Option<AuthConfig>,
    variables: &VariableContext,
) -> ResolvedRequest {
    let mut references = References::default();
    let (mut url, lookups) = resolve_text(&request.url, variables);
    // With query rows, the send rebuilds the query from them and drops the
    // URL's own, which can still list disabled params.
    match request.url.split_once('?') {
        Some((path, _)) if !request.query.is_empty() => {
            references.add(resolve_text(path, variables).1);
        }
        _ => references.add(lookups),
    }
    apply_path_params(&mut url, &request.path_params, variables, &mut references);

    // Each kind skips the blank keys the send skips.
    let headers = resolve_rows(
        &request.headers,
        |key| key.trim().is_empty(),
        variables,
        &mut references,
    );
    let query = resolve_rows(&request.query, str::is_empty, variables, &mut references);

    let body = request
        .body
        .as_ref()
        .map(|body| resolve_body(body, variables, &mut references));

    let auth_to_resolve = auth::effective_auth(request.auth.as_ref(), inherited_auth.as_ref());
    if let Some(auth) = auth_to_resolve.as_ref() {
        let used_fields: &[&Option<String>] = match auth.auth_type {
            AuthType::Bearer => &[&auth.token],
            AuthType::Basic => &[&auth.username, &auth.password],
            AuthType::ApiKey => &[&auth.value],
            AuthType::NoAuth | AuthType::Unsupported => &[],
        };
        for text in used_fields.iter().filter_map(|field| field.as_ref()) {
            references.add(resolve_text(text, variables).1);
        }
    }

    ResolvedRequest {
        url,
        headers,
        query,
        body,
        inherited_auth,
        unresolved_variables: references.unresolved,
        variable_values: references.values,
    }
}
fn resolve_rows(
    rows: &[KeyValue],
    is_blank_key: impl Fn(&str) -> bool,
    variables: &VariableContext,
    references: &mut References,
) -> Vec<KeyValue> {
    rows.iter()
        .map(|row| {
            let (value, lookups) = resolve_text(&row.value, variables);
            if row.enabled && !is_blank_key(&row.key) {
                references.add(lookups);
            }
            KeyValue {
                key: row.key.clone(),
                value,
                enabled: row.enabled,
            }
        })
        .collect()
}
fn apply_path_params(
    url: &mut String,
    path_params: &[KeyValue],
    variables: &VariableContext,
    references: &mut References,
) {
    for param in path_params.iter().filter(|param| param.enabled) {
        let key = param.key.trim();
        if key.is_empty() {
            continue;
        }

        let (value, lookups) = resolve_text(&param.value, variables);
        references.add(lookups);
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
            .is_none_or(|ch| matches!(ch, '/' | '?' | '#' | '&'));
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
    variables: &VariableContext,
    references: &mut References,
) -> RequestBody {
    let mode = body.mode;
    let mut track = |in_mode: BodyMode, lookups: Lookups| {
        if mode == in_mode {
            references.add(lookups);
        }
    };

    let (raw, lookups) = resolve_text(&body.raw, variables);
    track(BodyMode::Raw, lookups);
    let form_data = body
        .form_data
        .iter()
        .map(|field| {
            let (value, lookups) = resolve_text(&field.value, variables);
            let (file_path, file_lookups) =
                resolve_optional_text(field.file_path.as_ref(), variables);
            let (content_type, content_type_lookups) =
                resolve_optional_text(field.content_type.as_ref(), variables);
            // Mirrors `body::apply_body`: a text field sends only its value, a
            // file field only its path and content type.
            if field.enabled && !field.key.is_empty() {
                if field.field_type == BodyFieldType::File {
                    track(BodyMode::FormData, file_lookups);
                    track(BodyMode::FormData, content_type_lookups);
                } else {
                    track(BodyMode::FormData, lookups);
                }
            }
            BodyField {
                key: field.key.clone(),
                value,
                enabled: field.enabled,
                field_type: field.field_type,
                file_path,
                content_type,
            }
        })
        .collect();
    let urlencoded = body
        .urlencoded
        .iter()
        .map(|field| {
            let (value, lookups) = resolve_text(&field.value, variables);
            if field.enabled && !field.key.is_empty() {
                track(BodyMode::UrlEncoded, lookups);
            }
            KeyValue {
                key: field.key.clone(),
                value,
                enabled: field.enabled,
            }
        })
        .collect();
    let graphql = body.graphql.as_ref().map(|graphql| {
        let (query, lookups) = resolve_text(&graphql.query, variables);
        track(BodyMode::Graphql, lookups);
        let (graphql_variables, lookups) = resolve_text(&graphql.variables, variables);
        track(BodyMode::Graphql, lookups);
        GraphqlBody {
            query,
            variables: graphql_variables,
        }
    });
    let file = body.file.as_ref().map(|file| {
        let (path, lookups) = resolve_optional_text(file.path.as_ref(), variables);
        track(BodyMode::File, lookups);
        let (content_type, lookups) = resolve_optional_text(file.content_type.as_ref(), variables);
        track(BodyMode::File, lookups);
        FileBody { path, content_type }
    });

    RequestBody {
        mode,
        raw,
        raw_language: body.raw_language.clone(),
        form_data,
        urlencoded,
        graphql,
        file,
    }
}
fn resolve_optional_text(
    text: Option<&String>,
    variables: &VariableContext,
) -> (Option<String>, Lookups) {
    match text {
        Some(text) => {
            let (resolved, lookups) = resolve_text(text, variables);
            (Some(resolved), lookups)
        }
        None => (None, Lookups::default()),
    }
}
pub(super) fn resolve_text(text: &str, variables: &VariableContext) -> (String, Lookups) {
    let mut output = String::with_capacity(text.len());
    let mut lookups = Lookups::default();
    let mut rest = text;

    while let Some(start) = rest.find("{{") {
        let (before, after_start) = rest.split_at(start);
        output.push_str(before);
        if let Some(end) = after_start.find("}}") {
            let key = after_start[2..end].trim();
            if let Some(value) = variables.get(key) {
                output.push_str(&value);
                let shown = (!variables.is_sensitive(key)).then_some(value);
                lookups.found.push((key.to_string(), shown));
            } else {
                output.push_str(&after_start[..end + 2]);
                lookups.missing.push(key.to_string());
            }
            rest = &after_start[end + 2..];
        } else {
            output.push_str(after_start);
            rest = "";
        }
    }
    output.push_str(rest);
    (output, lookups)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key: &str, value: &str, enabled: bool) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: value.to_string(),
            enabled,
        }
    }

    fn unresolved_keys(request: &RequestDetail) -> Vec<String> {
        let preview = resolve_request_with_context(request, None, &VariableContext::default());
        preview.unresolved_variables
    }

    #[test]
    fn variables_in_disabled_rows_and_inactive_body_modes_do_not_block_sending() {
        let request = RequestDetail {
            url: "https://api.test/items".to_string(),
            headers: vec![row("Authorization", "{{oldToken}}", false)],
            query: vec![row("debug", "{{debugFlag}}", false)],
            body: Some(RequestBody {
                mode: BodyMode::UrlEncoded,
                raw: "{\"id\": \"{{leftoverRawId}}\"}".to_string(),
                raw_language: Some("json".to_string()),
                form_data: Vec::new(),
                urlencoded: vec![row("name", "{{name}}", true)],
                graphql: None,
                file: None,
            }),
            ..RequestDetail::default()
        };

        assert_eq!(unresolved_keys(&request), ["name"]);
    }

    #[test]
    fn a_disabled_param_left_in_the_url_and_blank_key_rows_do_not_block_sending() {
        let request = RequestDetail {
            url: "https://{{host}}/items?on=yes&off={{missing}}".to_string(),
            query: vec![row("on", "yes", true), row("off", "{{missing}}", false)],
            headers: vec![row("  ", "{{blankHeader}}", true)],
            ..RequestDetail::default()
        };

        assert_eq!(unresolved_keys(&request), ["host"]);
    }

    #[test]
    fn form_data_fields_count_only_the_parts_a_send_uses() {
        let field =
            |key: &str, field_type, value: &str, file_path: &str, content_type: &str| BodyField {
                key: key.to_string(),
                value: value.to_string(),
                enabled: true,
                field_type,
                file_path: Some(file_path.to_string()),
                content_type: Some(content_type.to_string()),
            };
        let request = RequestDetail {
            body: Some(RequestBody {
                mode: BodyMode::FormData,
                raw: String::new(),
                raw_language: None,
                form_data: vec![
                    field(
                        "note",
                        BodyFieldType::Text,
                        "{{note}}",
                        "{{unusedPath}}",
                        "{{unusedType}}",
                    ),
                    field(
                        "upload",
                        BodyFieldType::File,
                        "{{unusedValue}}",
                        "{{path}}",
                        "{{mime}}",
                    ),
                    field("", BodyFieldType::Text, "{{blankKey}}", "", ""),
                ],
                urlencoded: Vec::new(),
                graphql: None,
                file: None,
            }),
            ..RequestDetail::default()
        };

        assert_eq!(unresolved_keys(&request), ["note", "path", "mime"]);
    }

    #[test]
    fn preview_reports_the_value_of_each_variable_the_request_uses_and_hides_secrets() {
        let variables = VariableContext::with_values(&[
            ("host", "api.test", false),
            ("token", "s3cret", true),
            ("unused", "nobody asked", false),
            ("off", "disabled row", false),
        ]);
        let request = RequestDetail {
            url: "https://{{host}}/{{ token }}/{{missing}}".to_string(),
            headers: vec![row("X-Off", "{{off}}", false)],
            ..RequestDetail::default()
        };

        let resolved = resolve_request_with_context(&request, None, &variables);

        assert_eq!(
            resolved.variable_values,
            BTreeMap::from([
                ("host".to_string(), Some("api.test".to_string())),
                ("token".to_string(), None),
            ])
        );
        assert_eq!(resolved.unresolved_variables, ["missing"]);
    }

    #[test]
    fn unresolved_variables_are_listed_once_in_first_seen_order() {
        let request = RequestDetail {
            url: "https://{{host}}/{{version}}".to_string(),
            headers: vec![row("X-Trace", "{{trace}}-{{host}}", true)],
            ..RequestDetail::default()
        };

        assert_eq!(unresolved_keys(&request), ["host", "version", "trace"]);
    }
}
