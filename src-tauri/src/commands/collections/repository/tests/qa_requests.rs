use super::*;
use crate::commands::collections::tree::delete_subtree;
use serde_json::{json, Value};

fn named_folder(
    connection: &mut Connection,
    collection: &str,
    name: &str,
    parent_id: Option<String>,
) -> String {
    mutate(connection, collection, |connection| {
        add_folder(
            connection,
            &CreateFolderInput {
                collection_id: collection.into(),
                name: name.into(),
                parent_id,
                position: 0,
            },
        )
    })
    .unwrap()
}

#[cfg(unix)]
#[test]
fn failed_folder_delete_keeps_every_request_in_the_subtree() {
    use std::os::unix::fs::PermissionsExt;

    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let collection = &link.collection_id;
    let folder = named_folder(&mut connection, collection, "Parent", None);
    let child = named_folder(&mut connection, collection, "Child", Some(folder.clone()));
    let first = add(
        &mut connection,
        collection,
        "Parent request",
        Some(folder.clone()),
    );
    let second = add(
        &mut connection,
        collection,
        "Child request",
        Some(child.clone()),
    );
    let paths = root.0.join(".conductor/requests");
    let parent_file = paths.join("parent.json");
    let child_file = paths.join("parent/child.json");
    let later = if folder < child {
        &child_file
    } else {
        &parent_file
    };
    let directory = later.parent().unwrap();
    let permissions = fs::metadata(directory).unwrap().permissions();
    fs::set_permissions(directory, fs::Permissions::from_mode(0o555)).unwrap();
    let result = mutate(&mut connection, collection, |connection| {
        delete_subtree(connection, &folder)
    });
    fs::set_permissions(directory, permissions).unwrap();

    let error = result.expect_err("the later group file must be protected from deletion");
    assert!(error.to_string().contains("Permission denied"), "{error}");
    assert!(later.is_file());
    refresh_all(&mut connection).unwrap();
    let missing: Vec<_> = [&first, &second]
        .into_iter()
        .filter(|request| cache::revision(&connection, &request.id).unwrap().is_none())
        .map(|request| request.name.as_str())
        .collect();
    assert!(
        missing.is_empty(),
        "deleting the folder failed, but refresh dropped {missing:?}"
    );
}

#[test]
fn new_environment_uses_a_portable_suffix_when_another_filename_differs_only_in_case() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let collection = &link.collection_id;
    let directory = root.0.join(".conductor/environments");
    fs::create_dir_all(&directory).unwrap();
    let existing = directory.join("Development.json");
    let bytes = serde_json::to_vec(&json!({"id": Uuid::new_v4(), "name": "Hand edited"})).unwrap();
    fs::write(&existing, &bytes).unwrap();
    refresh_all(&mut connection).unwrap();

    add_environment(&mut connection, collection, "Development");

    assert_eq!(fs::read(&existing).unwrap(), bytes);
    assert!(
        directory.join("development-2.json").is_file(),
        "the worktree must allow a checkout on a case-insensitive filesystem"
    );
}

#[test]
fn refresh_reports_bad_or_oversized_group_paths_and_keeps_last_good_requests() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let collection = &link.collection_id;
    let request = add(&mut connection, collection, "Existing", None);
    let groups = root.0.join(".conductor/requests");
    let old = fs::read(groups.join("root.json")).unwrap();
    let long = format!("{}.json", "x".repeat(161));
    let names = ["..json", "UsErs.json", "日本語.json", &long];
    for name in names {
        fs::write(groups.join(name), b"{}").unwrap();
    }
    fs::File::create(groups.join("root.json"))
        .unwrap()
        .set_len(format::MAX_FILE_BYTES + 1)
        .unwrap();

    let refreshed = refresh_all(&mut connection).unwrap();
    let issue = refreshed.collections[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .as_deref()
        .unwrap();
    for name in names {
        assert!(issue.contains(name), "missing path from issue: {name}");
    }
    assert!(issue.contains("larger than 16 MiB"));
    assert!(refreshed.changed_requests.is_empty());
    assert_eq!(
        cache::revision(&connection, &request.id).unwrap(),
        request.repository_revision
    );
    assert_eq!(
        fs::metadata(groups.join("root.json")).unwrap().len(),
        format::MAX_FILE_BYTES + 1
    );

    fs::write(groups.join("root.json"), old).unwrap();
    assert!(refresh_all(&mut connection)
        .unwrap()
        .changed_requests
        .is_empty());
}

