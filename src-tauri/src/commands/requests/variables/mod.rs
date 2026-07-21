//! Environment/variable commands split into thematic submodules.
//!
//! * [`environments`] – environment CRUD.
//! * [`crud`] – variable list/save.
//! * [`import`] – Postman and .env imports.
//! * [`context`] – variable lookup used by request execution.

mod context;
mod crud;
mod environments;
mod import;
mod sensitive;

pub(super) use context::{load_variable_context, save_script_variable, VariableContext};
pub(crate) use sensitive::looks_sensitive_variable_key;
pub use crud::{list_variables, save_variables};
pub use environments::{
    create_environment, delete_environment, list_environments, rename_environment,
};
pub use import::import_postman_environment;
