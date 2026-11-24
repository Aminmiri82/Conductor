use super::*;
mod format_validation;
mod qa_requests;
mod qa_values;
use crate::{
    commands::{
        collections::{add_folder, tree::reparent_node},
        models::{
            CollectionNodeKind, CreateFolderInput, CreateRequestInput, MoveNodeInput, RequestDetail,
        },
        requests::{add_request, store_request, SaveRequestResult},
    },
    fixtures,
    storage::test_connection,
};

pub(super) struct Scratch(pub(super) PathBuf);

impl Scratch {
    pub(super) fn new() -> Self {
        let path = std::env::temp_dir().join(format!("conductor-repo-test-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn add(connection: &mut Connection, id: &str, name: &str, parent: Option<String>) -> RequestDetail {
    let created = mutate(connection, id, |connection| {
        add_request(
            connection,
            &CreateRequestInput {
                collection_id: id.into(),
                name: name.into(),
                parent_id: parent,
                position: 99,
            },
        )
    })
    .unwrap();
    RequestDetail {
        repository_revision: cache::revision(connection, &created.request_id).unwrap(),
        id: created.request_id,
        collection_id: id.into(),
        name: name.into(),
        method: "GET".into(),
        ..Default::default()
    }
}

fn hand_edit(path: &Path, local_id: &str, url: &str) {
    let before = fs::read(path).unwrap();
    let parsed = group::parse(&before).unwrap();
    let id = remote_id(local_id);
    let mut request = parsed.requests[id].as_ref().unwrap().clone();
    request.url = url.into();
    let bytes = group::edit(
        Some(&before),
        parsed.folder_id.as_deref(),
        &BTreeMap::from([(id.into(), Some(&request))]),
    )
    .unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn saving_a_request_keeps_edits_to_other_requests_and_conflicts_on_the_same_request() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let mut first = add(&mut connection, &link.collection_id, "Users", None);
    let second = add(&mut connection, &link.collection_id, "Orders", None);
    let path = root.0.join(".conductor/requests/root.json");
    hand_edit(&path, &second.id, "/external-orders");

    first.url = "/local-users".into();
    let result = store_request(&mut connection, first.clone()).unwrap();
    let SaveRequestResult::Saved {
        revision,
        refresh: Some(refresh),
    } = result
    else {
        panic!("unrelated edits can coexist")
    };
    first.repository_revision = revision;
    let after = group::parse(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        after.requests[remote_id(&first.id)].as_ref().unwrap().url,
        "/local-users"
    );
    assert_eq!(
        after.requests[remote_id(&second.id)].as_ref().unwrap().url,
        "/external-orders"
    );
    assert_eq!(refresh.changed_requests[0].request_id, second.id);

    hand_edit(&path, &first.id, "/external-users");
    first.url = "/my-draft".into();
    let result = store_request(&mut connection, first.clone()).unwrap();
    let SaveRequestResult::Conflict { revision, .. } = result else {
        panic!("external edit must conflict")
    };
    first.repository_revision = revision;
    hand_edit(&path, &first.id, "/external-users-again");
    assert!(matches!(
        store_request(&mut connection, first).unwrap(),
        SaveRequestResult::Conflict { .. }
    ));
}

#[test]
fn refresh_loads_valid_siblings_and_prunes_confirmed_removals() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let bad = add(&mut connection, &link.collection_id, "Bad input", None);
    let good = add(&mut connection, &link.collection_id, "Valid", None);
    let removed = add(&mut connection, &link.collection_id, "Removed", None);
    let path = root.0.join(".conductor/requests/root.json");
    let old = fs::read(&path).unwrap();
    fs::write(&path, format!(r#"{{"requests":{{"bad-input":{{"id":"{}","name":false}},"valid":{{"id":"{}","name":"Updated","method":"GET","url":"/hand-edited"}}}}}}"#,
        remote_id(&bad.id), remote_id(&good.id))).unwrap();
    let refreshed = refresh_all(&mut connection).unwrap();
    let issue = refreshed.collections.as_ref().unwrap()[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .as_ref()
        .unwrap();
    assert!(issue.contains("requests/root.json"));
    assert!(issue.contains(remote_id(&bad.id)));
    assert!(refreshed
        .changed_requests
        .iter()
        .any(|change| change.request_id == removed.id && change.revision.is_none()));
    assert_eq!(
        cache::revision(&connection, &bad.id).unwrap(),
        bad.repository_revision
    );
    assert_ne!(
        cache::revision(&connection, &good.id).unwrap(),
        good.repository_revision
    );

    fs::write(&path, b"{").unwrap();
    assert!(refresh_all(&mut connection)
        .unwrap()
        .changed_requests
        .is_empty());
    assert_eq!(
        read_tree(&connection, &link.collection_id).unwrap().len(),
        2
    );
    fs::write(&path, old).unwrap();
    assert_eq!(
        refresh_all(&mut connection).unwrap().changed_requests.len(),
        2
    );
}

#[test]
fn moves_keep_request_identity_and_relink_preserves_local_values() {
    let scratch = Scratch::new();
    let root = scratch.0.join("before");
    let moved = scratch.0.join("after");
    fs::create_dir(&root).unwrap();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root, true, None).unwrap();
    let id = &link.collection_id;
    let folder = mutate(&mut connection, id, |connection| {
        add_folder(
            connection,
            &CreateFolderInput {
                collection_id: id.clone(),
                parent_id: None,
                name: "../../users".into(),
                position: 0,
            },
        )
    })
    .unwrap();
    let admin = mutate(&mut connection, id, |connection| {
        add_folder(
            connection,
            &CreateFolderInput {
                collection_id: id.clone(),
                parent_id: Some(folder.clone()),
                name: "Admin".into(),
                position: 0,
            },
        )
    })
    .unwrap();
    let mut request = add(&mut connection, id, "Health", Some(admin));
    let requests = root.join(".conductor/requests");
    assert!(requests.join("users/admin.json").is_file());
    request.name = "Ping".into();
    let SaveRequestResult::Saved { revision, .. } =
        store_request(&mut connection, request.clone()).unwrap()
    else {
        panic!("renaming must save");
    };
    request.repository_revision = revision;
    fs::rename(requests.join("users.json"), requests.join("members.json")).unwrap();
    fs::rename(requests.join("users"), requests.join("members")).unwrap();
    assert!(refresh_all(&mut connection)
        .unwrap()
        .changed_requests
        .is_empty());
    mutate(&mut connection, id, |connection| {
        reparent_node(
            connection,
            &MoveNodeInput {
                node_id: request.id.clone(),
                parent_id: None,
                position: 99,
            },
        )
    })
    .unwrap();
    let group = fs::read(root.join(".conductor/requests/root.json")).unwrap();
    assert_eq!(
        group::key_for(&group, remote_id(&request.id))
            .unwrap()
            .as_deref(),
        Some("health")
    );
    assert_eq!(
        group::parse(&group).unwrap().requests[remote_id(&request.id)]
            .as_ref()
            .unwrap()
            .name,
        "Ping"
    );
    request.url = "/open-draft".into();
    assert!(matches!(
        store_request(&mut connection, request).unwrap(),
        SaveRequestResult::Saved { .. }
    ));
    fs::write(
        root.join(".conductor/local.json"),
        br#"{"collection":{"token":{"value":"private-token","sensitive":true}}}"#,
    )
    .unwrap();
    fs::rename(&root, &moved).unwrap();
    let refresh = refresh_all(&mut connection).unwrap();
    assert!(refresh.collections.as_ref().unwrap()[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .is_some());
    assert_eq!(read_tree(&connection, id).unwrap().len(), 2);
    let collections = relocate(&mut connection, id, &moved).unwrap();
    assert!(collections[0].repository.as_ref().unwrap().issue.is_none());
    refresh_all(&mut connection).unwrap();
    let value: String = connection
        .query_row(
            "SELECT value FROM repository_variables WHERE collection_id = ? AND scope_id = collection_id",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(value, "private-token");
}

#[test]
fn missing_identity_retains_the_last_good_group_and_blocks_replacement() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let folder = mutate(&mut connection, &link.collection_id, |connection| {
        add_folder(
            connection,
            &CreateFolderInput {
                collection_id: link.collection_id.clone(),
                parent_id: None,
                name: "Operations".into(),
                position: 0,
            },
        )
    })
    .unwrap();
    let request = add(
        &mut connection,
        &link.collection_id,
        "Health",
        Some(folder.clone()),
    );
    let path = root.0.join(".conductor/requests/operations.json");
    fs::write(
        &path,
        format!(r#"{{"folderId":"{}","requests":{{"health":{{"name":"Health","method":"GET","url":"/"}}}}}}"#, remote_id(&folder)),
    )
    .unwrap();

    let refreshed = refresh_all(&mut connection).unwrap();
    assert!(refreshed.changed_requests.is_empty());
    assert_eq!(
        cache::revision(&connection, &request.id).unwrap(),
        request.repository_revision
    );
    let mut other_connection = test_connection();
    let other = connect(&mut other_connection, &root.0, false, None).unwrap();
    assert!(other.collections[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .as_ref()
        .unwrap()
        .contains("health has no usable ID"));

    let result = mutate(&mut other_connection, &other.collection_id, |connection| {
        add_request(
            connection,
            &CreateRequestInput {
                collection_id: other.collection_id.clone(),
                name: "Other".into(),
                parent_id: Some(folder_id(&other.collection_id, remote_id(&folder))),
                position: 99,
            },
        )
    });
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("health has no usable ID"));
    assert!(!root
        .0
        .join(".conductor/requests/operations-2.json")
        .exists());
}

#[test]
fn review_regression_publishing_keeps_secret_named_defaults_out_of_the_shared_manifest() {
    use crate::commands::models::VariableTarget;

    let root = Scratch::new();
    let mut connection = test_connection();
    let source = fixtures::collection(&connection);
    let target = VariableTarget::Collection {
        collection_id: source.clone(),
    };
    let keys = [
        "SECRET_KEY",
        "AWS_SECRET_ACCESS_KEY",
        "accessKey",
        "apiKeyProd",
        "jwt",
        "passwd",
    ];
    for key in keys {
        fixtures::variable(&connection, &target, key, "private-default");
    }
    fixtures::variable(&connection, &target, "host", "example.test");
    let preview = publish_preview(&connection, &source).unwrap();
    connect(&mut connection, &root.0, true, Some((&source, &preview))).unwrap();

    let manifest =
        format::parse_manifest(&fs::read(root.0.join(".conductor/collection.json")).unwrap())
            .unwrap();
    assert_eq!(
        manifest.secrets,
        keys.into_iter().map(str::to_owned).collect()
    );
    assert_eq!(
        manifest.values,
        BTreeMap::from([("host".into(), "example.test".into())])
    );
}

#[test]
fn review_regression_publish_preview_keeps_user_selected_keys_private() {
    use crate::commands::models::VariableTarget;

    let root = Scratch::new();
    let mut connection = test_connection();
    let source = fixtures::collection(&connection);
    let target = VariableTarget::Collection {
        collection_id: source.clone(),
    };
    let env_id = fixtures::environment(&connection, &source);
    let environment = VariableTarget::Environment {
        environment_id: env_id.clone(),
    };
    fixtures::variable(&connection, &target, "host", "private-host");
    fixtures::variable(&connection, &target, "apiKeyProd", "private-key");
    fixtures::variable(&connection, &environment, "host", "shared-env-host");

    let mut preview = publish_preview(&connection, &source).unwrap();
    assert_eq!(preview[0].environment_id, None);
    assert!(preview[0].shared_keys.remove("host"));
    assert!(preview[0].secret_keys.contains("apiKeyProd"));
    preview[0].secret_keys.insert("host".into());
    assert_eq!(preview[1].environment_id.as_deref(), Some(env_id.as_str()));
    assert!(preview[1].shared_keys.contains("host"));
    assert!(!root.0.join(".conductor").exists());

    connect(&mut connection, &root.0, true, Some((&source, &preview))).unwrap();
    let manifest =
        format::parse_manifest(&fs::read(root.0.join(".conductor/collection.json")).unwrap())
            .unwrap();
    assert!(manifest.values.is_empty());
    assert_eq!(manifest.secrets, preview[0].secret_keys);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &fs::read(root.0.join(".conductor/local.json")).unwrap()
        )
        .unwrap()["collection"]["host"]["value"],
        "private-host",
    );
}

#[test]
fn review_regression_a_stale_publish_preview_requires_review_before_writing() {
    use crate::commands::models::VariableTarget;

    let root = Scratch::new();
    let mut connection = test_connection();
    let source = fixtures::collection(&connection);
    let preview = publish_preview(&connection, &source).unwrap();
    fixtures::variable(
        &connection,
        &VariableTarget::Collection {
            collection_id: source.clone(),
        },
        "host",
        "new-default",
    );

    let error = connect(&mut connection, &root.0, true, Some((&source, &preview))).unwrap_err();
    assert!(error.to_string().contains("Review them again"));
    assert!(fs::read_dir(&root.0).unwrap().next().is_none());
}

#[test]
fn publishing_shares_public_values_and_keeps_private_values_in_local_json() {
    let root = Scratch::new();
    git_init(&root.0);
    let mut connection = test_connection();
    let source = fixtures::collection(&connection);
    let environment = fixtures::environment(&connection, &source);
    connection.execute(
        "INSERT INTO variables (scope,environment_id,key,current_value,sensitive,created_at,updated_at)
         VALUES ('environment',?,'token','private-token',1,'',''), ('environment',?,'host','current-host',0,'','')",
        [&environment, &environment],
    ).unwrap();
    let preview = publish_preview(&connection, &source).unwrap();
    let link = connect(&mut connection, &root.0, true, Some((&source, &preview))).unwrap();
    let path = format::list(&root.0, "environments")
        .unwrap()
        .pop()
        .unwrap();
    let shared =
        format::parse_environment(&format::read(&root.0, &path).unwrap().unwrap()).unwrap();
    assert_eq!(
        shared.values.get("host").map(String::as_str),
        Some("current-host")
    );
    assert!(shared.secrets.contains("token"));
    assert!(!shared.values.contains_key("token"));
    let private = fs::read_to_string(root.0.join(".conductor/local.json")).unwrap();
    assert!(private.contains("private-token"));
    let local: serde_json::Value = serde_json::from_str(&private).unwrap();
    let stem = Path::new(&path).file_stem().unwrap().to_str().unwrap();
    assert_eq!(
        local["environments"][stem]["token"]["value"],
        "private-token"
    );
    assert!(!private.contains("current-host"));
    let values: String = connection
        .query_row(
            "SELECT group_concat(value, ',') FROM repository_variables WHERE collection_id = ?",
            [&link.collection_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(values.contains("private-token"));
    assert!(values.contains("current-host"));
}

fn git_init(root: &Path) {
    assert!(std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status()
        .unwrap()
        .success());
}

fn unignored_files(root: &Path) -> String {
    let output = std::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn review_regression_rejects_private_writes_to_a_tracked_file_in_a_nested_checkout() {
    let root = Scratch::new();
    let mut connection = test_connection();
    connect(&mut connection, &root.0, true, None).unwrap();
    let directory = root.0.join(".conductor");
    git_init(&directory);
    let placeholder = b"{}";
    let path = directory.join("local.json");
    fs::write(&path, placeholder).unwrap();
    assert!(std::process::Command::new("git")
        .args(["add", "-f", "--", "local.json"])
        .current_dir(&directory)
        .status()
        .unwrap()
        .success());

    let selected = canonical_root(directory.to_str().unwrap()).unwrap();
    let local = serde_json::from_slice(QA_PRIVATE_BYTES).unwrap();
    let error = values::write_local(&selected, &local).unwrap_err();
    assert!(error
        .to_string()
        .contains(".conductor is itself a Git checkout"));
    assert_eq!(fs::read(path).unwrap(), placeholder);
}

#[test]
fn review_regression_git_hides_shared_write_and_publish_staging_leftovers() {
    let root = Scratch::new();
    git_init(&root.0);
    connect(&mut test_connection(), &root.0, true, None).unwrap();
    let group = root.0.join(".conductor/requests/nested");
    fs::create_dir_all(&group).unwrap();
    fs::write(
        group.join(format!(".conductor-{}.tmp", Uuid::new_v4())),
        b"{}",
    )
    .unwrap();
    fs::write(
        group.join(format!(
            ".group.json.conductor-deleted-{}.tmp",
            Uuid::new_v4()
        )),
        b"{}",
    )
    .unwrap();
    let staging = Publish::new(&root.0).unwrap();
    fs::write(staging.path.join(".conductor/collection.json"), b"{}").unwrap();

    let visible = unignored_files(&root.0);
    assert!(!visible.contains(".conductor-"), "{visible}");
}

const QA_INTERRUPTED_LOCAL: &str =
    ".conductor/.local.json-2cace730-9dac-4381-b33d-2da1c4215d8e.tmp";
const QA_PRIVATE_BYTES: &[u8] = br#"{"collection":{"token":"qa-private-atomic-write-token"}}"#;

#[test]
fn qa_interrupted_initial_private_write_is_ignored_while_rules_are_present() {
    let root = Scratch::new();
    git_init(&root.0);
    let mut connection = test_connection();
    connect(&mut connection, &root.0, true, None).unwrap();
    // Exiting after writing and syncing the first temporary file can leave it
    // on disk before the local.json name is installed.
    fs::write(root.0.join(QA_INTERRUPTED_LOCAL), QA_PRIVATE_BYTES).unwrap();
    assert!(!root.0.join(".conductor/local.json").exists());
    assert!(!unignored_files(&root.0).contains(QA_INTERRUPTED_LOCAL));
}

#[test]
fn qa_private_write_rejects_ignore_rules_that_reinclude_temporary_files() {
    let root = Scratch::new();
    git_init(&root.0);
    let mut connection = test_connection();
    connect(&mut connection, &root.0, true, None).unwrap();
    fs::write(
        root.0.join(".conductor/.gitignore"),
        b"/local.json\n/.local.json-*.tmp\n!/.local.json-*.tmp\n",
    )
    .unwrap();
    let local = serde_json::from_slice(QA_PRIVATE_BYTES).unwrap();
    let error = values::write_local(&root.0, &local).unwrap_err();
    assert!(error.to_string().contains("ignored by Git and untracked"));
    assert!(!root.0.join(".conductor/local.json").exists());
    assert!(!unignored_files(&root.0).contains(".local.json-"));
}

#[test]
fn qa_request_save_rename_move_and_refresh_keep_the_private_token_local() {
    let root = Scratch::new();
    git_init(&root.0);
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let collection = &link.collection_id;
    let folder = add_users_folder(&mut connection, collection);
    let mut request = add(&mut connection, collection, "Fetch", None);
    let local = serde_json::from_slice(QA_PRIVATE_BYTES).unwrap();
    values::write_local(&root.0, &local).unwrap();
    let private_path = root.0.join(".conductor/local.json");
    let original = fs::read(&private_path).unwrap();
    let assert_private = || {
        assert_eq!(fs::read(&private_path).unwrap(), original);
        for relative in unignored_files(&root.0).lines() {
            assert!(
                !fs::read_to_string(root.0.join(relative))
                    .unwrap()
                    .contains("qa-private-atomic-write-token"),
                "{relative} contains the token from local.json"
            );
        }
    };

    request.auth = Some(
        serde_json::from_value(serde_json::json!({"authType":"bearer","token":"{{token}}"}))
            .unwrap(),
    );
    let SaveRequestResult::Saved { revision, .. } =
        store_request(&mut connection, request.clone()).unwrap()
    else {
        panic!("saving an auth reference must succeed");
    };
    assert_private();

    request.repository_revision = revision;
    request.name = "Fetch renamed".into();
    assert!(matches!(
        store_request(&mut connection, request.clone()).unwrap(),
        SaveRequestResult::Saved { .. }
    ));
    assert_private();

    mutate(&mut connection, collection, |connection| {
        reparent_node(
            connection,
            &MoveNodeInput {
                node_id: request.id,
                parent_id: Some(folder),
                position: 0,
            },
        )
    })
    .unwrap();
    assert_private();
    assert!(
        fs::read_to_string(root.0.join(".conductor/requests/users.json"))
            .unwrap()
            .contains("{{token}}")
    );
    refresh_all(&mut connection).unwrap();
    assert_private();
}

#[cfg(unix)]
#[test]
fn qa_private_write_rejects_local_json_symlink_without_touching_its_target() {
    let root = Scratch::new();
    git_init(&root.0);
    let mut connection = test_connection();
    connect(&mut connection, &root.0, true, None).unwrap();
    let target = root.0.join("shared.json");
    fs::write(&target, b"{}").unwrap();
    let link = root.0.join(".conductor/local.json");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let local = serde_json::from_slice(QA_PRIVATE_BYTES).unwrap();
    let error = values::write_local(&root.0, &local).unwrap_err();
    assert!(error.to_string().contains("must be a regular file"));
    assert_eq!(fs::read_link(link).unwrap(), target);
    assert_eq!(fs::read(target).unwrap(), b"{}");
}

#[test]
fn qa_private_write_without_git_uses_safe_rules_or_refuses() {
    const CHILD: &str = "CONDUCTOR_QA_WITHOUT_GIT";
    if std::env::var_os(CHILD).is_some() {
        let root = Scratch::new();
        let mut connection = test_connection();
        connect(&mut connection, &root.0, true, None).unwrap();
        let local = serde_json::from_slice(QA_PRIVATE_BYTES).unwrap();
        values::write_local(&root.0, &local).unwrap();
        let path = root.0.join(".conductor/local.json");
        assert!(fs::read_to_string(&path)
            .unwrap()
            .contains("qa-private-atomic-write-token"));
        fs::remove_file(&path).unwrap();
        fs::create_dir(root.0.join(".git")).unwrap();
        let error = values::write_local(&root.0, &local).unwrap_err();
        assert!(error.to_string().contains("ignored by Git and untracked"));
        assert!(!path.exists());
        return;
    }

    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("qa_private_write_without_git_uses_safe_rules_or_refuses")
        .env(CHILD, "1")
        .env("PATH", "")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn add_users_folder(connection: &mut Connection, collection_id: &str) -> String {
    mutate(connection, collection_id, |connection| {
        add_folder(
            connection,
            &CreateFolderInput {
                collection_id: collection_id.into(),
                parent_id: None,
                name: "Users".into(),
                position: 0,
            },
        )
    })
    .unwrap()
}

fn duplicate_into_folder(root: &Path, request_id: &str, folder: &str, moves: u32) {
    let bytes = fs::read(root.join(".conductor/requests/root.json")).unwrap();
    let mut request = group::parse(&bytes).unwrap().requests[remote_id(request_id)]
        .as_ref()
        .unwrap()
        .clone();
    request.move_generation = moves;
    request.parent_id = Some(remote_id(folder).into());
    let path = root.join(".conductor/requests/users.json");
    let old = fs::read(&path).unwrap();
    let next = group::edit(
        Some(&old),
        Some(remote_id(folder)),
        &BTreeMap::from([(request.id.clone(), Some(&request))]),
    )
    .unwrap();
    fs::write(path, next).unwrap();
}

#[test]
fn refresh_and_relink_keep_the_destination_after_an_interrupted_move() {
    let scratch = Scratch::new();
    let root = scratch.0.join("before");
    fs::create_dir(&root).unwrap();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root, true, None).unwrap();
    let folder = add_users_folder(&mut connection, &link.collection_id);
    let request = add(&mut connection, &link.collection_id, "List users", None);
    duplicate_into_folder(&root, &request.id, &folder, 1);

    let renamed = scratch.0.join("renamed");
    fs::rename(&root, &renamed).unwrap();
    let collections = relocate(&mut connection, &link.collection_id, &renamed).unwrap();
    refresh_all(&mut connection).unwrap();
    let issue = collections[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .as_ref()
        .unwrap();
    assert!(issue.contains("Duplicate request ID (using requests/users.json)"));
    assert_eq!(
        cache::revision(&connection, &request.id).unwrap(),
        request.repository_revision
    );
    let reopened = connect(&mut test_connection(), &renamed, false, None).unwrap();
    let [node] = reopened.tree.as_slice() else {
        panic!("one root folder expected");
    };
    let CollectionNodeKind::Folder { children } = &node.kind else {
        panic!("Users folder expected");
    };
    assert_eq!(children[0].name, "List users");
    let source = fs::read_to_string(renamed.join(".conductor/requests/root.json")).unwrap();
    assert!(
        source.contains("list-users"),
        "refresh must preserve external bytes"
    );
}

#[test]
fn saving_reports_a_duplicate_added_to_another_group() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let folder = add_users_folder(&mut connection, &link.collection_id);
    let mut request = add(&mut connection, &link.collection_id, "List users", None);
    duplicate_into_folder(&root.0, &request.id, &folder, 0);
    request.url = "/draft-change".into();
    let outcome = store_request(&mut connection, request.clone()).unwrap();
    assert!(matches!(outcome, SaveRequestResult::Conflict { .. }));

    let path = root.0.join(".conductor/requests/users.json");
    let mut group: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    group["requests"]["list-users"]["url"] = serde_json::Value::Bool(false);
    fs::write(path, serde_json::to_vec_pretty(&group).unwrap()).unwrap();
    let outcome = store_request(&mut connection, request).unwrap();
    assert!(matches!(outcome, SaveRequestResult::Conflict { .. }));
}

#[test]
fn publishing_cleans_up_abandoned_staging() {
    let root = Scratch::new();
    let staging = Publish::new(&root.0).unwrap();
    let abandoned = staging.path.clone();
    fs::write(abandoned.join(".conductor/collection.json"), b"{}").unwrap();
    let mut staging = staging;
    drop(staging.marker.take());
    std::mem::forget(staging);
    connect(&mut test_connection(), &root.0, true, None).unwrap();
    assert!(!abandoned.exists());
}

#[test]
fn a_badly_named_group_is_reported_without_blocking_valid_edits() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let mut request = add(&mut connection, &link.collection_id, "Ping", None);
    fs::write(root.0.join(".conductor/requests/Billing API.json"), b"{}").unwrap();
    request.url = "/still-saves".into();
    let SaveRequestResult::Saved {
        refresh: Some(refresh),
        ..
    } = store_request(&mut connection, request).unwrap()
    else {
        panic!("valid group should remain editable");
    };
    assert!(refresh.collections.as_ref().unwrap()[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .as_ref()
        .unwrap()
        .contains("requests/Billing API.json"));
}

#[test]
fn placeholder_names_rekey_on_first_rename_and_moves_suffix_collisions() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let id = &link.collection_id;
    let folder = mutate(&mut connection, id, |connection| {
        add_folder(
            connection,
            &CreateFolderInput {
                collection_id: id.clone(),
                name: "New folder".into(),
                parent_id: None,
                position: 0,
            },
        )
    })
    .unwrap();
    add(&mut connection, id, "Ping", Some(folder.clone()));
    mutate(&mut connection, id, |connection| {
        connection.execute(
            "UPDATE collection_nodes SET name = 'Health' WHERE id = ?",
            [&folder],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(!root
        .0
        .join(format!(
            ".conductor/requests/new-folder-{}.json",
            remote_id(&folder)
        ))
        .exists());
    let mut request = add(&mut connection, id, "New request", None);
    request.name = "Ping".into();
    assert!(matches!(
        store_request(&mut connection, request.clone()).unwrap(),
        SaveRequestResult::Saved { .. }
    ));
    let root_group = fs::read(root.0.join(".conductor/requests/root.json")).unwrap();
    assert_eq!(
        group::key_for(&root_group, remote_id(&request.id))
            .unwrap()
            .as_deref(),
        Some("ping")
    );
    mutate(&mut connection, id, |connection| {
        reparent_node(
            connection,
            &MoveNodeInput {
                node_id: request.id.clone(),
                parent_id: Some(folder),
                position: 99,
            },
        )
    })
    .unwrap();
    let group = fs::read(root.0.join(".conductor/requests/health.json")).unwrap();
    assert_eq!(
        group::key_for(&group, remote_id(&request.id))
            .unwrap()
            .as_deref(),
        Some("ping-2")
    );
}

#[test]
fn adding_to_a_folder_keeps_a_group_created_on_disk_since_the_last_refresh() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let folder = add_users_folder(&mut connection, &link.collection_id);
    let remote_folder = remote_id(&folder);
    let requests = root.0.join(".conductor/requests");
    fs::remove_file(requests.join("users.json")).unwrap();
    refresh_all(&mut connection).unwrap();

    let external = Uuid::new_v4();
    fs::write(
        requests.join("previous-name.json"),
        format!(
            r#"{{"folderId":"{remote_folder}","requests":{{"external":{{"id":"{external}","name":"External","method":"GET","url":"/external"}}}}}}"#
        ),
    )
    .unwrap();
    let created = mutate(&mut connection, &link.collection_id, |connection| {
        add_request(
            connection,
            &CreateRequestInput {
                collection_id: link.collection_id.clone(),
                name: "Local".into(),
                parent_id: Some(folder),
                position: 0,
            },
        )
    });

    let reopened = connect(&mut test_connection(), &root.0, false, None).unwrap();
    let CollectionNodeKind::Folder { children } = &reopened.tree[0].kind else {
        panic!("the Users folder must remain visible");
    };
    assert_eq!(
        children.len(),
        if created.is_ok() { 2 } else { 1 },
        "an independent cache must see the external request and any successfully added request"
    );
    assert!(children.iter().any(|node| node.name == "External"));
}

#[test]
fn a_failed_request_cache_write_leaves_the_repository_at_its_previous_version() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let mut request = add(&mut connection, &link.collection_id, "List users", None);
    let original_url = request.url.clone();
    connection
        .execute_batch(
            "CREATE TEMP TRIGGER reject_group_write BEFORE INSERT ON repository_request_groups
             BEGIN SELECT RAISE(ABORT, 'simulated cache write failure'); END;",
        )
        .unwrap();

    request.url = "/new-users-with-different-length".into();
    let request_id = request.id.clone();
    let error = store_request(&mut connection, request).err().unwrap();
    assert!(error.to_string().contains("simulated cache write failure"));
    connection
        .execute_batch("DROP TRIGGER reject_group_write")
        .unwrap();
    refresh_all(&mut connection).unwrap();
    let cached_url: String = connection
        .query_row(
            "SELECT url FROM requests WHERE id = ?",
            [request_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        cached_url, original_url,
        "a failed save must stay unapplied after refresh"
    );
}

#[test]
fn publishing_can_be_retried_when_the_initial_database_write_fails() {
    let root = Scratch::new();
    let mut connection = test_connection();
    git_init(&root.0);
    let source = fixtures::collection(&connection);
    connection.execute(
        "INSERT INTO variables (scope,collection_id,key,current_value,sensitive,created_at,updated_at)
         VALUES ('collection',?1,'token','retry-secret',1,'',''),
                ('collection',?1,'host','public-default',0,'','')", [&source],
    ).unwrap();
    connection
        .execute_batch(
            "CREATE TEMP TRIGGER reject_link_write BEFORE INSERT ON repository_links
             BEGIN SELECT RAISE(ABORT, 'simulated link write failure'); END;",
        )
        .unwrap();

    let mut preview = publish_preview(&connection, &source).unwrap();
    let error = connect(&mut connection, &root.0, true, Some((&source, &preview)))
        .err()
        .unwrap();
    assert!(error.to_string().contains("simulated link write failure"));
    assert!(repository_exists(&root.0).unwrap());
    fs::remove_file(root.0.join(".conductor/local.json")).unwrap();
    connection
        .execute_batch("DROP TRIGGER reject_link_write")
        .unwrap();
    assert!(preview[0].shared_keys.remove("host"));
    preview[0].secret_keys.insert("host".into());
    let Err(error) = connect(&mut connection, &root.0, true, Some((&source, &preview))) else {
        panic!("the installed key is still shared");
    };
    assert!(error.to_string().contains("already published"));
    assert!(!root.0.join(".conductor/local.json").exists());

    let retry = connect(&mut connection, &root.0, false, None);
    assert!(retry.is_ok(), "retry failed: {:?}", retry.err());
    assert!(fs::read_to_string(root.0.join(".conductor/local.json"))
        .unwrap()
        .contains("retry-secret"));
}

#[test]
fn refresh_keeps_last_good_requests_when_their_group_is_renamed_and_invalid() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let folder = add_users_folder(&mut connection, &link.collection_id);
    let request = add(
        &mut connection,
        &link.collection_id,
        "List users",
        Some(folder),
    );
    let groups = root.0.join(".conductor/requests");
    let renamed = groups.join("renamed.json");
    fs::rename(groups.join("users.json"), &renamed).unwrap();
    fs::write(renamed, b"{").unwrap();

    refresh_all(&mut connection).unwrap();
    assert_eq!(
        cache::revision(&connection, &request.id).unwrap(),
        request.repository_revision,
        "an unreadable renamed group must retain its last good requests"
    );
}

#[test]
fn a_placement_only_disk_change_allows_save_and_delivers_the_refresh() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let mut first = add(&mut connection, &link.collection_id, "First", None);
    let second = add(&mut connection, &link.collection_id, "Second", None);
    let path = root.0.join(".conductor/requests/root.json");
    let mut group: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    group["requests"]["first"]["position"] = serde_json::json!(9999);
    group["requests"]["second"]["position"] = serde_json::json!(1);
    fs::write(&path, serde_json::to_vec_pretty(&group).unwrap()).unwrap();

    first.url = "/saved-after-reorder".into();
    let SaveRequestResult::Saved {
        refresh: Some(refresh),
        ..
    } = store_request(&mut connection, first).unwrap()
    else {
        panic!("a matching revision must save and carry the placement refresh");
    };
    assert_eq!(
        refresh.changed_collection_ids,
        std::slice::from_ref(&link.collection_id)
    );
    assert!(refresh.changed_environment_ids.is_empty());
    assert!(refresh.changed_variable_collection_ids.is_empty());
    let tree = read_tree(&connection, &link.collection_id).unwrap();
    assert_eq!(tree[0].id, second.id);
    let saved = fs::read_to_string(path).unwrap();
    assert!(saved.contains("/saved-after-reorder"));
}

#[test]
fn unchanged_refresh_reads_no_repository_file_contents() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let assert_no_reads = |connection: &mut Connection| {
        format::FILE_READS.with(|reads| reads.set(0));
        assert!(refresh_all(connection)
            .unwrap()
            .changed_collection_ids
            .is_empty());
        format::FILE_READS.with(|reads| assert_eq!(reads.get(), 0));
    };
    assert_no_reads(&mut connection);
    let mut request = add(&mut connection, &link.collection_id, "Ping", None);
    request.url = "/changed".into();
    assert!(matches!(
        store_request(&mut connection, request).unwrap(),
        SaveRequestResult::Saved { .. }
    ));
    assert_no_reads(&mut connection);
}

#[test]
fn removing_a_link_leaves_repository_and_local_file_intact() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let data = root.0.join(".conductor");
    assert_eq!(canonical_root(data.to_str().unwrap()).unwrap(), root.0);
    let local = br#"{"collection":{"token":"fixture"}}"#;
    fs::write(data.join("local.json"), local).unwrap();

    super::super::remove_collection(&connection, &link.collection_id).unwrap();
    assert!(!linked(&connection, &link.collection_id).unwrap());
    assert_eq!(fs::read(data.join("local.json")).unwrap(), local);
    assert!(data.join("collection.json").is_file());
    assert!(!data.join(".conductor").exists());
}

#[test]
fn creating_at_a_previously_linked_root_requires_recovery() {
    let root = Scratch::new();
    let mut connection = test_connection();
    connect(&mut connection, &root.0, true, None).unwrap();
    fs::remove_dir_all(root.0.join(".conductor")).unwrap();
    let error = connect(&mut connection, &root.0, true, None).err().unwrap();
    assert!(error
        .to_string()
        .contains("Restore its .conductor directory or relink"));
}

#[cfg(unix)]
#[test]
fn repository_lock_coordinates_processes_without_worktree_files() {
    const CHILD_ROOT: &str = "CONDUCTOR_TEST_ROOT_LOCK";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let _lock = lock_root(Path::new(&root)).unwrap();
        std::process::exit(0);
    }

    let root = Scratch::new();
    let run_child = || {
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "commands::collections::repository::tests::repository_lock_coordinates_processes_without_worktree_files",
            ])
            .env(CHILD_ROOT, &root.0)
            .output()
            .unwrap()
    };
    let lock = lock_root(&root.0).unwrap();
    let blocked = run_child();
    assert!(!blocked.status.success());
    assert!(String::from_utf8_lossy(&blocked.stdout).contains("Another Conductor window"));
    assert_eq!(fs::read_dir(&root.0).unwrap().count(), 0);
    drop(lock);
    assert!(run_child().status.success());
    assert!(lock_root(&root.0).is_ok());
}

