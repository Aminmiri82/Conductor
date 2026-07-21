//! Collection commands.
//!
//! * [`crud`] — list collections, get the tree, rename, delete.
//! * [`postman_import`] — Postman v2 collection import.
//! * [`postman_export`] — export a collection to Postman v2.1 JSON.

mod crud;
mod postman_export;
mod postman_import;

pub use crud::{delete_collection, get_collection_tree, list_collections, rename_collection};
pub use postman_export::export_postman_collection;
pub use postman_import::import_postman_collection;

pub(crate) const SORT_ORDER_STEP: i64 = 1024;
