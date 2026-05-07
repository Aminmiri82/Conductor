mod commands;
mod storage;

use commands::storage::database_status;
use storage::Database;
use tauri::Manager;

pub struct AppState {
    database: Database,
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {name}! You've been greeted from Rust!")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let database = Database::open(app_data_dir)?;

            app.manage(AppState { database });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![database_status, greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
