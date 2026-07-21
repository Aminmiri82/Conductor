use serde_json::{json, Map, Value};

use crate::commands::models::RequestDetail;

use super::request_auth::redact_auth_value;

const REDACTED: &str = "[REDACTED]";

/// Return a JSON payload representing the request that is safe to persist in
/// `request_history`: auth secrets, sensitive headers/query/body fields, and
/// secret-looking URL query values are replaced with `[REDACTED]`.
pub(super) fn redact_request(request: &RequestDetail) -> Value {
    let mut value = json!(request);
    redact_request_value(&mut value);
    value
}

/// Defensively re-redact a previously stored request snapshot.
pub(crate) fn redact_request_value(value: &mut Value) {
    for auth_key in ["auth", "inheritedAuth", "effectiveAuth"] {
        if let Some(auth) = value.get(auth_key).cloned() {
            value[auth_key] = redact_auth_value(auth);
        }
    }
    if let Some(headers) = value.get_mut("headers").and_then(Value::as_array_mut) {
        redact_key_value_array(headers, is_sensitive_header);
    }
    if let Some(query) = value.get_mut("query").and_then(Value::as_array_mut) {
        redact_key_value_array(query, is_sensitive_param);
    }
    if let Some(path_params) = value.get_mut("pathParams").and_then(Value::as_array_mut) {
        redact_key_value_array(path_params, is_sensitive_param);
    }
    if let Some(url) = value.get("url").and_then(Value::as_str) {
        value["url"] = Value::String(redact_url(url));
    }
    if let Some(body) = value.get_mut("body") {
        redact_body(body);
    }
}

/// Redact sensitive values inside response metadata before persist/return.
pub(crate) fn redact_response_meta(mut meta: Value) -> Value {
    if let Some(headers) = meta.get_mut("headers").and_then(Value::as_array_mut) {
        for header in headers.iter_mut() {
            let name = header
                .get("key")
                .or_else(|| header.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if is_sensitive_header(name) {
                if header.get("value").is_some() {
                    header["value"] = Value::String(REDACTED.to_string());
                }
            }
        }
    }
    meta
}

/// Redact secret-looking query parameters in a URL string.
pub(crate) fn redact_url(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else {
        return url.to_string();
    };
    if query.is_empty() {
        return url.to_string();
    }
    let redacted_query = query
        .split('&')
        .map(|pair| {
            let Some((key, value)) = pair.split_once('=') else {
                return pair.to_string();
            };
            if is_sensitive_param(key) {
                format!("{key}={REDACTED}")
            } else {
                format!("{key}={value}")
            }
        })
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}?{redacted_query}")
}

fn redact_key_value_array(entries: &mut [Value], sensitive: fn(&str) -> bool) {
    for entry in entries.iter_mut() {
        let key = entry
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if sensitive(key) {
            entry["value"] = Value::String(REDACTED.to_string());
        }
    }
}

fn redact_body(body: &mut Value) {
    if let Some(urlencoded) = body.get_mut("urlencoded").and_then(Value::as_array_mut) {
        redact_key_value_array(urlencoded, is_sensitive_param);
    }
    if let Some(form_data) = body.get_mut("formData").and_then(Value::as_array_mut) {
        redact_key_value_array(form_data, is_sensitive_param);
    }
    if let Some(raw) = body.get("raw").and_then(Value::as_str) {
        if looks_like_secret_payload(raw) {
            body["raw"] = Value::String(REDACTED.to_string());
        }
    }
    if let Some(graphql) = body.get_mut("graphql").and_then(Value::as_object_mut) {
        if let Some(variables) = graphql.get("variables").and_then(Value::as_str) {
            if looks_like_secret_payload(variables) {
                graphql.insert(
                    "variables".to_string(),
                    Value::String(REDACTED.to_string()),
                );
            }
        }
    }
}

fn is_sensitive_header(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "x-api-key"
            | "api-key"
            | "x-auth-token"
            | "x-access-token"
            | "x-secret-key"
            | "x-csrf-token"
    ) || lower.contains("api-key")
        || lower.contains("apikey")
        || lower.ends_with("-token")
        || lower.ends_with("-secret")
        || lower.ends_with("-password")
}

