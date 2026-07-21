mod collection_tree;
mod history_redaction;
mod postman_scripts;
pub(crate) mod request_auth;
mod request_body;
mod request_sender;
mod request_store;
mod variable_resolver;
mod variables;

pub use collection_tree::{create_folder, delete_node, move_node};
pub use request_sender::{cancel_send_request, resolve_request, send_request};
pub use request_store::{
    create_request, delete_request, duplicate_request, get_request, save_request,
};
pub use variables::{
    create_environment, delete_environment, import_postman_environment, list_environments,
    list_variables, rename_environment, save_variables,
};

use std::fs as std_fs;

use crate::commands::models::SaveTextFileInput;
use crate::commands::{AppError, AppResult};

#[tauri::command]
pub fn save_text_file(input: SaveTextFileInput) -> AppResult<()> {
    std_fs::write(input.path, input.contents).map_err(AppError::from)
}
