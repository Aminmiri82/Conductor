use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{
    ipc::{InvokeResponseBody, IpcResponse},
    State,
};

use crate::commands::{
    error::CommandError,
    models::{
        DiscardResponsesInput, GetResponseBodyInput, ResponseBodyFormat, SaveResponseBodyInput,
    },
};
use crate::AppState;

const MISSING_BODY: &str =
    "This response is no longer available. Send the request again to see it.";

/// The body of each request's latest response, one file per response, so
/// neither the webview nor Rust holds bodies the user is not looking at. The
/// webview reads the one it shows with `get_response_body` and discards a
/// request's body when its tab closes. The directory is emptied at startup,
/// so a crash leaves nothing behind for longer than one session.
pub struct ResponseBodies {
    dir: PathBuf,
    /// Request id to the history id of its latest response, which names the
    /// file. Only ids in here are ever turned into paths.
    latest: Mutex<HashMap<String, String>>,
}

impl ResponseBodies {
    pub fn open(dir: PathBuf) -> io::Result<Self> {
        match fs::remove_dir_all(&dir) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            latest: Mutex::default(),
        })
    }

    /// Replaces the request's previous body.
    pub fn insert(&self, request_id: &str, history_id: &str, body: &[u8]) -> io::Result<()> {
        fs::write(self.dir.join(history_id), body)?;
        let previous = self
            .lock()
            .insert(request_id.to_string(), history_id.to_string());
        if let Some(previous) = previous {
            let _ = fs::remove_file(self.dir.join(previous));
        }
        Ok(())
    }

    pub fn discard(&self, request_ids: &[String]) {
        let mut latest = self.lock();
        for request_id in request_ids {
            if let Some(history_id) = latest.remove(request_id) {
                let _ = fs::remove_file(self.dir.join(history_id));
            }
        }
    }

    pub(super) fn read_text(&self, history_id: &str) -> Result<RawText, CommandError> {
        Ok(RawText(fs::read(self.path(history_id)?)?))
    }

    fn path(&self, history_id: &str) -> Result<PathBuf, CommandError> {
        self.lock()
            .values()
            .any(|latest| latest == history_id)
            .then(|| self.dir.join(history_id))
            .ok_or_else(|| MISSING_BODY.into())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.latest
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

/// UTF-8 text that crosses IPC as raw bytes and reaches the webview as an
/// `ArrayBuffer`. As a JSON string, a large body would also exist escaped in
/// Rust and be parsed again in the webview.
pub struct RawText(pub(super) Vec<u8>);

impl IpcResponse for RawText {
    fn body(self) -> tauri::Result<InvokeResponseBody> {
        Ok(InvokeResponseBody::Raw(self.0))
    }
}

impl specta::Type for RawText {
    fn definition(_: &mut specta::Types) -> specta::datatype::DataType {
        specta_typescript::define("ArrayBuffer").into()
    }
}

/// The text the response viewer shows: pretty-printed for JSON, as received
/// for text. Binary bodies are only ever saved with `save_response_body`.
#[tauri::command(async)]
#[specta::specta]
pub fn get_response_body(
    input: GetResponseBodyInput,
    state: State<'_, AppState>,
) -> Result<RawText, CommandError> {
    state.response_bodies.read_text(&input.history_id)
}

#[tauri::command(async)]
#[specta::specta]
pub fn discard_responses(input: DiscardResponsesInput, state: State<'_, AppState>) {
    state.response_bodies.discard(&input.request_ids);
}

#[tauri::command(async)]
#[specta::specta]
pub fn save_response_body(
    input: SaveResponseBodyInput,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    fs::copy(state.response_bodies.path(&input.history_id)?, input.path)?;
    Ok(())
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

    fn bodies() -> (ResponseBodies, PathBuf) {
        let dir = std::env::temp_dir().join(format!("conductor-bodies-{}", uuid::Uuid::new_v4()));
        (ResponseBodies::open(dir.clone()).unwrap(), dir)
    }

    #[test]
    fn only_the_latest_body_per_request_is_kept() {
        let (bodies, dir) = bodies();
        bodies.insert("request", "first", b"one").unwrap();
        bodies.insert("request", "second", b"two").unwrap();

        assert!(bodies.path("first").is_err());
        assert!(!dir.join("first").exists());
        assert_eq!(fs::read(bodies.path("second").unwrap()).unwrap(), b"two");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_discarded_body_is_deleted() {
        let (bodies, dir) = bodies();
        bodies.insert("request", "history", b"body").unwrap();

        bodies.discard(&["request".to_string()]);

        assert!(bodies.path("history").is_err());
        assert!(!dir.join("history").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn opening_empties_what_a_previous_session_left() {
        let (bodies, dir) = bodies();
        bodies.insert("request", "history", b"body").unwrap();
        drop(bodies);

        let _reopened = ResponseBodies::open(dir.clone()).unwrap();

        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::remove_dir_all(dir).unwrap();
    }
}