fn add_environment(connection: &mut Connection, collection: &str, name: &str) -> String {
    let id = new_entity_id(connection, collection).unwrap();
    mutate(connection, collection, |connection| {
        connection.execute(
            "INSERT INTO environments (id, collection_id, name, created_at, updated_at)
             VALUES (?, ?, ?, '', '')",
            params![id, collection, name],
        )?;
        Ok(())
    })
    .unwrap();
    id
}

fn private_token(connection: &Connection, scope: &str) -> Option<String> {
    connection
        .query_row(
            "SELECT value FROM repository_variables WHERE scope_id = ? AND key = 'token' AND local",
            [scope],
            |row| row.get(0),
        )
        .optional()
        .unwrap()
}

#[test]
fn local_stems_survive_display_renames_and_report_file_renames() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let id = add_environment(&mut connection, &link.collection_id, "Development");
    let data = root.0.join(".conductor");
    let local_path = data.join("local.json");
    let local = br#"{"environments":{"development":{"token":"fixture-development"}}}"#;
    fs::write(&local_path, local).unwrap();
    refresh_all(&mut connection).unwrap();
    assert_eq!(
        private_token(&connection, &id).as_deref(),
        Some("fixture-development")
    );

    mutate(&mut connection, &link.collection_id, |connection| {
        connection.execute(
            "UPDATE environments SET name = 'New name' WHERE id = ?",
            [&id],
        )?;
        Ok(())
    })
    .unwrap();
    let original = data.join("environments/development.json");
    assert!(original.is_file());
    assert_eq!(
        private_token(&connection, &id).as_deref(),
        Some("fixture-development")
    );

    fs::rename(original, data.join("environments/renamed.json")).unwrap();
    let target = values::target(
        &connection,
        &crate::commands::models::VariableTarget::Environment {
            environment_id: id.clone(),
        },
    )
    .unwrap()
    .unwrap();
    let error =
        values::set_script_values(&connection, &[(target, "token", "stale-write")]).unwrap_err();
    assert!(error.to_string().contains("Refresh the repository"));
    assert_eq!(fs::read(&local_path).unwrap(), local);

    let refresh = refresh_all(&mut connection).unwrap();
    let issue = refresh.collections.as_ref().unwrap()[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .as_deref()
        .unwrap();
    assert!(issue.contains("orphaned") && issue.contains("development"));
    assert!(!issue.contains("fixture-development"));
    assert_eq!(private_token(&connection, &id), None);
    assert_eq!(fs::read(&local_path).unwrap(), local);

    fs::write(
        &local_path,
        br#"{"environments":{"renamed":{"token":"fixture-development"}}}"#,
    )
    .unwrap();
    let refresh = refresh_all(&mut connection).unwrap();
    assert!(refresh.collections.as_ref().unwrap()[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .is_none());
    assert_eq!(
        private_token(&connection, &id).as_deref(),
        Some("fixture-development")
    );
}

#[test]
fn local_stems_keep_orphans_byte_stable_during_variable_edits() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let id = add_environment(&mut connection, &link.collection_id, "Development");
    let local_path = root.0.join(".conductor/local.json");
    let orphan = r#""ret\u0069red" : { "token" : "fixture-orphan" }"#;
    let untouched = r#""ne\u0069ghbor" : { "sensitive": true, "value" : "fixture-neighbor" }"#;
    let collection = r#""collection" : { "token" : "fixture-collection" }"#;
    fs::write(
        &local_path,
        format!(r#"{{{collection},"environments":{{{orphan},"development":{{{untouched},"token":"old"}},"empty":{{}}}}}}"#),
    )
    .unwrap();

    let mut reopened = test_connection();
    let reopened_link = connect(&mut reopened, &root.0, false, None).unwrap();
    let issue = reopened_link.collections[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .as_deref()
        .unwrap();
    assert!(issue.contains("retired") && issue.contains("empty"));
    assert!(!issue.contains("fixture-orphan"));

    let target = values::target(
        &connection,
        &crate::commands::models::VariableTarget::Environment {
            environment_id: id.clone(),
        },
    )
    .unwrap()
    .unwrap();
    values::set_script_values(&connection, &[(target, "token", "edited")]).unwrap();
    assert_eq!(private_token(&connection, &id).as_deref(), Some("edited"));
    assert_eq!(
        private_token(&connection, &link.collection_id).as_deref(),
        Some("fixture-collection")
    );
    assert_eq!(
        private_token(&reopened, &reopened_link.collection_id).as_deref(),
        Some("fixture-collection")
    );
    let after = fs::read_to_string(local_path).unwrap();
    for member in [orphan, untouched, collection, r#""empty":{}"#] {
        assert!(after.contains(member), "untouched member changed: {member}");
    }
}

#[test]
fn local_stems_disambiguate_colliding_names_and_reserve_orphans() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let local_path = root.0.join(".conductor/local.json");
    fs::write(
        &local_path,
        br#"{"environments":{"team-dev":{"token":"fixture-orphan"}}}"#,
    )
    .unwrap();
    let first = add_environment(&mut connection, &link.collection_id, "Team Dev");
    let second = add_environment(&mut connection, &link.collection_id, "Team-Dev");
    let paths = format::list(&root.0, "environments").unwrap();
    assert_eq!(
        paths,
        [
            "environments/team-dev-2.json",
            "environments/team-dev-3.json"
        ]
    );
    fs::write(&local_path, br#"{"environments":{"team-dev":{"token":"fixture-orphan"},"team-dev-2":{"token":"first"},"team-dev-3":{"token":"second"}}}"#).unwrap();
    refresh_all(&mut connection).unwrap();
    assert_eq!(private_token(&connection, &first).as_deref(), Some("first"));
    assert_eq!(
        private_token(&connection, &second).as_deref(),
        Some("second")
    );
}

#[test]
fn local_stems_reject_duplicate_json_keys_without_rewriting_the_file() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    add_environment(&mut connection, &link.collection_id, "Development");
    let local_path = root.0.join(".conductor/local.json");
    let duplicate = br#"{"environments":{"development":{"token":"first"},"developm\u0065nt":{"token":"second"}}}"#;
    fs::write(&local_path, duplicate).unwrap();
    let refresh = refresh_all(&mut connection).unwrap();
    let issue = refresh.collections.as_ref().unwrap()[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .as_deref()
        .unwrap();
    assert!(issue.contains("invalid local values"));
    assert!(!issue.contains("first") && !issue.contains("second"));
    assert_eq!(fs::read(local_path).unwrap(), duplicate);
}

#[cfg(unix)]
#[test]
fn a_single_file_save_reports_directory_sync_failure_before_committing_the_cache() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let linked = connect(&mut connection, &root.0, true, None).unwrap();
    let mut request = add(&mut connection, &linked.collection_id, "Ping", None);
    request.url = "/not-yet-durable".into();

    format::FAIL_DIRECTORY_SYNC.set(true);
    let saved = store_request(&mut connection, request.clone());
    let sync_was_attempted = !format::FAIL_DIRECTORY_SYNC.replace(false);

    assert!(sync_was_attempted);
    let Err(error) = saved else {
        panic!("save must report directory sync failure");
    };
    assert!(error
        .to_string()
        .contains("simulated directory sync failure"));
    assert_eq!(
        cache::revision(&connection, &request.id).unwrap(),
        request.repository_revision
    );
}

#[cfg(unix)]
#[test]
fn a_malformed_filename_can_report_an_issue_without_faking_a_duplicate_request() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let linked = connect(&mut connection, &root.0, true, None).unwrap();
    let mut request = add(&mut connection, &linked.collection_id, "Ping", None);
    let misleading = format!(
        "fake [{}]: Duplicate request ID.json",
        remote_id(&request.id)
    );
    fs::write(root.0.join(".conductor/requests").join(&misleading), b"{}").unwrap();

    let refresh = refresh_all(&mut connection).unwrap();
    assert!(refresh.collections.unwrap()[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .as_ref()
        .unwrap()
        .contains(&misleading));
    request.url = "/still-editable".into();
    assert!(matches!(
        store_request(&mut connection, request).unwrap(),
        SaveRequestResult::Saved { .. }
    ));
}

#[test]
fn refresh_sends_tab_labels_and_only_replaces_changed_collection_summaries() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let linked = connect(&mut connection, &root.0, true, None).unwrap();
    let request = add(&mut connection, &linked.collection_id, "Ping", None);
    let data = root.0.join(".conductor");
    let group_path = data.join("requests/root.json");
    let mut group: serde_json::Value =
        serde_json::from_slice(&fs::read(&group_path).unwrap()).unwrap();
    group["requests"]["ping"]["name"] = serde_json::json!("Write health");
    group["requests"]["ping"]["method"] = serde_json::json!("POST");
    fs::write(group_path, serde_json::to_vec(&group).unwrap()).unwrap();

    let refresh = refresh_all(&mut connection).unwrap();
    assert!(refresh.collections.is_none());
    let [change] = refresh.changed_requests.as_slice() else {
        panic!("refresh must report the edited request");
    };
    assert_eq!(change.request_id, request.id);
    assert_eq!(change.name.as_deref(), Some("Write health"));
    assert_eq!(change.method.as_deref(), Some("POST"));

    let path = data.join(MANIFEST);
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["name"] = serde_json::json!("Health checks");
    fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert_eq!(
        refresh_all(&mut connection).unwrap().collections.unwrap()[0].name,
        "Health checks"
    );
    assert!(refresh_all(&mut connection).unwrap().collections.is_none());
}

