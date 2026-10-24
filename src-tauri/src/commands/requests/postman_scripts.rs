use std::cell::OnceCell;

use serde_json::Value;

#[derive(Debug)]
pub(super) enum ScriptVariableScope {
    Collection,
    Environment,
}

#[derive(Debug)]
pub(super) struct ScriptVariableWrite {
    pub scope: ScriptVariableScope,
    pub key: String,
    pub value: String,
}

/// `body_text` is parsed as JSON only if a line reads `jsonData`, so a send
/// without such a script never builds a tree of the response.
pub(super) fn collect_postman_script_variables(
    script: Option<&Value>,
    body_text: Option<&str>,
) -> Vec<ScriptVariableWrite> {
    let Some(lines) = script
        .and_then(|script| script.get("exec"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    let body_json = OnceCell::new();
    let body_json = || {
        body_json
            .get_or_init(|| body_text.and_then(|text| serde_json::from_str::<Value>(text).ok()))
            .as_ref()
    };
    lines
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|line| parse_postman_variable_set(line, &body_json))
        .collect()
}
fn parse_postman_variable_set<'a>(
    line: &str,
    body_json: &impl Fn() -> Option<&'a Value>,
) -> Option<ScriptVariableWrite> {
    let (marker, scope) = if line.contains("postman.setEnvironmentVariable") {
        (
            "postman.setEnvironmentVariable",
            ScriptVariableScope::Environment,
        )
    } else if line.contains("pm.environment.set") {
        ("pm.environment.set", ScriptVariableScope::Environment)
    } else if line.contains("pm.collectionVariables.set") {
        (
            "pm.collectionVariables.set",
            ScriptVariableScope::Collection,
        )
    } else {
        return None;
    };

    let call = line.split_once(marker)?.1;
    let args = call
        .strip_prefix('(')?
        .trim_end_matches(';')
        .trim_end_matches(')');
    let (key_part, value_part) = args.split_once(',')?;
    let key = unquote(key_part.trim())?;
    let value = resolve_script_expression(value_part.trim(), body_json)?;

    Some(ScriptVariableWrite { scope, key, value })
}
fn resolve_script_expression<'a>(
    expression: &str,
    body_json: &impl Fn() -> Option<&'a Value>,
) -> Option<String> {
    if let Some(value) = unquote(expression) {
        return Some(value);
    }

    let path = expression.strip_prefix("jsonData.")?;
    let mut current = body_json()?;
    for segment in path.split('.') {
        current = current.get(segment.trim())?;
    }

    match current {
        Value::String(value) => Some(value.clone()),
        _ => Some(current.to_string()),
    }
}
fn unquote(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.len() < 2 {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let quote = bytes[0];
    if (quote == b'"' || quote == b'\'') && bytes[trimmed.len() - 1] == quote {
        return Some(trimmed[1..trimmed.len() - 1].to_string());
    }
    None
}
