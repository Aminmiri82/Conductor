use std::collections::VecDeque;
use std::fs;
use std::sync::Mutex;

use bytes::Bytes;
use tauri::State;

use crate::commands::models::{ResponseBodyFormat, SaveResponseBodyInput};
use crate::AppState;

/// Binary bodies are kept only so they can be downloaded. Most recent first,
/// one per request, at most this many, so a session of file downloads cannot
/// grow memory without bound.
const MAX_BINARY_BODIES: usize = 8;

/// Raw bytes of recent binary responses. They never cross IPC; the frontend
/// asks Rust to write them to disk by history id.
#[derive(Default)]
pub struct BinaryBodies(Mutex<VecDeque<BinaryBody>>);

struct BinaryBody {
    request_id: String,
    history_id: String,
    bytes: Bytes,
}

impl BinaryBodies {
    pub fn insert(&self, request_id: &str, history_id: &str, bytes: Bytes) {
        let mut bodies = self.0.lock().unwrap_or_else(|error| error.into_inner());
        bodies.retain(|body| body.request_id != request_id);
        bodies.push_front(BinaryBody {
            request_id: request_id.to_string(),
            history_id: history_id.to_string(),
            bytes,
        });
        bodies.truncate(MAX_BINARY_BODIES);
    }

    fn get(&self, history_id: &str) -> Option<Bytes> {
        let bodies = self.0.lock().unwrap_or_else(|error| error.into_inner());
        bodies
            .iter()
            .find(|body| body.history_id == history_id)
            .map(|body| body.bytes.clone())
    }
}

#[tauri::command]
#[specta::specta]
pub fn save_response_body(
    input: SaveResponseBodyInput,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let bytes = state
        .binary_bodies
        .get(&input.history_id)
        .ok_or("This response is no longer in memory. Send the request again to download it.")?;
    fs::write(input.path, bytes).map_err(|error| error.to_string())
}

/// A body is binary when it is not UTF-8 and the server did not call it text,
/// so an xlsx or PDF is never decoded into a string of replacement characters.
pub fn is_binary_body(bytes: &[u8], content_type: Option<&str>) -> bool {
    if content_type.is_some_and(is_textual_content_type) {
        return false;
    }
    std::str::from_utf8(bytes).is_err()
}

fn is_textual_content_type(content_type: &str) -> bool {
    let mime = content_type.split(';').next().unwrap_or_default().trim();
    let mime = mime.to_ascii_lowercase();
    let Some((kind, subtype)) = mime.split_once('/') else {
        return false;
    };
    // Match whole subtypes: "xml" inside "vnd.openxmlformats-…sheet" is an Excel file.
    kind == "text"
        || subtype.ends_with("+json")
        || subtype.ends_with("+xml")
        || matches!(
            subtype,
            "json" | "xml" | "javascript" | "x-www-form-urlencoded" | "graphql" | "yaml" | "x-yaml"
        )
}

/// The name the save dialog suggests: the server's `Content-Disposition`
/// filename when it sends one, otherwise a name that fits the body.
pub fn download_file_name(
    content_disposition: Option<&str>,
    content_type: Option<&str>,
    format: ResponseBodyFormat,
) -> String {
    if let Some(name) = content_disposition.and_then(disposition_file_name) {
        return name;
    }
    let extension = match format {
        ResponseBodyFormat::Json => "json",
        ResponseBodyFormat::Text => "txt",
        ResponseBodyFormat::Binary => content_type.and_then(binary_extension).unwrap_or("bin"),
    };
    format!("response.{extension}")
}

fn disposition_file_name(header: &str) -> Option<String> {
    let mut plain = None;
    for part in header.split(';').map(str::trim) {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            // RFC 5987: `filename*=UTF-8''Q3%20report.xlsx`. Preferred over `filename`.
            "filename*" => {
                let encoded = value.trim().splitn(3, '\'').nth(2)?;
                if let Some(name) = percent_decode(encoded).and_then(|name| safe_file_name(&name)) {
                    return Some(name);
                }
            }
            "filename" => plain = safe_file_name(value.trim().trim_matches('"')),
            _ => {}
        }
    }
    plain
}

