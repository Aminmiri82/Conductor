mod commands;
mod storage;

use commands::collections::{get_collection_tree, import_postman_collection, list_collections};
use commands::requests::{
    create_folder, create_request, delete_node, delete_request, duplicate_request, get_request,
    list_variables, move_node, resolve_request, save_request, save_text_file, save_variables,
    send_request,
};
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
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let database = Database::open(app_data_dir)?;

            app.manage(AppState { database });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            database_status,
            create_folder,
            create_request,
            delete_node,
            delete_request,
            duplicate_request,
            get_collection_tree,
            get_request,
            import_postman_collection,
            list_collections,
            list_variables,
            resolve_request,
            move_node,
            save_request,
            save_text_file,
            save_variables,
            send_request,
            greet
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
