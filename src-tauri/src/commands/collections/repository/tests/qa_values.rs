use super::*;
use crate::commands::{
    models::{SharedVariableChange, VariableChange, VariableTarget},
    variables::{read_environments, read_variables},
};

#[test]
fn masking_a_variable_preserves_a_value_edited_on_disk() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let scope = VariableTarget::Collection {
        collection_id: link.collection_id,
    };
    let local = root.0.join(".conductor/local.json");
    fs::write(&local, br#"{"collection":{"token":"original-token"}}"#).unwrap();
    refresh_all(&mut connection).unwrap();

    let baseline = read_variables(&connection, &scope).unwrap();
    let mut row = read_variables(&connection, &scope).unwrap().remove(0);
    assert_eq!(row.value, "original-token");
    row.sensitive = true;
    let change = VariableChange {
        baseline: Some(baseline),
        target: scope.clone(),
        upserts: vec![row],
        deletes: vec![],
    };

    fs::write(&local, br#"{"collection":{"token":"rotated-in-editor"}}"#).unwrap();
    refresh_all(&mut connection).unwrap();
    let target = values::target(&connection, &scope).unwrap().unwrap();
    values::edit(&connection, &[(target, &change)]).unwrap();

    refresh_all(&mut connection).unwrap();
    let stored = read_variables(&connection, &scope).unwrap().remove(0);
    assert_eq!(stored.value, "rotated-in-editor");
    assert!(stored.sensitive);
}

#[test]
fn unrelated_editor_changes_survive_a_variable_save() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let id = link.collection_id;
    let scope = VariableTarget::Collection {
        collection_id: id.clone(),
    };
    let local = root.0.join(".conductor/local.json");
    let manifest = root.0.join(".conductor/collection.json");
    let shared = fs::read(&manifest).unwrap();
    fs::write(&local, br#"{"collection":{"token":"original-token"}}"#).unwrap();
    refresh_all(&mut connection).unwrap();
    let mut row = read_variables(&connection, &scope).unwrap().remove(0);
    row.value = "token-from-app".into();
    let change = VariableChange {
        baseline: None,
        target: scope.clone(),
        upserts: vec![row],
        deletes: vec![],
    };
    let edited = r#""neighbor" : "written-in-editor""#;
    fs::write(
        &local,
        format!(r#"{{"collection":{{"token":"original-token",{edited}}}}}"#),
    )
    .unwrap();
    let target = values::target(&connection, &scope).unwrap().unwrap();
    values::edit(&connection, &[(target, &change)]).unwrap();

    assert!(fs::read_to_string(&local).unwrap().contains(edited));
    assert_eq!(
        private_token(&connection, &id).as_deref(),
        Some("token-from-app")
    );
    assert_eq!(fs::read(&manifest).unwrap(), shared);
}

#[test]
fn an_invalid_local_file_keeps_cached_private_values_and_blocks_saves() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let id = add_environment(&mut connection, &link.collection_id, "Development");
    let local = root.0.join(".conductor/local.json");
    fs::write(
        &local,
        br#"{"environments":{"development":{"token":"last-valid-token"}}}"#,
    )
    .unwrap();
    refresh_all(&mut connection).unwrap();
    let invalid = br#"{"environments":{"development":{"token":"unfinished"#;
    fs::write(&local, invalid).unwrap();
    let refresh = refresh_all(&mut connection).unwrap();

    let issue = refresh.collections[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .as_deref()
        .unwrap();
    assert!(issue.contains("invalid local values"));
    assert!(!issue.contains("unfinished"));
    assert_eq!(
        private_token(&connection, &id).as_deref(),
        Some("last-valid-token")
    );
    let target = values::target(
        &connection,
        &VariableTarget::Environment { environment_id: id },
    )
    .unwrap()
    .unwrap();
    let error =
        values::set_script_values(&connection, &[(target, "token", "app-write")]).unwrap_err();
    assert!(error.to_string().contains("invalid local values"));
    assert_eq!(fs::read(local).unwrap(), invalid);
}

#[test]
fn reopening_a_removed_link_restores_its_private_environment_values() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let id = add_environment(&mut connection, &link.collection_id, "Development");
    let local = root.0.join(".conductor/local.json");
    let bytes = br#"{"environments":{"development":{"token":"private-token"}}}"#;
    fs::write(&local, bytes).unwrap();
    refresh_all(&mut connection).unwrap();
    assert_eq!(
        private_token(&connection, &id).as_deref(),
        Some("private-token")
    );

    super::super::super::remove_collection(&connection, &link.collection_id).unwrap();
    let reopened = connect(&mut connection, &root.0, false, None).unwrap();
    let environments = read_environments(&connection).unwrap();
    assert_eq!(environments.len(), 1);
    let environment = &environments[0];
    assert_eq!(environment.collection_id, reopened.collection_id);
    assert_eq!(
        private_token(&connection, &environment.id).as_deref(),
        Some("private-token")
    );
    assert_eq!(fs::read(local).unwrap(), bytes);
}

#[test]
fn a_failed_cross_repository_transfer_keeps_the_token_at_both_ends_for_retry() {
    let source_root = Scratch::new();
    let destination_root = Scratch::new();
    git_init(&source_root.0);
    git_init(&destination_root.0);
    let mut connection = test_connection();
    let source = connect(&mut connection, &source_root.0, true, None).unwrap();
    let destination = connect(&mut connection, &destination_root.0, true, None).unwrap();
    let source_scope = VariableTarget::Collection {
        collection_id: source.collection_id,
    };
    let destination_scope = VariableTarget::Collection {
        collection_id: destination.collection_id,
    };
    let source_local = source_root.0.join(".conductor/local.json");
    let destination_local = destination_root.0.join(".conductor/local.json");
    fs::write(&source_local, QA_PRIVATE_BYTES).unwrap();
    refresh_all(&mut connection).unwrap();
    let original = read_variables(&connection, &source_scope)
        .unwrap()
        .remove(0);
    let remove = VariableChange {
        baseline: None,
        target: source_scope.clone(),
        deletes: vec![original.key.clone()],
        upserts: vec![],
    };
    let insert = VariableChange {
        baseline: None,
        target: destination_scope.clone(),
        deletes: vec![],
        upserts: vec![original],
    };
    let source_ignore = source_root.0.join(".conductor/.gitignore");
    let ignore = fs::read(&source_ignore).unwrap();
    fs::write(&source_ignore, [&ignore[..], b"!/local.json\n"].concat()).unwrap();
    let transfer = |connection: &Connection| {
        values::edit(
            connection,
            &[
                (
                    values::target(connection, &source_scope).unwrap().unwrap(),
                    &remove,
                ),
                (
                    values::target(connection, &destination_scope)
                        .unwrap()
                        .unwrap(),
                    &insert,
                ),
            ],
        )
    };

    let error = transfer(&connection).unwrap_err();
    assert!(error.to_string().contains("ignored by Git and untracked"));
    assert_eq!(fs::read(&source_local).unwrap(), QA_PRIVATE_BYTES);
    let copied = fs::read_to_string(&destination_local).unwrap();
    assert!(copied.contains("qa-private-atomic-write-token"));
    assert!(!unignored_files(&destination_root.0).contains("local.json"));

    fs::write(&source_ignore, ignore).unwrap();
    transfer(&connection).unwrap();
    assert!(!fs::read_to_string(source_local)
        .unwrap()
        .contains("qa-private-atomic-write-token"));
    assert_eq!(fs::read_to_string(destination_local).unwrap(), copied);
}

#[test]
fn editing_shared_values_updates_the_selected_file_and_keeps_local_overrides() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let environment_id = add_environment(&mut connection, &link.collection_id, "Development");
    let environment = VariableTarget::Environment { environment_id };
    let collection = VariableTarget::Collection {
        collection_id: link.collection_id,
    };
    let conductor = root.0.join(".conductor");
    let local_bytes = br#"{"environments":{"development":{"baseUrl":"personal-url"}}}"#;
    fs::write(conductor.join("local.json"), local_bytes).unwrap();

    for (scope, file) in [
        (&collection, "collection.json"),
        (&environment, "environments/development.json"),
    ] {
        let file = conductor.join(file);
        let initial = fs::read_to_string(&file).unwrap();
        let initial = initial.trim_end().strip_suffix('}').unwrap();
        let original = format!("{initial},\n  \"values\": {{ \"baseUrl\" : \"team-url\", \"neighbor\" : \"keep\" }}\n}}\n");
        fs::write(&file, &original).unwrap();
        refresh_all(&mut connection).unwrap();
        let target = values::target(&connection, scope).unwrap().unwrap();
        let previous = values::read_shared(&connection, &target, "baseUrl").unwrap();
        assert_eq!(previous, "team-url");

        values::edit_shared(
            &connection,
            &target,
            "baseUrl",
            SharedVariableChange::Value {
                previous: previous.clone(),
                value: "new-team-url".into(),
            },
        )
        .unwrap();
        let saved = fs::read_to_string(&file).unwrap();
        assert_eq!(saved, original.replace("team-url", "new-team-url"));
        let error = values::edit_shared(
            &connection,
            &target,
            "baseUrl",
            SharedVariableChange::Value {
                previous,
                value: "stale-editor-url".into(),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("changed on disk"));
        assert_eq!(fs::read_to_string(&file).unwrap(), saved);
    }

    refresh_all(&mut connection).unwrap();
    assert_eq!(
        read_variables(&connection, &collection).unwrap()[0].value,
        "new-team-url"
    );
    assert_eq!(
        read_variables(&connection, &environment).unwrap()[0].value,
        "personal-url"
    );
    assert_eq!(fs::read(conductor.join("local.json")).unwrap(), local_bytes);
}

#[test]
fn declaring_a_required_secret_writes_only_its_name_and_rejects_a_shared_value() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let scope = VariableTarget::Collection {
        collection_id: link.collection_id,
    };
    let conductor = root.0.join(".conductor");
    let target = values::target(&connection, &scope).unwrap().unwrap();
    values::edit_shared(
        &connection,
        &target,
        "apiToken",
        SharedVariableChange::RequiredSecret,
    )
    .unwrap();
    let path = conductor.join("collection.json");
    let bytes = fs::read(&path).unwrap();
    let manifest = format::parse_manifest(&bytes).unwrap();
    assert!(manifest.secrets.contains("apiToken"));
    assert!(!manifest.values.contains_key("apiToken"));
    assert!(!conductor.join("local.json").exists());

    let error = values::edit_shared(
        &connection,
        &target,
        "apiToken",
        SharedVariableChange::Value {
            previous: String::new(),
            value: "private-token".into(),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("can only have a local value"));
    assert_eq!(fs::read(path).unwrap(), bytes);

    refresh_all(&mut connection).unwrap();
    let row = read_variables(&connection, &scope).unwrap().remove(0);
    assert_eq!(row.key, "apiToken");
    assert!(row.value.is_empty() && row.sensitive && !row.enabled);
}