fn is_sensitive_param(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase().replace('_', "-");
    matches!(
        lower.as_str(),
        "access-token"
            | "api-key"
            | "apikey"
            | "auth"
            | "authorization"
            | "key"
            | "password"
            | "passwd"
            | "secret"
            | "token"
            | "refresh-token"
            | "client-secret"
            | "private-key"
    ) || lower.contains("api-key")
        || lower.contains("apikey")
        || lower.ends_with("-token")
        || lower.ends_with("-secret")
        || lower.ends_with("-password")
}

fn looks_like_secret_payload(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    lower.contains("\"password\"")
        || lower.contains("\"client_secret\"")
        || lower.contains("\"client-secret\"")
        || lower.contains("\"access_token\"")
        || lower.contains("\"refresh_token\"")
        || lower.contains("\"api_key\"")
        || lower.contains("\"api-key\"")
}

/// Build a redacted response-meta object from typed response headers.
pub(crate) fn response_meta_json(
    headers: &[crate::commands::models::ResponseHeader],
    content_type: Option<&str>,
    body_bytes: usize,
    body_format: &str,
    body_truncated: bool,
) -> Value {
    let headers_json: Vec<Value> = headers
        .iter()
        .map(|header| {
            let value = if is_sensitive_header(&header.key) {
                REDACTED.to_string()
            } else {
                header.value.clone()
            };
            json!({
                "key": header.key,
                "value": value,
            })
        })
        .collect();

    let mut map = Map::new();
    map.insert("headers".to_string(), Value::Array(headers_json));
    map.insert(
        "contentType".to_string(),
        content_type
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
    );
    map.insert("bodyBytes".to_string(), json!(body_bytes));
    map.insert("bodyFormat".to_string(), Value::String(body_format.to_string()));
    map.insert("bodyTruncated".to_string(), Value::Bool(body_truncated));
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::models::{AuthConfig, KeyValue, RequestBody};

    fn sample_request() -> RequestDetail {
        RequestDetail {
            id: "r1".into(),
            collection_id: "c1".into(),
            name: "demo".into(),
            method: "GET".into(),
            url: "https://api.example/v1?api_key=secret&page=1".into(),
            headers: vec![
                KeyValue {
                    key: "Authorization".into(),
                    value: "Bearer tok".into(),
                    enabled: true,
                },
                KeyValue {
                    key: "X-Custom-Token".into(),
                    value: "abc".into(),
                    enabled: true,
                },
                KeyValue {
                    key: "Accept".into(),
                    value: "application/json".into(),
                    enabled: true,
                },
            ],
            query: vec![KeyValue {
                key: "token".into(),
                value: "query-secret".into(),
                enabled: true,
            }],
            path_params: vec![],
            auth: Some(AuthConfig {
                auth_type: "bearer".into(),
                token: Some("bearer-secret".into()),
                username: None,
                password: None,
                key: None,
                value: None,
                add_to: None,
            }),
            inherited_auth: None,
            effective_auth: None,
            body: Some(RequestBody {
                mode: "urlencoded".into(),
                raw: String::new(),
                raw_language: None,
                form_data: vec![],
                urlencoded: vec![KeyValue {
                    key: "password".into(),
                    value: "hunter2".into(),
                    enabled: true,
                }],
                graphql: None,
                file: None,
            }),
            pre_request_script: None,
            test_script: None,
            updated_at: "now".into(),
        }
    }

    #[test]
    fn redacts_auth_headers_query_url_and_body() {
        let redacted = redact_request(&sample_request());
        assert_eq!(redacted["auth"]["token"], REDACTED);
        assert_eq!(redacted["headers"][0]["value"], REDACTED);
        assert_eq!(redacted["headers"][1]["value"], REDACTED);
        assert_eq!(redacted["headers"][2]["value"], "application/json");
        assert_eq!(redacted["query"][0]["value"], REDACTED);
        assert_eq!(
            redacted["url"].as_str().unwrap(),
            "https://api.example/v1?api_key=[REDACTED]&page=1"
        );
        assert_eq!(redacted["body"]["urlencoded"][0]["value"], REDACTED);
    }

    #[test]
    fn redacts_set_cookie_in_response_meta() {
        let meta = json!({
            "headers": [
                {"key": "Set-Cookie", "value": "session=abc"},
                {"key": "Content-Type", "value": "application/json"}
            ]
        });
        let redacted = redact_response_meta(meta);
        assert_eq!(redacted["headers"][0]["value"], REDACTED);
        assert_eq!(redacted["headers"][1]["value"], "application/json");
    }
}