/// Server-supplied names are only a suggestion, but never a path.
fn safe_file_name(name: &str) -> Option<String> {
    let name = name.rsplit(['/', '\\']).next()?.trim();
    (!name.is_empty() && name != "." && name != "..").then(|| name.to_string())
}

fn percent_decode(value: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(value.len());
    let mut input = value.bytes();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let hex = [input.next()?, input.next()?];
            bytes.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes).ok()
}

fn binary_extension(content_type: &str) -> Option<&'static str> {
    let mime = content_type.split(';').next()?.trim().to_ascii_lowercase();
    Some(match mime.as_str() {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "xlsx",
        "application/vnd.ms-excel" => "xls",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        "application/pdf" => "pdf",
        "application/zip" => "zip",
        "application/gzip" => "gz",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
    // The first bytes of a real xlsx: a zip local file header.
    const ZIP_HEADER: &[u8] = b"PK\x03\x04\x14\x00\x06\x00\x08\x00\x00\x00!\x00\xa0\xff";

    #[test]
    fn an_excel_file_is_binary_and_json_is_not() {
        assert!(is_binary_body(ZIP_HEADER, Some(XLSX)));
        assert!(is_binary_body(ZIP_HEADER, None));
        assert!(!is_binary_body(br#"{"ok":true}"#, Some("application/json")));
        assert!(!is_binary_body(b"{}", Some("application/problem+json")));
    }

    #[test]
    fn text_the_server_labels_as_text_stays_text_even_when_not_utf8() {
        // Latin-1 "café", which is not valid UTF-8.
        assert!(!is_binary_body(
            b"caf\xe9",
            Some("text/plain; charset=iso-8859-1")
        ));
    }

    #[test]
    fn download_uses_the_servers_file_name() {
        let name = download_file_name(
            Some(r#"attachment; filename="products.xlsx""#),
            Some(XLSX),
            ResponseBodyFormat::Binary,
        );
        assert_eq!(name, "products.xlsx");
    }

    #[test]
    fn download_prefers_the_encoded_unicode_file_name() {
        let name = download_file_name(
            Some(
                r#"attachment; filename="fallback.xlsx"; filename*=UTF-8''Q3%20r%C3%A9sum%C3%A9.xlsx"#,
            ),
            Some(XLSX),
            ResponseBodyFormat::Binary,
        );
        assert_eq!(name, "Q3 résumé.xlsx");
    }

    #[test]
    fn download_never_suggests_a_path_from_the_server() {
        let name = download_file_name(
            Some(r#"attachment; filename="../../etc/report.xlsx""#),
            None,
            ResponseBodyFormat::Binary,
        );
        assert_eq!(name, "report.xlsx");
    }

    #[test]
    fn download_without_a_file_name_picks_an_extension_from_the_content_type() {
        assert_eq!(
            download_file_name(None, Some(XLSX), ResponseBodyFormat::Binary),
            "response.xlsx"
        );
        assert_eq!(
            download_file_name(None, None, ResponseBodyFormat::Binary),
            "response.bin"
        );
        assert_eq!(
            download_file_name(None, None, ResponseBodyFormat::Json),
            "response.json"
        );
    }

    #[test]
    fn only_the_latest_binary_body_per_request_is_kept() {
        let bodies = BinaryBodies::default();
        bodies.insert("request", "first", Bytes::from_static(b"one"));
        bodies.insert("request", "second", Bytes::from_static(b"two"));

        assert!(bodies.get("first").is_none());
        assert_eq!(bodies.get("second").as_deref(), Some(&b"two"[..]));
    }

    #[test]
    fn binary_bodies_are_capped() {
        let bodies = BinaryBodies::default();
        for index in 0..=MAX_BINARY_BODIES {
            bodies.insert(
                &format!("request-{index}"),
                &format!("history-{index}"),
                Bytes::new(),
            );
        }

        assert!(bodies.get("history-0").is_none());
        assert!(bodies
            .get(&format!("history-{MAX_BINARY_BODIES}"))
            .is_some());
    }
}
