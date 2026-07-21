use serde_json::{json, Value};

use crate::commands::models::RequestDetail;

use super::request_auth::redact_auth_value;

/// Return a JSON payload representing the request that is safe to persist in
/// `request_history`: all `token`, `password`, and `value` auth fields are
/// replaced with the literal `[REDACTED]` string.
pub(super) fn redact_request(request: &RequestDetail) -> Value {
    let mut value = json!(request);
    if let Some(auth) = value.get("auth").cloned() {
        value["auth"] = redact_auth_value(auth);
    }
    if let Some(auth) = value.get("inheritedAuth").cloned() {
        value["inheritedAuth"] = redact_auth_value(auth);
    }
    if let Some(auth) = value.get("effectiveAuth").cloned() {
        value["effectiveAuth"] = redact_auth_value(auth);
    }
    if let Some(headers) = value.get_mut("headers").and_then(Value::as_array_mut) {
        for header in headers.iter_mut() {
            if is_sensitive_header(header.get("key").and_then(Value::as_str).unwrap_or_default()) {
                header["value"] = Value::String("[REDACTED]".to_string());
            }
        }
    }
    value
}

fn is_sensitive_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "authorization" | "cookie" | "proxy-authorization" | "x-api-key"
    )
}
