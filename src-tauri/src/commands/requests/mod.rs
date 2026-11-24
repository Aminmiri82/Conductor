mod auth;
mod body;
mod curl;
mod postman_scripts;
mod pretty_json;
mod resolver;
mod response_files;
mod sender;
mod store;

use rusqlite::{Connection, Transaction};

use crate::{
    commands::{
        models::{AuthConfig, RequestDetail},
        variables::{load_variable_context, VariableContext},
    },
    storage::StorageError,
};

pub use curl::request_as_curl;
pub use response_files::{
    discard_responses, get_response_body, save_response_body, ResponseBodies,
};
pub use sender::{resolve_request, send_request};
#[cfg(test)]
pub(crate) use store::SaveRequestResult;
#[cfg(test)]
pub(crate) use store::{add_request, store_request};
pub use store::{
    create_request, duplicate_request, get_request, save_repository_request_as_new, save_request,
};

enum RequestPurpose {
    Preview,
    Send,
    CurlTemplate,
    CurlWithValues,
}

fn request_context(
    connection: &Connection,
    request: &RequestDetail,
    environment_id: Option<&str>,
    purpose: RequestPurpose,
) -> Result<(VariableContext, Option<AuthConfig>), StorageError> {
    let snapshot = connection.unchecked_transaction()?;
    let context = read_request_context(&snapshot, request, environment_id, purpose)?;
    snapshot.commit()?;
    Ok(context)
}

fn read_request_context(
    snapshot: &Transaction<'_>,
    request: &RequestDetail,
    environment_id: Option<&str>,
    purpose: RequestPurpose,
) -> Result<(VariableContext, Option<AuthConfig>), StorageError> {
    let require_current = matches!(
        purpose,
        RequestPurpose::Send | RequestPurpose::CurlWithValues
    );
    let inherited_auth = auth::inherited_auth_for_request(snapshot, request, require_current)?;
    let variables = match purpose {
        RequestPurpose::CurlTemplate => VariableContext::nothing(),
        // A ⌘-copied cURL is meant to run as is, so it gets secrets too.
        RequestPurpose::Send | RequestPurpose::Preview | RequestPurpose::CurlWithValues => {
            load_variable_context(snapshot, request, environment_id)?
        }
    };
    Ok((variables, inherited_auth))
}

/// Stored JSON columns are read leniently: one that no longer parses reads as
/// absent instead of making the request unopenable.
fn parse_json_optional<T: serde::de::DeserializeOwned>(value: &str) -> Option<T> {
    serde_json::from_str(value).ok()
}
