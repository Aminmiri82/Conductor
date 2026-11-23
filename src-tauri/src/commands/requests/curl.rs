//! "Copy as cURL": the request as a shell command that sends what Send sends.
//! Variables without a value to copy become `<name>` placeholders.

use tauri::State;

use crate::commands::{
    error::CommandError,
    models::{ApiKeyLocation, AuthConfig, AuthType, BodyFieldType, BodyMode, RequestDetail},
    variables::{load_variable_context, VariableContext},
};
use crate::AppState;

use super::{auth, body, resolver};

/// With `include_values`, fills in the variables the request's environment
/// has, except secrets; without it, every variable is a placeholder.
#[tauri::command(async)]
#[specta::specta]
pub fn request_as_curl(
    request: RequestDetail,
    environment_id: Option<String>,
    include_values: bool,
    state: State<'_, AppState>,
) -> Result<String, CommandError> {
    let (variables, inherited_auth) = state.database.with_read_connection(|connection| {
        let variables = if include_values {
            load_variable_context(connection, &request, environment_id.as_deref())?
                .without_secrets()
        } else {
            VariableContext::nothing()
        };
        Ok((
            variables,
            auth::inherited_auth_for_request(connection, &request)?,
        ))
    })?;
    to_curl(&request, inherited_auth, &variables)
}

pub(super) fn to_curl(
    request: &RequestDetail,
    inherited_auth: Option<AuthConfig>,
    variables: &VariableContext,
) -> Result<String, CommandError> {
    let resolved = resolver::resolve_request_with_context(request, inherited_auth, variables);
    let resolve = |text: &str| resolver::resolve_text(text, variables).0;

    let mut query = resolved
        .query
        .iter()
        .filter(|row| row.enabled && !row.key.is_empty())
        .map(|row| (row.key.clone(), row.value.clone()))
        .collect::<Vec<_>>();
    let mut headers = resolved
        .headers
        .iter()
        .filter(|row| row.enabled && !row.key.trim().is_empty())
        .map(|row| format!("{}: {}", row.key, row.value))
        .collect::<Vec<_>>();
    let mut args = Vec::new();

    if let Some(auth) = request.auth.as_ref().or(resolved.inherited_auth.as_ref()) {
        match auth.auth_type {
            AuthType::Bearer => {
                if let Some(token) = &auth.token {
                    headers.push(format!("Authorization: Bearer {}", resolve(token)));
                }
            }
            AuthType::Basic => {
                let username = auth.username.as_deref().map(resolve).unwrap_or_default();
                let password = auth.password.as_deref().map(resolve).unwrap_or_default();
                args.push(format!("-u {}", quote(&format!("{username}:{password}"))));
            }
            AuthType::ApiKey => {
                let key = auth.key.as_deref().unwrap_or("Authorization");
                let value = auth.value.as_deref().map(resolve).unwrap_or_default();
                if auth.add_to == Some(ApiKeyLocation::Query) {
                    query.push((key.to_string(), value));
                } else if !key.is_empty() {
                    headers.push(format!("{key}: {value}"));
                }
            }
            AuthType::NoAuth | AuthType::Unsupported => {}
        }
    }

    if let Some(body) = &resolved.body {
        let has_content_type = resolved
            .headers
            .iter()
            .any(|row| row.enabled && row.key.trim().eq_ignore_ascii_case("content-type"));
        if let Some(content_type) = body::default_content_type(body).filter(|_| !has_content_type) {
            headers.push(format!("Content-Type: {content_type}"));
        }
        match body.mode {
            BodyMode::Raw => args.push(format!("--data-raw {}", quote(&body.raw))),
            BodyMode::UrlEncoded => {
                for field in body
                    .urlencoded
                    .iter()
                    .filter(|field| field.enabled && !field.key.is_empty())
                {
                    args.push(format!(
                        "--data-urlencode {}",
                        quote(&format!("{}={}", field.key, field.value))
                    ));
                }
            }
            BodyMode::FormData => {
                for field in body
                    .form_data
                    .iter()
                    .filter(|field| field.enabled && !field.key.is_empty())
                {
                    if field.field_type == BodyFieldType::File {
                        // Chosen paths are never saved, so a reopened request
                        // has none; the placeholder keeps the field visible.
                        let mut part = format!("{}=@{}", field.key, file_path(&field.file_path));
                        if let Some(content_type) = field
                            .content_type
                            .as_deref()
                            .filter(|value| !value.is_empty())
                        {
                            part.push_str(&format!(";type={content_type}"));
                        }
                        args.push(format!("-F {}", quote(&part)));
                    } else {
                        // `--form-string` because `-F` reads a value starting
                        // with `@` or `<` as a file.
                        args.push(format!(
                            "--form-string {}",
                            quote(&format!("{}={}", field.key, field.value))
                        ));
                    }
                }
            }
            BodyMode::Graphql => {
                let payload = body::graphql_payload(body.graphql.as_ref())?;
                args.push(format!("--data-raw {}", quote(&payload.to_string())));
            }
            BodyMode::File => {
                let path = file_path(&body.file.as_ref().and_then(|file| file.path.clone()));
                args.push(format!("--data-binary {}", quote(&format!("@{path}"))));
            }
            BodyMode::None | BodyMode::Unsupported => {}
        }
    }

    // Like Send: query rows replace the URL's own query; anything else (an
    // API key) is appended to whatever the URL has.
    let mut url = if resolved.query.is_empty() {
        resolved.url.clone()
    } else {
        resolved
            .url
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .to_string()
    };
    for (key, value) in &query {
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str(&format!("{}={}", encode(key), encode(value)));
    }

    let mut lines = vec![match request.method.as_str() {
        "GET" => format!("curl {}", quote(&url)),
        // `-X HEAD` makes curl wait for a body that never comes.
        "HEAD" => format!("curl --head {}", quote(&url)),
        method => format!("curl -X {method} {}", quote(&url)),
    }];
    lines.extend(headers.iter().map(|header| format!("-H {}", quote(header))));
    lines.extend(args);
    Ok(lines.join(" \\\n  "))
}

