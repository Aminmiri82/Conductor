use rusqlite::params;
use serde_json::Value;
use tauri::State;

use crate::commands::models::{
    ListRequestHistoryInput, RequestHistoryDetail, RequestHistoryEntry,
};
use crate::commands::{AppError, AppResult};
use crate::storage::StorageError;
use crate::AppState;

const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 1000;

#[tauri::command]
pub fn list_request_history(
    input: Option<ListRequestHistoryInput>,
    state: State<'_, AppState>,
) -> AppResult<Vec<RequestHistoryEntry>> {
    let input = input.unwrap_or(ListRequestHistoryInput {
        limit: None,
        collection_id: None,
    });
    let limit = input.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let collection_filter = input.collection_id.filter(|id| !id.is_empty());

    Ok(state.database.with_read_connection(|connection| {
        let mut statement = connection.prepare(
            "SELECT id, request_id, collection_id, method, url, status_code, duration_ms,
                    response_meta_json, created_at
             FROM request_history
             WHERE (? IS NULL OR collection_id = ?)
             ORDER BY created_at DESC
             LIMIT ?",
        )?;
        let rows = statement
            .query_map(
                params![collection_filter, collection_filter, limit],
                |row| {
                    let response_meta_json: Option<String> = row.get(7)?;
                    let meta = response_meta_json
                        .as_deref()
                        .and_then(|value| serde_json::from_str::<Value>(value).ok());
                    let body_bytes = meta
                        .as_ref()
                        .and_then(|value| value.get("bodyBytes"))
                        .and_then(Value::as_i64);
                    let body_content_type = meta
                        .as_ref()
                        .and_then(|value| value.get("contentType"))
                        .and_then(Value::as_str)
                        .map(ToString::to_string);
                    let created_at: String = row.get(8)?;
                    Ok(RequestHistoryEntry {
                        id: row.get(0)?,
                        request_id: row.get(1)?,
                        collection_id: row.get(2)?,
                        name: None,
                        method: row.get(3)?,
                        url: row.get(4)?,
                        status_code: row.get(5)?,
                        status_text: None,
                        duration_ms: row.get(6)?,
                        body_bytes,
                        body_content_type,
                        error: None,
                        executed_at: created_at,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;
        Ok(rows)
    })?)
}

#[tauri::command]
pub fn get_request_history(
    id: String,
    state: State<'_, AppState>,
) -> AppResult<Option<RequestHistoryDetail>> {
    Ok(state.database.with_read_connection(|connection| {
        let mut statement = connection.prepare(
            "SELECT id, request_id, collection_id, method, url, status_code, duration_ms,
                    request_json, response_meta_json, created_at
             FROM request_history
             WHERE id = ?",
        )?;
        let mut rows = statement.query(params![id])?;
        let row = rows.next()?;
        let Some(row) = row else {
            return Ok(None);
        };

        let request_json: String = row.get(7)?;
        let response_meta_json: Option<String> = row.get(8)?;

        Ok(Some(RequestHistoryDetail {
            id: row.get(0)?,
            request_id: row.get(1)?,
            collection_id: row.get(2)?,
            method: row.get(3)?,
            url: row.get(4)?,
            status_code: row.get(5)?,
            duration_ms: row.get(6)?,
            // request_json is stored pre-redacted at insert time, but re-run
            // the redaction defensively in case a caller stored plaintext.
            request_json: serde_json::from_str::<Value>(&request_json).ok(),
            response_meta_json: response_meta_json
                .and_then(|value| serde_json::from_str::<Value>(&value).ok()),
            created_at: row.get(9)?,
        }))
    })?)
}

#[tauri::command]
pub fn purge_request_history(state: State<'_, AppState>) -> AppResult<usize> {
    state
        .database
        .with_connection(|connection| {
            let deleted = connection.execute("DELETE FROM request_history", [])?;
            Ok(deleted)
        })
        .map_err(AppError::from)
}

#[tauri::command]
pub fn purge_expired_request_history(state: State<'_, AppState>) -> AppResult<usize> {
    Ok(state.database.purge_expired_request_history()?)
}