#[test]
fn saving_one_request_has_a_constant_shared_file_read_budget() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let linked = connect(&mut connection, &root.0, true, None).unwrap();
    let folder = add_users_folder(&mut connection, &linked.collection_id);
    add(
        &mut connection,
        &linked.collection_id,
        "Other request",
        Some(folder),
    );
    let mut request = add(&mut connection, &linked.collection_id, "Ping", None);
    request.url = "/edited".into();

    format::FILE_READS.set(0);
    assert!(matches!(
        store_request(&mut connection, request).unwrap(),
        SaveRequestResult::Saved { refresh: None, .. }
    ));
    // The target group is loaded, rechecked at replacement, and verified
    // before its metadata stamp is cached.
    assert!(format::FILE_READS.get() <= 3);
}

#[test]
fn a_private_refresh_during_send_does_not_hide_a_collection_rename_from_the_sidebar() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let linked = connect(&mut connection, &root.0, true, None).unwrap();
    let data = root.0.join(".conductor");
    let path = data.join(MANIFEST);
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["name"] = serde_json::json!("Renamed collection");
    fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    fs::write(
        data.join(values::LOCAL),
        br#"{"collection":{"token":"test-fixture"}}"#,
    )
    .unwrap();

    refresh_private_for_send(&mut connection, &linked.collection_id).unwrap();
    let refresh = refresh_workspace(&mut connection, &linked.collections).unwrap();

    assert_eq!(refresh.collections.unwrap()[0].name, "Renamed collection");
}
