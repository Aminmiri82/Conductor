use tauri::State;

use crate::{storage::DatabaseStatus, AppState};

#[tauri::command]
pub fn database_status(state: State<'_, AppState>) -> Result<DatabaseStatus, String> {
    state.database.status().map_err(|error| error.to_string())
}
