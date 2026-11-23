use super::*;
use crate::{
    commands::{
        collections::{add_folder, tree::reparent_node},
        models::{CreateFolderInput, CreateRequestInput, MoveNodeInput, RequestDetail},
        requests::{add_request, store_request, SaveRequestResult},
    },
    fixtures,
    storage::test_connection,
};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
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
    let SaveRequestResult::Saved { revision } = result else {
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
    assert_eq!(
        refresh_all(&mut connection).unwrap().changed_requests[0].request_id,
        second.id
    );

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
    fs::write(&path, format!(r#"{{"requests":{{"{}":false,"{}":{{"name":"Updated","method":"GET","url":"/hand-edited"}}}}}}"#,
        remote_id(&bad.id), remote_id(&good.id))).unwrap();
    let refreshed = refresh_all(&mut connection).unwrap();
    let issue = refreshed.collections[0]
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
    let mut request = add(&mut connection, id, "Health", Some(folder.clone()));
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
    let group =
        group::parse(&fs::read(root.join(".conductor/requests/root.json")).unwrap()).unwrap();
    assert!(group.requests.contains_key(remote_id(&request.id)));
    request.url = "/open-draft".into();
    assert!(matches!(
        store_request(&mut connection, request).unwrap(),
        SaveRequestResult::Saved { .. }
    ));
    connection.execute(
        "INSERT INTO variables (scope,collection_id,key,current_value,sensitive,created_at,updated_at)
         VALUES ('collection',?,'token','private-token',1,'','')", [id],
    ).unwrap();
    fs::rename(&root, &moved).unwrap();
    let refresh = refresh_all(&mut connection).unwrap();
    assert!(refresh.collections[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .is_some());
    assert_eq!(read_tree(&connection, id).unwrap().len(), 2);
    relocate(&mut connection, id, &moved).unwrap();
    assert!(refresh_all(&mut connection).unwrap().collections[0]
        .repository
        .as_ref()
        .unwrap()
        .issue
        .is_none());
    let value: String = connection
        .query_row(
            "SELECT current_value FROM variables WHERE collection_id = ?",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(value, "private-token");
}

#[test]
fn publishing_a_collection_copies_private_variables_only_to_the_linked_database() {
    let root = Scratch::new();
    let mut connection = test_connection();
    let source = fixtures::collection(&connection);
    let environment = fixtures::environment(&connection, &source);
    connection.execute(
        "INSERT INTO variables (scope,environment_id,key,current_value,sensitive,created_at,updated_at)
         VALUES ('environment',?,'token','private-token',1,'',''), ('environment',?,'host','current-host',0,'','')",
        [&environment, &environment],
    ).unwrap();
    let link = connect(&mut connection, &root.0, true, Some(&source)).unwrap();
    let path = format::list(&root.0, "environments")
        .unwrap()
        .pop()
        .unwrap();
    let shared =
        format::parse_environment(&format::read(&root.0, &path).unwrap().unwrap()).unwrap();
    assert!(shared.defaults.is_empty());
    let values: String = connection.query_row(
        "SELECT group_concat(v.current_value, ',') FROM variables v JOIN environments e ON v.environment_id = e.id
         WHERE e.collection_id = ?", [&link.collection_id], |row| row.get(0)
    ).unwrap();
    assert!(values.contains("private-token"));
    assert!(values.contains("current-host"));
}
