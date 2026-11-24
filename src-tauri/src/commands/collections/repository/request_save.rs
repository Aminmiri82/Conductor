use super::{
    apply_edits, cache, file_stamp, format, group, input, lock_root, read_group, read_link,
    record_watch, refresh_after, refresh_locked, remote_id, Edit, Entity, File, IssueKind,
    RequestGroup, Watch, MANIFEST, MAX_REPOSITORY_BYTES, REPOSITORY_SIZE_ERROR,
};
use crate::{
    commands::{models::RequestDetail, requests::SaveRequestResult},
    storage::StorageError,
};
use rusqlite::Connection;
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

pub(crate) fn save_linked_request(
    connection: &mut Connection,
    collection_id: &str,
    request: &RequestDetail,
    update: impl FnOnce(&Connection) -> Result<(), StorageError>,
) -> Result<Option<SaveRequestResult>, StorageError> {
    let Some(mut link) = read_link(connection, collection_id)? else {
        return Ok(None);
    };
    let root = Path::new(&link.root);
    let _lock = lock_root(root)?;
    let watch = Watch::read(root)?;
    let (watch, mut refresh) = if watch.matches(link.watch.as_ref()) {
        (Some(watch), None)
    } else {
        let (changes, watch) = refresh_locked(connection, collection_id, &link, Ok(watch))?;
        let refresh = refresh_after(connection, Some((collection_id, changes)))?;
        link = read_link(connection, collection_id)?.expect("locked linked collection");
        (watch, Some(refresh))
    };

    let stored = cache::stored_request(connection, &request.id)?;
    let revision = stored.as_ref().map(|stored| stored.revision.as_str());
    if revision.is_none()
        || revision != request.repository_revision.as_deref()
        || link.watch.as_ref().is_some_and(|watch| {
            watch.issues.iter().any(|kind| {
                matches!(
                    kind, IssueKind::DuplicateRequest(id) if id == remote_id(&request.id)
                )
            })
        })
    {
        return Ok(Some(SaveRequestResult::Conflict {
            revision: revision.map(str::to_owned),
            refresh,
        }));
    }

    let saved = watch
        .ok_or_else(|| input("The repository could not be refreshed. Repair it before editing."))
        .and_then(|watch| {
            save_current(
                connection,
                Path::new(&link.root),
                link.watch,
                watch,
                collection_id,
                stored.expect("checked request revision"),
                update,
            )
        });
    let result = match saved {
        Ok(revision) => {
            if let Some(refresh) = &mut refresh {
                // This response delivers the final revision separately.
                refresh
                    .changed_requests
                    .retain(|change| change.request_id != request.id);
            }
            SaveRequestResult::Saved {
                revision: Some(revision),
                refresh,
            }
        }
        Err(error) => match refresh {
            Some(refresh) => SaveRequestResult::Failed {
                message: error.to_string(),
                refresh,
            },
            None => return Err(error),
        },
    };
    Ok(Some(result))
}

fn save_current(
    connection: &mut Connection,
    root: &Path,
    state: Option<super::WatchState>,
    watch: Watch,
    collection_id: &str,
    previous: cache::StoredFile,
    update: impl FnOnce(&Connection) -> Result<(), StorageError>,
) -> Result<String, StorageError> {
    let placement: cache::Placement<'_> = serde_json::from_slice(&previous.contents)?;
    let folder_id = placement.parent_id.unwrap_or_default();
    let (path, byte_count) = cache::request_group(connection, collection_id, folder_id)?
        .ok_or_else(|| {
            input("The request group was removed. Refresh the repository before editing.")
        })?;
    let (bytes, group) = read_group(root, &path)?.ok_or_else(|| {
        input(format!(
            "{path} was removed on disk. Refresh the repository before editing."
        ))
    })?;
    let id = remote_id(&previous.id);
    if let Some(error) = &group.identity_issue {
        return Err(input(format!("{path}: {error}")));
    }
    let original = group
        .request_bytes(id)
        .map_err(|error| input(format!("{path} [{id}]: {error}")))?;
    if original.as_deref() != Some(previous.contents.as_slice()) {
        return Err(input(format!(
            "{path} [{id}] has changed on disk. Refresh the repository before editing."
        )));
    }

    let tx = connection.savepoint()?;
    update(&tx)?;
    let request = cache::project_one_request(&tx, &previous.id, placement)?;
    let contents = format::encode(&request)?;
    if contents == previous.contents {
        tx.commit()?;
        return Ok(previous.revision);
    }
    if file_stamp(root, MANIFEST)? != watch.0.get(MANIFEST).copied() {
        return Err(input(
            "collection.json has changed on disk. Refresh the repository before editing.",
        ));
    }
    let next = group::edit_with_keys(
        Some((&bytes, &group)),
        placement.parent_id,
        &BTreeMap::from([(request.id.clone(), Some(&request))]),
        &HashMap::new(),
    )?;
    if cache::repository_bytes(&tx, collection_id)? - i64::from(byte_count) + next.len() as i64
        > MAX_REPOSITORY_BYTES as i64
    {
        return Err(input(REPOSITORY_SIZE_ERROR));
    }

    cache::record_group(
        &tx,
        collection_id,
        &path,
        &RequestGroup {
            folder_id: folder_id.to_owned(),
            byte_count: next.len() as u32,
        },
    )?;
    let key = cache::request_key(&request.id);
    let revision = cache::record_file(
        &tx,
        collection_id,
        &key,
        &File {
            contents,
            entity: Entity::Request(Box::new(request)),
        },
        Some(&previous),
    )?;
    let edits = [Edit {
        order: 2,
        path,
        old: Some(bytes),
        next: Some(next),
    }];
    let watch =
        (group.duplicate_keys.is_empty() && watch.matches_before(root, &edits)).then_some(watch);
    apply_edits(root, &edits, || {
        record_watch(
            &tx,
            collection_id,
            watch.and_then(|watch| watch.after_edit(root, &edits)),
            state,
        )?;
        tx.commit()?;
        Ok(())
    })?;
    Ok(revision)
}