fn file_path(path: &Option<String>) -> String {
    path.as_deref()
        .filter(|path| !path.is_empty())
        .unwrap_or("{{file}}")
        .to_string()
}

/// Single-quotes for POSIX shells, with unresolved `{{name}}` as `<name>`.
fn quote(text: &str) -> String {
    format!("'{}'", placeholders(text).replace('\'', r"'\''"))
}

fn placeholders(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let Some(end) = rest[start..].find("}}").map(|end| start + end) else {
            break;
        };
        output.push_str(&rest[..start]);
        output.push('<');
        output.push_str(rest[start + 2..end].trim());
        output.push('>');
        rest = &rest[end + 2..];
    }
    output.push_str(rest);
    output
}

/// Form-encodes like Send's query (space as `+`), but leaves `{{name}}`
/// whole so it still becomes a readable placeholder.
fn encode(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let (before, token, after) = match rest.find("{{") {
            Some(start) => match rest[start..].find("}}") {
                Some(end) => (
                    &rest[..start],
                    &rest[start..start + end + 2],
                    &rest[start + end + 2..],
                ),
                None => (rest, "", ""),
            },
            None => (rest, "", ""),
        };
        for byte in before.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                    output.push(byte as char)
                }
                b' ' => output.push('+'),
                _ => output.push_str(&format!("%{byte:02X}")),
            }
        }
        output.push_str(token);
        if after.is_empty() {
            return output;
        }
        rest = after;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::models::{BodyField, KeyValue, RequestBody};

    fn row(key: &str, value: &str) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: value.to_string(),
            enabled: true,
        }
    }

    fn body(mode: BodyMode) -> RequestBody {
        RequestBody {
            mode,
            raw: String::new(),
            raw_language: None,
            form_data: Vec::new(),
            urlencoded: Vec::new(),
            graphql: None,
            file: None,
        }
    }

    fn post_json() -> RequestDetail {
        RequestDetail {
            method: "POST".to_string(),
            url: "{{host}}/users/:id?stale=1".to_string(),
            path_params: vec![row("id", "{{userId}}")],
            query: vec![
                row("q", "a b&{{term}}"),
                KeyValue {
                    enabled: false,
                    ..row("off", "x")
                },
            ],
            headers: vec![row("X-Trace", "{{trace}}")],
            auth: Some(AuthConfig {
                auth_type: AuthType::Bearer,
                token: Some("{{token}}".to_string()),
                username: None,
                password: None,
                key: None,
                value: None,
                add_to: None,
            }),
            body: Some(RequestBody {
                raw: "{\"name\": \"it's {{name}}\"}".to_string(),
                raw_language: Some("json".to_string()),
                ..body(BodyMode::Raw)
            }),
            ..RequestDetail::default()
        }
    }

    #[test]
    fn without_values_every_variable_is_a_placeholder() {
        let curl = to_curl(&post_json(), None, &VariableContext::nothing()).unwrap();
        assert_eq!(
            curl,
            [
                "curl -X POST '<host>/users/<userId>?q=a+b%26<term>'",
                "-H 'X-Trace: <trace>'",
                "-H 'Authorization: Bearer <token>'",
                "-H 'Content-Type: application/json'",
                r#"--data-raw '{"name": "it'\''s <name>"}'"#,
            ]
            .join(" \\\n  ")
        );
    }

    #[test]
    fn with_values_fills_all_but_secrets_and_missing_ones() {
        let variables = VariableContext::with_values(&[
            ("host", "https://api.test", false),
            ("userId", "42", false),
            ("term", "x y", false),
            ("token", "s3cret", true),
            ("name", "Ada", false),
        ])
        .without_secrets();
        let curl = to_curl(&post_json(), None, &variables).unwrap();
        assert!(curl.starts_with("curl -X POST 'https://api.test/users/42?q=a+b%26x+y'"));
        assert!(curl.contains("-H 'X-Trace: <trace>'"));
        assert!(curl.contains("-H 'Authorization: Bearer <token>'"));
        assert!(!curl.contains("s3cret"));
        assert!(curl.contains(r#""it'\''s Ada""#));
    }

    #[test]
    fn dynamic_variables_stay_placeholders_without_values() {
        let request = RequestDetail {
            method: "GET".to_string(),
            url: "https://api.test/{{$guid}}".to_string(),
            ..RequestDetail::default()
        };
        assert_eq!(
            to_curl(&request, None, &VariableContext::nothing()).unwrap(),
            "curl 'https://api.test/<$guid>'"
        );
    }

    #[test]
    fn inherited_auth_and_form_bodies_match_what_send_sends() {
        let field = |key: &str, field_type, value: &str| BodyField {
            key: key.to_string(),
            value: value.to_string(),
            enabled: true,
            field_type,
            file_path: None,
            content_type: Some("image/png".to_string()),
        };
        let request = RequestDetail {
            method: "PUT".to_string(),
            url: "https://api.test/upload".to_string(),
            headers: vec![row("content-type", "text/plain")],
            body: Some(RequestBody {
                form_data: vec![
                    field("note", BodyFieldType::Text, "@not-a-file"),
                    field("photo", BodyFieldType::File, ""),
                ],
                ..body(BodyMode::FormData)
            }),
            ..RequestDetail::default()
        };
        let inherited = AuthConfig {
            auth_type: AuthType::ApiKey,
            token: None,
            username: None,
            password: None,
            key: Some("api_key".to_string()),
            value: Some("k".to_string()),
            add_to: Some(ApiKeyLocation::Query),
        };
        assert_eq!(
            to_curl(&request, Some(inherited), &VariableContext::nothing()).unwrap(),
            [
                "curl -X PUT 'https://api.test/upload?api_key=k'",
                "-H 'content-type: text/plain'",
                "--form-string 'note=@not-a-file'",
                "-F 'photo=@<file>;type=image/png'",
            ]
            .join(" \\\n  ")
        );
    }

    #[test]
    fn head_requests_do_not_wait_for_a_body() {
        let request = RequestDetail {
            method: "HEAD".to_string(),
            url: "https://api.test".to_string(),
            ..RequestDetail::default()
        };
        assert_eq!(
            to_curl(&request, None, &VariableContext::nothing()).unwrap(),
            "curl --head 'https://api.test'"
        );
    }
}
