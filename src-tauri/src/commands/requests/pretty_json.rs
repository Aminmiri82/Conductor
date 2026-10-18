//! Pretty-prints a JSON response for the viewer straight from its text.
//!
//! Building a `serde_json::Value` first would hold the whole document as a
//! tree next to the text, and would rewrite what the server sent: integers
//! past 64 bits lose precision and `1.50` becomes `1.5`. Here every number,
//! string, and escape is copied byte for byte; only whitespace changes.

use serde::de::IgnoredAny;

/// `None` when the text is not exactly one JSON value, so the caller shows it
/// as plain text. The layout matches `serde_json::to_string_pretty`.
pub(super) fn pretty_print(text: &str) -> Option<String> {
    // Checks the syntax without building anything.
    serde_json::from_str::<IgnoredAny>(text).ok()?;

    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len() + text.len() / 2);
    let mut depth = 0;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'"' => {
                let end = string_end(bytes, index);
                out.extend_from_slice(&bytes[index..end]);
                index = end;
                continue;
            }
            b'{' | b'[' => {
                let close = if byte == b'{' { b'}' } else { b']' };
                let next = skip_whitespace(bytes, index + 1);
                out.push(byte);
                if bytes[next] == close {
                    out.push(close);
                    index = next + 1;
                    continue;
                }
                depth += 1;
                new_line(&mut out, depth);
            }
            b'}' | b']' => {
                depth -= 1;
                new_line(&mut out, depth);
                out.push(byte);
            }
            b',' => {
                out.push(b',');
                new_line(&mut out, depth);
            }
            b':' => out.extend_from_slice(b": "),
            b' ' | b'\t' | b'\n' | b'\r' => {}
            _ => out.push(byte),
        }
        index += 1;
    }
    // Only ASCII was added or removed between whole UTF-8 sequences.
    String::from_utf8(out).ok()
}

/// The index just past the closing quote of the string opening at `start`.
/// Multi-byte UTF-8 never contains `"` or `\`, so bytes are safe to scan.
fn string_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1;
    loop {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => return index + 1,
            _ => index += 1,
        }
    }
}

fn skip_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while matches!(bytes[index], b' ' | b'\t' | b'\n' | b'\r') {
        index += 1;
    }
    index
}

fn new_line(out: &mut Vec<u8>, depth: usize) {
    out.push(b'\n');
    out.resize(out.len() + depth * 2, b' ');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lays_out_json_like_serde_pretty_printing() {
        let raw = r#" {"a":[1, {"b" :null},[]],"c":{},"d":{"e":"f"}} "#;
        let value = serde_json::from_str::<serde_json::Value>(raw).unwrap();

        assert_eq!(
            pretty_print(raw).as_deref(),
            Some(serde_json::to_string_pretty(&value).unwrap().as_str())
        );
    }

    #[test]
    fn keeps_numbers_exactly_as_the_server_sent_them() {
        let pretty = pretty_print(r#"{"id":12345678901234567890123,"price":1.50,"e":1E+2}"#);

        assert_eq!(
            pretty.as_deref(),
            Some("{\n  \"id\": 12345678901234567890123,\n  \"price\": 1.50,\n  \"e\": 1E+2\n}")
        );
    }

    #[test]
    fn leaves_punctuation_and_escapes_inside_strings_alone() {
        let raw = r#"["a, {b}: [c]", "quote \" and \\", "caf\u00e9 ☕"]"#;

        assert_eq!(
            pretty_print(raw).as_deref(),
            Some("[\n  \"a, {b}: [c]\",\n  \"quote \\\" and \\\\\",\n  \"caf\\u00e9 ☕\"\n]")
        );
    }

    #[test]
    fn text_that_is_not_one_json_value_is_not_formatted() {
        for raw in [
            "",
            "hello",
            "{\"a\":1",
            "{\"a\":1} trailing",
            "<html></html>",
        ] {
            assert_eq!(pretty_print(raw), None, "{raw:?}");
        }
    }
}
