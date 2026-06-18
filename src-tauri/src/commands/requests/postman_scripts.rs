use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ScriptVariableScope {
    Collection,
    Environment,
}

#[derive(Debug, Clone)]
pub(super) struct ScriptVariableWrite {
    pub scope: ScriptVariableScope,
    pub key: String,
    pub value: String,
}

pub(super) fn collect_postman_script_variables(
    script: Option<&Value>,
    body_json: Option<&Value>,
) -> Vec<ScriptVariableWrite> {
    let Some(lines) = script
        .and_then(|script| script.get("exec"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    lines
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|line| parse_postman_variable_set(line, body_json))
        .collect()
}
fn parse_postman_variable_set(
    line: &str,
    body_json: Option<&Value>,
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
fn resolve_script_expression(expression: &str, body_json: Option<&Value>) -> Option<String> {
    if let Some(value) = unquote(expression) {
        return Some(value);
    }

    let path = expression.strip_prefix("jsonData.")?;
    let mut current = body_json?;
    for segment in path.split('.') {
        current = current.get(segment.trim())?;
    }

    match current {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
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