#[test]
fn unusual_folder_names_keep_distinct_portable_paths_and_original_names() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let collection = &link.collection_id;
    let long = "a".repeat(700);
    let cases = [
        ("..", "item.json".to_owned()),
        ("../été/Users", "t-users.json".to_owned()),
        ("USERS", "users.json".to_owned()),
        ("users", "users-2.json".to_owned()),
        ("日本語", "item-2.json".to_owned()),
        (long.as_str(), format!("{}.json", "a".repeat(48))),
    ];
    for (name, file) in &cases {
        let folder = named_folder(&mut connection, collection, name, None);
        add(&mut connection, collection, "Child request", Some(folder));
        assert!(
            root.0.join(".conductor/requests").join(file).is_file(),
            "{name}"
        );
    }
    let reopened = connect(&mut test_connection(), &root.0, false, None).unwrap();
    for (name, _) in cases {
        assert!(reopened.tree.iter().any(|node| node.name == name), "{name}");
    }
}

#[test]
fn equal_generation_branch_moves_keep_one_visible_request_and_preserve_both_files() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let collection = &link.collection_id;
    let alpha = named_folder(&mut connection, collection, "Alpha", None);
    let beta = named_folder(&mut connection, collection, "Beta", None);
    let original = add(&mut connection, collection, "Fetch", None);
    let requests = root.0.join(".conductor/requests");
    let baseline = fs::read(requests.join("root.json")).unwrap();
    let move_and_save = |connection: &mut Connection, folder: String, url: &str| {
        mutate(connection, collection, |connection| {
            reparent_node(
                connection,
                &MoveNodeInput {
                    node_id: original.id.clone(),
                    parent_id: Some(folder),
                    position: 0,
                },
            )
        })
        .unwrap();
        let mut moved = original.clone();
        moved.repository_revision = cache::revision(connection, &moved.id).unwrap();
        moved.url = url.into();
        assert!(matches!(
            store_request(connection, moved).unwrap(),
            SaveRequestResult::Saved { .. }
        ));
    };

    let empty_alpha = fs::read(requests.join("alpha.json")).unwrap();
    move_and_save(&mut connection, alpha, "/alpha-edit");
    let alpha_move = fs::read(requests.join("alpha.json")).unwrap();
    fs::write(requests.join("alpha.json"), empty_alpha).unwrap();
    fs::write(requests.join("root.json"), baseline).unwrap();
    refresh_all(&mut connection).unwrap();
    move_and_save(&mut connection, beta, "/beta-edit");
    let beta_move = fs::read(requests.join("beta.json")).unwrap();
    fs::write(requests.join("alpha.json"), &alpha_move).unwrap();

    let refresh = refresh_all(&mut connection).unwrap();
    let issue = refresh.collections[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .as_deref()
        .unwrap();
    assert!(
        issue.contains("Duplicate request ID (using requests/alpha.json)"),
        "{issue}"
    );
    let url: String = connection
        .query_row(
            "SELECT url FROM requests WHERE id = ?",
            [&original.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(url, "/alpha-edit");
    for (path, bytes) in [("alpha.json", alpha_move), ("beta.json", beta_move)] {
        let retained = fs::read(requests.join(path)).unwrap();
        assert_eq!(retained, bytes);
        let json: Value = serde_json::from_slice(&retained).unwrap();
        assert_eq!(json["requests"]["fetch"]["moveGeneration"], 1);
    }
}

#[test]
fn refresh_keeps_requests_through_each_folder_creation_step() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let collection = &link.collection_id;
    let existing = add(&mut connection, collection, "Existing", None);
    let manifest = root.0.join(".conductor/collection.json");
    let group = root.0.join(".conductor/requests/users.json");
    let old_manifest = fs::read(&manifest).unwrap();
    let folder = add_users_folder(&mut connection, collection);
    let new_manifest = fs::read(&manifest).unwrap();
    let new_group = fs::read(&group).unwrap();

    fs::remove_file(&group).unwrap();
    for completed in 0..=2 {
        fs::write(
            &manifest,
            if completed == 0 {
                &old_manifest
            } else {
                &new_manifest
            },
        )
        .unwrap();
        if completed == 2 {
            fs::write(&group, &new_group).unwrap();
        }
        let refresh = refresh_all(&mut connection).unwrap();
        assert!(refresh.collections[0]
            .repository
            .as_ref()
            .unwrap()
            .issue
            .is_none());
        let tree = read_tree(&connection, collection).unwrap();
        assert_eq!(tree.iter().any(|node| node.id == folder), completed > 0);
        assert!(tree.iter().any(|node| node.id == existing.id));
    }
}

#[test]
fn refresh_keeps_requests_through_each_placeholder_folder_rename_step() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let collection = &link.collection_id;
    let folder = named_folder(&mut connection, collection, "New folder", None);
    let request = add(&mut connection, collection, "Fetch", Some(folder.clone()));
    let manifest = root.0.join(".conductor/collection.json");
    let placeholder = root.0.join(".conductor/requests/new-folder.json");
    let named = root.0.join(".conductor/requests/health.json");
    // Exercise legacy checkouts whose placeholder groups still have plain keys.
    fs::rename(
        root.0.join(format!(
            ".conductor/requests/new-folder-{}.json",
            remote_id(&folder)
        )),
        &placeholder,
    )
    .unwrap();
    refresh_all(&mut connection).unwrap();
    let old_manifest = fs::read(&manifest).unwrap();
    let old_group = fs::read(&placeholder).unwrap();
    mutate(&mut connection, collection, |connection| {
        connection.execute(
            "UPDATE collection_nodes SET name = 'Health' WHERE id = ?",
            [&folder],
        )?;
        Ok(())
    })
    .unwrap();
    let new_manifest = fs::read(&manifest).unwrap();
    let new_group = fs::read(&named).unwrap();

    fs::remove_file(&named).unwrap();
    fs::write(&placeholder, &old_group).unwrap();
    for completed in 0..=3 {
        if completed == 1 {
            fs::write(&named, &new_group).unwrap();
        }
        fs::write(
            &manifest,
            if completed < 2 {
                &old_manifest
            } else {
                &new_manifest
            },
        )
        .unwrap();
        if completed == 3 {
            fs::remove_file(&placeholder).unwrap();
        }
        let refresh = refresh_all(&mut connection).unwrap();
        let issue = refresh.collections[0]
            .repository
            .as_ref()
            .unwrap()
            .issue
            .as_deref();
        if matches!(completed, 1 | 2) {
            assert!(issue
                .unwrap()
                .contains("Duplicate request ID (using requests/health.json)"));
            assert_eq!(fs::read(&placeholder).unwrap(), old_group);
            assert_eq!(fs::read(&named).unwrap(), new_group);
        } else {
            assert!(issue.is_none());
        }
        let tree = read_tree(&connection, collection).unwrap();
        assert_eq!(
            tree[0].name,
            if completed < 2 {
                "New folder"
            } else {
                "Health"
            }
        );
        let CollectionNodeKind::Folder { children } = &tree[0].kind else {
            panic!("folder must remain visible after step {completed}");
        };
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, request.id);
    }
}

