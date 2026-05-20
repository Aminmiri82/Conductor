mod commands;
mod storage;

use commands::collections::{get_collection_tree, import_postman_collection, list_collections};
use commands::requests::{
    create_folder, create_request, delete_node, delete_request, duplicate_request, get_request,
    list_variables, move_node, resolve_request, save_request, save_text_file, save_variables,
    send_request,
};
use commands::storage::{database_status, get_workspace_state, set_workspace_state};
use storage::Database;
use tauri::{
    menu::{Menu, MenuItemBuilder, PredefinedMenuItem, Submenu},
    Emitter, Manager,
};

pub struct AppState {
    database: Database,
    http_client: reqwest::Client,
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
            let http_client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::limited(10))
                .build()?;

            app.manage(AppState {
                database,
                http_client,
            });
            app.set_menu(build_app_menu(app.handle())?)?;
            Ok(())
        })
        .on_menu_event(|app, event| {
            let action = match event.id().as_ref() {
                "open_request" => Some("open-request"),
                "new_request" => Some("new-request"),
                "duplicate_request" => Some("duplicate-request"),
                "save_request" => Some("save-request"),
                "send_request" => Some("send-request"),
                "close_request" => Some("close-request"),
                "toggle_sidebar" => Some("toggle-sidebar"),
                "focus_url" => Some("focus-url"),
                "settings" => Some("settings"),
                _ => None,
            };

            if let Some(action) = action {
                let _ = app.emit("app-menu-action", action);
            }
        })
        .invoke_handler(tauri::generate_handler![
            database_status,
            get_workspace_state,
            set_workspace_state,
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

fn build_app_menu<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<Menu<R>> {
    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &MenuItemBuilder::with_id("new_request", "New Request")
                .accelerator("CmdOrCtrl+N")
                .build(app)?,
            &MenuItemBuilder::with_id("open_request", "Open Request...")
                .accelerator("CmdOrCtrl+O")
                .build(app)?,
            &MenuItemBuilder::with_id("duplicate_request", "Duplicate Request")
                .accelerator("CmdOrCtrl+D")
                .build(app)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItemBuilder::with_id("save_request", "Save Request")
                .accelerator("CmdOrCtrl+S")
                .build(app)?,
            &MenuItemBuilder::with_id("send_request", "Send Request")
                .accelerator("CmdOrCtrl+Enter") // enter icon looks weird on mac, check if we can use something else
                .build(app)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItemBuilder::with_id("close_request", "Close Request")
                .accelerator("CmdOrCtrl+W")
                .build(app)?,
        ],
    )?;

    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItemBuilder::with_id("focus_url", "Focus URL")
                .accelerator("CmdOrCtrl+L")
                .build(app)?,
        ],
    )?;

    let view_menu = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &MenuItemBuilder::with_id("toggle_sidebar", "Toggle Sidebar")
                .accelerator("CmdOrCtrl+B")
                .build(app)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;

    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
        ],
    )?;

    let help_menu = Submenu::with_items(app, "Help", true, &[])?;

    Menu::with_items(
        app,
        &[
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                app,
                app.package_info().name.clone(),
                true,
                &[
                    &PredefinedMenuItem::about(app, None, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &MenuItemBuilder::with_id("settings", "Settings...")
                        .accelerator("CmdOrCtrl+,")
                        .build(app)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::services(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::hide(app, None)?,
                    &PredefinedMenuItem::hide_others(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::quit(app, None)?,
                ],
            )?,
            &file_menu,
            &edit_menu,
            &view_menu,
            &window_menu,
            &help_menu,
        ],
    )
}
