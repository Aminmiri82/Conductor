#[cfg(test)]
mod bindings;
mod commands;
mod storage;

use commands::collections::{get_collection_tree, import_postman_collection, list_collections};
use commands::requests::{
    create_environment, create_folder, create_request, delete_environment, delete_node,
    delete_request, duplicate_request, get_request, import_postman_environment, list_environments,
    list_variables, move_node, rename_environment, resolve_request, save_request, save_text_file,
    save_variables, send_request,
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

/// Registers every command exposed to the frontend. Tauri's command macros
/// live at the crate root, so this must stay in `lib.rs`.
fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            database_status,
            get_workspace_state,
            set_workspace_state,
            create_environment,
            create_folder,
            create_request,
            delete_environment,
            delete_node,
            delete_request,
            duplicate_request,
            get_collection_tree,
            get_request,
            import_postman_environment,
            import_postman_collection,
            list_collections,
            list_environments,
            list_variables,
            rename_environment,
            resolve_request,
            move_node,
            save_request,
            save_text_file,
            save_variables,
            send_request,
        ])
        // Commands reject with the error string, matching plain `invoke`.
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        // Positions, byte counts, and durations never approach 2^53.
        .dangerously_cast_bigints_to_number()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let specta = specta_builder();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build());

    // Agent automation bridge (`pnpm dev:agent`). Never in normal or release builds.
    #[cfg(feature = "mcp-bridge")]
    let builder = builder.plugin(
        tauri_plugin_mcp_bridge::Builder::new()
            .bind_address("127.0.0.1")
            .build(),
    );

    builder
        .setup(|app| {
            #[cfg(feature = "mcp-bridge")]
            app.add_capability(
                r#"{"identifier":"mcp-bridge","windows":["*"],"permissions":["mcp-bridge:default"]}"#,
            )?;

            // Explicit data directories also keep agent runs isolated.
            let app_data_dir = match std::env::var_os("CONDUCTOR_DATA_DIR") {
                Some(dir) => std::path::PathBuf::from(dir),
                None => default_data_dir(app.path().app_data_dir()?, cfg!(debug_assertions)),
            };
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
                "check_for_updates" => Some("check-for-updates"),
                _ => None,
            };

            if let Some(action) = action {
                let _ = app.emit("app-menu-action", action);
            }
        })
        .invoke_handler(specta.invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn default_data_dir(app_data_dir: std::path::PathBuf, development: bool) -> std::path::PathBuf {
    if development {
        app_data_dir.with_file_name("com.yaramiri.conductor.dev")
    } else {
        app_data_dir
    }
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
                    &MenuItemBuilder::with_id("check_for_updates", "Check for Updates...")
                        .build(app)?,
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

#[cfg(test)]
mod data_dir_tests {
    use super::default_data_dir;
    use std::path::PathBuf;

    #[test]
    fn development_and_release_use_separate_default_data_directories() {
        let release =
            PathBuf::from("/Users/yara/Library/Application Support/com.yaramiri.conductor");

        assert_eq!(default_data_dir(release.clone(), false), release);
        assert_eq!(
            default_data_dir(release, true),
            PathBuf::from("/Users/yara/Library/Application Support/com.yaramiri.conductor.dev")
        );
    }
}