#[test]
fn refresh_keeps_last_good_request_when_its_only_moved_copy_is_invalid() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let collection = &link.collection_id;
    add_users_folder(&mut connection, collection);
    let mut request = add(&mut connection, collection, "Fetch", None);
    request.url = "/last-known-good".into();
    assert!(matches!(
        store_request(&mut connection, request.clone()).unwrap(),
        SaveRequestResult::Saved { .. }
    ));
    let groups = root.0.join(".conductor/requests");
    let source = groups.join("root.json");
    let destination = groups.join("users.json");
    let source_data: Value = serde_json::from_slice(&fs::read(&source).unwrap()).unwrap();
    let mut destination_data: Value =
        serde_json::from_slice(&fs::read(&destination).unwrap()).unwrap();
    let mut moved = source_data["requests"]["fetch"].clone();
    moved["name"] = Value::Bool(false);
    moved["moveGeneration"] = json!(1);
    destination_data["requests"]["fetch"] = moved;
    fs::write(&destination, serde_json::to_vec(&destination_data).unwrap()).unwrap();
    fs::write(&source, br#"{"requests":{}}"#).unwrap();

    let refresh = refresh_all(&mut connection).unwrap();
    let issue = refresh.collections[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .as_deref()
        .unwrap();
    assert!(issue.contains("requests/users.json") && issue.contains(remote_id(&request.id)));
    let cached: Option<String> = connection
        .query_row(
            "SELECT url FROM requests WHERE id = ?",
            [&request.id],
            |row| row.get(0),
        )
        .optional()
        .unwrap();
    assert_eq!(
        cached.as_deref(),
        Some("/last-known-good"),
        "the invalid moved copy must not discard the last readable request"
    );

    let malformed = fs::read(&destination).unwrap();
    request.repository_revision = cache::revision(&connection, &request.id).unwrap();
    request.url = "/user-edit".into();
    assert!(store_request(&mut connection, request.clone()).is_err());
    assert_eq!(fs::read(&destination).unwrap(), malformed);

    destination_data["requests"]["fetch"]["name"] = json!("Fetch");
    fs::write(&destination, serde_json::to_vec(&destination_data).unwrap()).unwrap();
    refresh_all(&mut connection).unwrap();
    request.repository_revision = cache::revision(&connection, &request.id).unwrap();
    assert!(matches!(
        store_request(&mut connection, request).unwrap(),
        SaveRequestResult::Saved { .. }
    ));
}

#[test]
fn ui_placeholder_keys_are_unique_and_legacy_git_merge_keeps_both_requests() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    let root = Scratch::new();
    git_init(&root.0);
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let collection = &link.collection_id;
    let path = ".conductor/requests/root.json";
    let group_path = root.0.join(path);
    let left = add(&mut connection, collection, NEW_REQUEST, None);
    let right = add(&mut connection, collection, NEW_REQUEST, None);
    let created = fs::read(&group_path).unwrap();
    for request in [&left, &right] {
        let id = remote_id(&request.id);
        assert_eq!(
            group::key_for(&created, id).unwrap(),
            Some(format!("new-request-{id}"))
        );
    }
    let first_folder = named_folder(&mut connection, collection, NEW_FOLDER, None);
    let second_folder = named_folder(&mut connection, collection, NEW_FOLDER, None);
    for folder in [&first_folder, &second_folder] {
        assert!(!root
            .0
            .join(format!(
                ".conductor/requests/new-folder-{}.json",
                remote_id(folder)
            ))
            .exists());
        add(
            &mut connection,
            collection,
            "Fetch",
            Some(folder.to_string()),
        );
        assert!(root
            .0
            .join(format!(
                ".conductor/requests/new-folder-{}.json",
                remote_id(folder)
            ))
            .is_file());
    }

    // Model a merge from older app builds, where teammates used the same key.
    // Insert the entries far enough apart for Git to merge both independently.
    let member = |name: &str, id: &str| {
        json!({"id": id, "name": name, "method": "GET", "url": "/"}).to_string()
    };
    let anchors = (0..10)
        .map(|n| {
            format!(
                r#"  "anchor-{n}": {},"#,
                member("Anchor", &Uuid::new_v4().to_string())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let end = member("End", &Uuid::new_v4().to_string());
    let document = |first: &str, last: &str| {
        format!("{{\"requests\":{{\n{first}{anchors}\n{last}  \"end\": {end}\n}}}}\n")
    };
    let left_line = format!(
        "  \"new-request\": {},\n",
        member(NEW_REQUEST, remote_id(&left.id))
    );
    let right_line = format!(
        "  \"new-request\": {},\n",
        member(NEW_REQUEST, remote_id(&right.id))
    );
    let mut history = String::new();
    for (branch, mark, parent, bytes) in [
        ("base", 1, "", document("", "")),
        ("left", 2, "from :1\n", document(&left_line, "")),
        ("right", 3, "from :1\n", document("", &right_line)),
    ] {
        history.push_str(&format!(
            "commit refs/heads/{branch}\nmark :{mark}\ncommitter Conductor Test <test@example.invalid> 1 +0000\ndata 4\ntest\n{parent}M 100644 inline {path}\ndata {}\n{bytes}\n",
            bytes.len()
        ));
    }
    let git = || {
        let mut git = Command::new("git");
        git.current_dir(&root.0).args([
            "-c",
            "user.name=Conductor Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
        ]);
        git
    };
    let mut import = git()
        .args(["fast-import", "--quiet"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    import
        .stdin
        .take()
        .unwrap()
        .write_all(history.as_bytes())
        .unwrap();
    assert!(import.wait().unwrap().success());
    fs::remove_file(&group_path).unwrap();
    assert!(git()
        .args(["switch", "--quiet", "left"])
        .status()
        .unwrap()
        .success());
    assert!(git()
        .args(["merge", "--no-commit", "--no-ff", "right"])
        .status()
        .unwrap()
        .success());

    let refresh = refresh_all(&mut connection).unwrap();
    let issue = refresh.collections[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .as_deref()
        .unwrap();
    assert!(issue.contains("Duplicate request key \"new-request\""));
    let tree = read_tree(&connection, collection).unwrap();
    let ids: HashSet<_> = tree
        .iter()
        .filter(|node| node.name == NEW_REQUEST)
        .map(|node| node.id.as_str())
        .collect();
    assert_eq!(ids, HashSet::from([left.id.as_str(), right.id.as_str()]));

    let mut draft = left;
    draft.repository_revision = cache::revision(&connection, &draft.id).unwrap();
    draft.url = "/changed".into();
    assert!(matches!(
        store_request(&mut connection, draft).unwrap(),
        SaveRequestResult::Saved { .. }
    ));
    let merged = group::parse(&fs::read(&group_path).unwrap()).unwrap();
    assert!(merged.duplicate_keys.is_empty());
    assert!(merged.requests[remote_id(&right.id)].is_ok());
    let refresh = refresh_all(&mut connection).unwrap();
    assert!(refresh.collections[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .is_none());
}

#[test]
fn publish_names_the_request_folder_and_credential_field_with_a_fix() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let collection = fixtures::collection(&connection);
    let folder = named_folder(&mut connection, &collection, "Payments", None);
    let request = add(&mut connection, &collection, "Capture", Some(folder));
    connection
        .execute(
            "UPDATE requests SET auth_json = ? WHERE id = ?",
            params![
                r#"{"authType":"bearer","token":"literal-credential"}"#,
                request.id
            ],
        )
        .unwrap();
    let error = connect(&mut connection, &root.0, true, Some(&collection))
        .expect_err("publishing a literal credential must fail")
        .to_string();
    for text in ["Payments", "Capture", "auth token", "{{token}}"] {
        assert!(error.contains(text), "{error}");
    }
    assert!(!error.contains("literal-credential"));
}

#[test]
fn a_failed_cache_commit_after_folder_deletion_leaves_a_recoverable_group() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let link = connect(&mut connection, &root.0, true, None).unwrap();
    let collection = &link.collection_id;
    let folder = named_folder(&mut connection, collection, "Parent", None);
    add(&mut connection, collection, "Fetch", Some(folder.clone()));
    let groups = root.0.join(".conductor/requests");
    let before = fs::read(groups.join("parent.json")).unwrap();
    connection
        .execute_batch(
            "CREATE TEMP TRIGGER fail_link_write BEFORE UPDATE OF watch_stamp ON repository_links
             BEGIN SELECT RAISE(ABORT, 'Cannot save the repository link'); END;",
        )
        .unwrap();

    let error = mutate(&mut connection, collection, |connection| {
        delete_subtree(connection, &folder)
    })
    .unwrap_err()
    .to_string();

    let backup = fs::read_dir(groups)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".parent.json.conductor-deleted-")
        })
        .expect("the original request group is recoverable after the error");
    assert_eq!(fs::read(&backup).unwrap(), before);
    assert!(error.contains(backup.to_str().unwrap()), "{error}");
}
