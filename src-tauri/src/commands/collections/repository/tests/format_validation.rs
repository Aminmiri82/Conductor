use super::*;
use crate::commands::{models::VariableTarget, variables::read_variables};

#[test]
fn refresh_reports_a_duplicate_shared_key_and_keeps_its_last_good_value() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let id = add_environment(&mut connection, &link.collection_id, "Development");
    let path = root.0.join(".conductor/environments/development.json");
    let id_on_disk = remote_id(&id);
    fs::write(
        &path,
        format!(
            r#"{{"id":"{id_on_disk}","name":"Development","values":{{"baseUrl":"/original"}}}}"#
        ),
    )
    .unwrap();
    refresh_all(&mut connection).unwrap();

    let duplicate = format!(
        r#"{{"id":"{id_on_disk}","name":"Development","values":{{"baseUrl":"/first","base\u0055rl":"/second"}}}}"#
    );
    fs::write(&path, &duplicate).unwrap();
    let refresh = refresh_all(&mut connection).unwrap();
    let issue = refresh.collections.as_ref().unwrap()[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .as_deref()
        .unwrap();
    assert!(issue.contains("environments/development.json"), "{issue}");
    assert!(issue.contains(r#"duplicate JSON key "baseUrl""#), "{issue}");

    let variables = read_variables(
        &connection,
        &VariableTarget::Environment { environment_id: id },
    )
    .unwrap();
    assert_eq!(variables.len(), 1);
    assert_eq!(variables[0].value, "/original");
    assert_eq!(fs::read_to_string(&path).unwrap(), duplicate);
}

#[test]
fn refresh_names_the_file_with_git_conflict_markers_and_keeps_its_requests() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let request = add(&mut connection, &link.collection_id, "Health", None);
    let path = root.0.join(".conductor/requests/root.json");
    let conflicted = b"{\n  \"requests\": {\n<<<<<<< HEAD\n=======\n>>>>>>> teammate\n  }\n}\n";
    fs::write(&path, conflicted).unwrap();

    let refresh = refresh_all(&mut connection).unwrap();
    let issue = refresh.collections.as_ref().unwrap()[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .as_deref()
        .unwrap();
    assert!(
        issue.contains("Resolve the git conflict in requests/root.json, then refresh."),
        "{issue}"
    );
    assert_eq!(
        cache::revision(&connection, &request.id).unwrap(),
        request.repository_revision
    );
    assert_eq!(fs::read(path).unwrap(), conflicted);
}

#[test]
fn review_regression_credential_headers_require_variable_references() {
    for header in [
        "X-Vault-Token",
        "DD-API-KEY",
        "X-Token",
        "apikey",
        "Authentication",
    ] {
        let request = serde_json::json!({
            "id": Uuid::new_v4().to_string(),
            "name": "Request",
            "method": "GET",
            "url": "/",
            "headers": [{"key": header, "value": "private-default"}],
        });
        let literal = format::parse_request(&serde_json::to_vec(&request).unwrap());
        let error = literal
            .expect_err("literal credentials must be rejected")
            .to_string();
        assert!(error.contains(header), "{error}");
        let mut reference = request;
        reference["headers"][0]["value"] = serde_json::json!("{{token}}");
        format::parse_request(&serde_json::to_vec(&reference).unwrap()).unwrap();
    }
}
