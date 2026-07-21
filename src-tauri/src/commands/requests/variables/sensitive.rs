//! Heuristics for deciding whether a variable key should be treated as sensitive.

/// True when a variable key name looks like it holds a secret.
pub(crate) fn looks_sensitive_variable_key(key: &str) -> bool {
    let lower = key.trim().to_ascii_lowercase().replace('_', "-");
    matches!(
        lower.as_str(),
        "access-token"
            | "api-key"
            | "apikey"
            | "auth"
            | "authorization"
            | "client-secret"
            | "password"
            | "passwd"
            | "private-key"
            | "refresh-token"
            | "secret"
            | "token"
            | "auth-token"
    ) || lower.contains("password")
        || lower.contains("secret")
        || lower.contains("api-key")
        || lower.contains("apikey")
        || lower.ends_with("-token")
}
