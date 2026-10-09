#[cfg(any(test, feature = "mcp-bridge"))]
mod agent_data_dir;
#[cfg(test)]
mod bindings;
mod commands;
#[cfg(test)]
mod fixtures;
mod storage;
#[cfg(test)]
mod test_server;

use commands::collections::{
    create_folder, delete_node, get_collection_tree, import_postman_collection, list_collections,
    move_node,
};
use commands::requests::{
    create_request, duplicate_request, get_request, resolve_request, save_request,
    save_response_body, save_text_file, send_request, BinaryBodies,
};
use commands::variables::{
    apply_variable_changes, create_environment, delete_environment, import_postman_environment,
    list_environments, list_variables, rename_environment,
};
use commands::workspace_state::{get_workspace_state, set_workspace_state};
use storage::Database;
use tauri::{
    menu::{Menu, MenuItemBuilder, PredefinedMenuItem, Submenu},
    Manager,
};
use tauri_specta::Event;

pub struct AppState {
    database: Database,
    http_client: reqwest::Client,
    binary_bodies: BinaryBodies,
}

/// What the user picked in the app menu, sent to the webview as the typed
/// `app-menu-action` event.
#[derive(
    Debug, Clone, Copy, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event,
)]
#[serde(rename_all = "kebab-case")]
pub enum AppMenuAction {
    OpenRequest,
    NewRequest,
    DuplicateRequest,
    SaveRequest,
    SendRequest,
    CloseRequest,
    ToggleSidebar,
    FocusUrl,
    Settings,
    CheckForUpdates,
}

impl AppMenuAction {
    /// The ids given to the menu items in `build_app_menu`; predefined items
    /// (copy, quit) have none of ours.
    fn from_menu_id(id: &str) -> Option<Self> {
        Some(match id {
            "open_request" => Self::OpenRequest,
            "new_request" => Self::NewRequest,
            "duplicate_request" => Self::DuplicateRequest,
            "save_request" => Self::SaveRequest,
            "send_request" => Self::SendRequest,
            "close_request" => Self::CloseRequest,
            "toggle_sidebar" => Self::ToggleSidebar,
            "focus_url" => Self::FocusUrl,
            "settings" => Self::Settings,
            "check_for_updates" => Self::CheckForUpdates,
            _ => return None,
        })
    }
}

/// Registers every command exposed to the frontend. Tauri's command macros
/// live at the crate root, so this must stay in `lib.rs`.
///
/// Commands are declared `#[tauri::command(async)]` even when synchronous:
/// Tauri otherwise runs sync commands on the main thread, where a large
/// import or file write freezes the window.
fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            get_workspace_state,
            set_workspace_state,
            apply_variable_changes,
            create_environment,
            create_folder,
            create_request,
            delete_environment,
            delete_node,
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
            save_response_body,
            send_request,
        ])
        .events(tauri_specta::collect_events![AppMenuAction])
        // Commands reject with the error string, matching plain `invoke`.
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        // Positions, byte counts, and durations never approach 2^53.
        .dangerously_cast_bigints_to_number()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let specta = specta_builder();
    let invoke_handler = specta.invoke_handler();

    let builder = tauri::Builder::default()
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
        .setup(move |app| {
            specta.mount_events(app);
            #[cfg(feature = "mcp-bridge")]
            app.add_capability(
                r#"{"identifier":"mcp-bridge","windows":["*"],"permissions":["mcp-bridge:default"]}"#,
            )?;

            #[cfg(feature = "mcp-bridge")]
            let app_data_dir = agent_data_dir::resolve(
                std::env::var_os("CONDUCTOR_DATA_DIR").map(std::path::PathBuf::from),
                &app.path().app_data_dir()?,
            )?;
            #[cfg(not(feature = "mcp-bridge"))]
            let app_data_dir = match std::env::var_os("CONDUCTOR_DATA_DIR") {
                Some(dir) => std::path::PathBuf::from(dir),
                None => default_data_dir(app.path().app_data_dir()?, cfg!(debug_assertions)),
            };
            let database = Database::open(app_data_dir)?;
            let http_client = build_http_client()?;

            app.manage(AppState {
                database,
                http_client,
                binary_bodies: BinaryBodies::default(),
            });
            app.set_menu(build_app_menu(app.handle())?)?;
            Ok(())
        })
        .on_menu_event(|app, event| {
            if let Some(action) = AppMenuAction::from_menu_id(event.id().as_ref()) {
                let _ = action.emit(app);
            }
        })
        .invoke_handler(invoke_handler)
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn build_http_client() -> reqwest::Result<reqwest::Client> {
    // reqwest is built without a bundled crypto provider; use ring, which the
    // updater installs too (whichever runs first wins).
    let _ = rustls::crypto::ring::default_provider().install_default();
    // No overall timeout: slow endpoints and big downloads are normal. These
    // only stop a send hanging forever on an unreachable or silent server.
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(300))
        .build()
}

#[cfg(any(test, not(feature = "mcp-bridge")))]
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
mod http_client_tests {
    #[test]
    fn http_client_builds_with_the_app_tls_setup() {
        super::build_http_client().expect("client should build at startup");
    }
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
