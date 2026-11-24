use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    ops::Range,
};

use crate::storage::StorageError;
use serde::Deserialize;
use serde_json::value::RawValue;

use super::{
    format::{self, Request},
    input as invalid,
};

pub(super) struct Group {
    pub(super) folder_id: Option<String>,
    pub(super) requests: BTreeMap<String, Result<Request, String>>,
    pub(super) identity_issue: Option<String>,
    pub(super) duplicate_keys: BTreeSet<String>,
    pub(super) duplicate_ids: BTreeSet<String>,
    entries: Range<usize>,
    members: Vec<Member>,
}

impl Group {
    pub(super) fn request_bytes(&self, id: &str) -> Result<Option<Vec<u8>>, StorageError> {
        match self.requests.get(id) {
            Some(Ok(request)) => format::encode(request).map(Some),
            Some(Err(error)) => Err(invalid(error.clone())),
            None => Ok(None),
        }
    }
}

struct Member {
    key: String,
    id: Option<String>,
    separator: Range<usize>,
    value: Range<usize>,
}

pub(super) fn parse(contents: &[u8]) -> Result<Group, StorageError> {
    let (folder_id, entries) = envelope(contents)?;
    let map = &contents[entries.clone()];
    let mut members = format::json_members(map, |member| Member {
        key: member.key,
        id: None,
        separator: member.separator,
        value: member.value,
    })?;
    let duplicate_keys = duplicate_keys(&members)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let mut requests = BTreeMap::new();
    let mut duplicate_ids = BTreeSet::new();
    let mut unidentified: Option<(String, String)> = None;
    for member in &mut members {
        let raw = &map[member.value.clone()];
        let parsed = request(raw, folder_id.as_deref());
        let Some(id) = parsed
            .as_ref()
            .ok()
            .map(|request| request.id.clone())
            .or_else(|| member_id(raw))
        else {
            if unidentified
                .as_ref()
                .is_none_or(|(key, _)| member.key <= *key)
            {
                unidentified = Some((member.key.clone(), parsed.unwrap_err().to_string()));
            }
            continue;
        };
        let parsed = if !format::slug_key(&member.key) {
            Err(format!(
                "invalid request key {}: use lowercase letters, digits and hyphens",
                member.key
            ))
        } else {
            parsed.map_err(|error| error.to_string())
        };
        member.id = Some(id.clone());
        requests
            .entry(id.clone())
            .and_modify(|previous| {
                duplicate_ids.insert(id);
                *previous = Err("duplicate request ID".into());
            })
            .or_insert(parsed);
    }
    Ok(Group {
        folder_id,
        requests,
        identity_issue: unidentified
            .map(|(key, error)| format!("request {key} has no usable ID: {error}")),
        duplicate_keys,
        duplicate_ids,
        entries,
        members,
    })
}

fn request(raw: &[u8], folder_id: Option<&str>) -> Result<Request, StorageError> {
    let mut request = format::parse_request(raw)?;
    if request.parent_id.is_some() {
        return Err(invalid("request parentId comes from its group"));
    }
    request.parent_id = folder_id.map(str::to_owned);
    Ok(request)
}

fn member_id(raw: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct Identity {
        id: String,
    }
    let Identity { id } = serde_json::from_slice(raw).ok()?;
    format::validate_id(&id).ok()?;
    Some(id)
}

pub(super) fn folder_id(contents: &[u8]) -> Result<Option<String>, StorageError> {
    Ok(envelope(contents)?.0)
}

impl Group {
    pub(super) fn key(&self, id: &str) -> Option<&str> {
        self.members
            .iter()
            .find(|member| member.id.as_deref() == Some(id))
            .map(|member| member.key.as_str())
    }
}

#[cfg(test)]
pub(super) fn key_for(contents: &[u8], id: &str) -> Result<Option<String>, StorageError> {
    Ok(parse(contents)?.key(id).map(str::to_owned))
}

fn duplicate_keys(members: &[Member]) -> HashSet<&str> {
    let mut seen = HashSet::new();
    members
        .iter()
        .map(|member| member.key.as_str())
        .filter(|key| !seen.insert(*key))
        .collect()
}

#[cfg(test)]
pub(super) fn edit(
    previous: Option<&[u8]>,
    folder_id: Option<&str>,
    edits: &BTreeMap<String, Option<&Request>>,
) -> Result<Vec<u8>, StorageError> {
    let parsed = previous.map(parse).transpose()?;
    edit_with_keys(
        previous.zip(parsed.as_ref()),
        folder_id,
        edits,
        &HashMap::new(),
    )
}

pub(super) fn edit_with_keys(
    previous: Option<(&[u8], &Group)>,
    folder_id: Option<&str>,
    edits: &BTreeMap<String, Option<&Request>>,
    move_keys: &HashMap<&str, String>,
) -> Result<Vec<u8>, StorageError> {
    let empty;
    let empty_group;
    let (contents, group) = match previous {
        Some(previous) => previous,
        None => {
            empty = match folder_id {
                Some(id) => format!(
                    "{{\n  \"folderId\": {},\n  \"requests\": {{}}\n}}\n",
                    serde_json::to_string(id)?
                ),
                None => "{\n  \"requests\": {}\n}\n".to_string(),
            };
            empty_group = parse(empty.as_bytes())?;
            (empty.as_bytes(), &empty_group)
        }
    };
    if group.folder_id.as_deref() != folder_id {
        return Err(invalid("request group folderId changed on disk"));
    }
    let entries = &group.entries;
    let map = &contents[entries.clone()];
    let members = &group.members;
    let mut slugs = format::Slugs::default();
    for member in members {
        slugs.reserve(&member.key);
    }
    let mut existing = HashSet::new();
    let mut retained_keys = HashSet::new();
    let mut order = Vec::new();
    for member in members {
        let mut key = member.key.clone();
        if let Some(id) = &member.id {
            let replacement = match edits.get(id) {
                Some(None) => continue,
                Some(Some(request)) => Some(*request),
                None => None,
            };
            let previous = group
                .requests
                .get(id)
                .and_then(|request| request.as_ref().ok());
            if let Some(request) = replacement.filter(|request| {
                super::placeholder_key(&key, "new-request")
                    && !request.name.eq_ignore_ascii_case(super::NEW_REQUEST)
                    && previous.is_some_and(|old| old.name.eq_ignore_ascii_case(super::NEW_REQUEST))
            }) {
                key = slugs.claim("", &request.name);
            } else if !retained_keys.insert(member.key.as_str()) {
                key = match replacement.or(previous) {
                    Some(request) => claim_request_key(&mut slugs, request),
                    None => slugs.keep_key(&key),
                };
            }
            existing.insert(id.as_str());
        }
        order.push((key, member.id.as_deref(), Some(member)));
    }
    for (id, request) in edits {
        if let Some(request) = request.filter(|_| !existing.contains(id.as_str())) {
            let key = if let Some(key) = move_keys.get(id.as_str()) {
                slugs.keep_key(key)
            } else {
                claim_request_key(&mut slugs, request)
            };
            order.push((key, Some(id.as_str()), None));
        }
    }
    if members.windows(2).all(|pair| pair[0].key <= pair[1].key) {
        order.sort_by(|(left, _, _), (right, _, _)| left.cmp(right));
    }

    let (newline, indent): (&[u8], &[u8]) = if contents.windows(2).any(|pair| pair == b"\r\n") {
        (b"\r\n", b"\r\n    ")
    } else if contents.contains(&b'\n') {
        (b"\n", b"\n    ")
    } else {
        (b"\n", b"")
    };
    let new_separator = members
        .first()
        .map_or(indent, |member| &map[member.separator.clone()]);
    let mut output = Vec::with_capacity(contents.len());
    output.extend_from_slice(&contents[..entries.start + 1]);
    for (position, (key, id, member)) in order.iter().enumerate() {
        let separator = member.map_or(new_separator, |member| &map[member.separator.clone()]);
        format::write_separator(&mut output, separator, position > 0);
        let inset = separator
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map(|newline| &separator[newline + 1..]);
        if let Some(member) = member.filter(|member| member.key == *key) {
            output.extend_from_slice(&map[member.separator.end..member.value.start]);
        } else {
            serde_json::to_writer(&mut output, key)?;
            output.extend_from_slice(if inset.is_some() { b": " } else { b":" });
        }
        if let Some(request) = id.and_then(|id| edits.get(id)).copied().flatten() {
            if Some(request.id.as_str()) != *id || request.parent_id.as_deref() != folder_id {
                return Err(invalid("request ID or parent does not match its group"));
            }
            write_request(&mut output, request, inset, newline)?;
        } else {
            let member = member.expect("an untouched request has an existing member");
            output.extend_from_slice(&map[member.value.clone()]);
        }
    }
    let end = members.last().map_or(1, |entry| entry.value.end);
    if members.is_empty() && map.get(end) == Some(&b'}') && !order.is_empty() && !indent.is_empty()
    {
        output.extend_from_slice(newline);
        output.extend_from_slice(b"  ");
    }
    output.extend_from_slice(&contents[entries.start + end..]);
    Ok(output)
}

fn claim_request_key(slugs: &mut format::Slugs, request: &Request) -> String {
    if request.name.eq_ignore_ascii_case(super::NEW_REQUEST) {
        slugs.claim("", &format!("new-request-{}", request.id))
    } else {
        slugs.claim("", &request.name)
    }
}

fn write_request(
    output: &mut Vec<u8>,
    request: &Request,
    inset: Option<&[u8]>,
    newline: &[u8],
) -> Result<(), StorageError> {
    let mut request = serde_json::to_value(request)?;
    let fields = request
        .as_object_mut()
        .expect("Request serializes to an object");
    fields.shift_remove("parentId");
    if let Some(inset) = inset {
        let bytes = serde_json::to_vec_pretty(&request)?;
        for (index, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
            if index > 0 {
                output.extend_from_slice(newline);
                output.extend_from_slice(inset);
            }
            output.extend_from_slice(line);
        }
    } else {
        serde_json::to_writer(output, &request)?;
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope<'a> {
    folder_id: Option<String>,
    #[serde(borrow)]
    requests: &'a RawValue,
}

fn envelope(bytes: &[u8]) -> Result<(Option<String>, Range<usize>), StorageError> {
    let envelope: Envelope = format::decode_json(bytes)?;
    if !bytes.trim_ascii_start().starts_with(b"{") {
        return Err(invalid("expected a JSON object"));
    }
    if let Some(id) = &envelope.folder_id {
        format::validate_id(id)?;
    }
    let requests = envelope.requests.get();
    let start = requests.as_ptr() as usize - bytes.as_ptr() as usize;
    Ok((envelope.folder_id, start..start + requests.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: &str = "1bfa5325-d738-4fb0-8f64-5ea8947f80be";
    const MIDDLE: &str = "624e0ded-113b-4d78-8e10-f566bd77b347";
    const LAST: &str = "e2e71012-4617-4508-b139-18d38fd74c67";
    #[test]
    fn malformed_envelopes_cannot_be_edited() {
        for contents in [
            r#"[null,{}]"#,
            r#"{"requests":[]}"#,
            r#"{"requests":{}}{}"#,
            r#"{"requests":{},"requests":{}}"#,
        ] {
            assert!(edit(Some(contents.as_bytes()), None, &BTreeMap::new()).is_err());
        }
    }

    #[test]
    fn readable_keys_preserve_identity_and_untouched_hand_formatting() {
        let neighbor =
            format!(r#""zebra" : {{"id":"{LAST}","name":"Zebra","method":"GET","url":"/zebra"}}"#);
        let input = format!(
            "{{\r\n  \"requests\": {{\r\n    \"get-user\": {{\"id\":\"{FIRST}\",\"name\":\"Get user\",\"method\":\"GET\",\"url\":\"/users\"}},\r\n    {neighbor}\r\n  }}\r\n}}\r\n"
        );
        assert_eq!(
            edit(Some(input.as_bytes()), None, &BTreeMap::new()).unwrap(),
            input.as_bytes()
        );

        let group = parse(input.as_bytes()).unwrap();
        let mut renamed = group.requests[FIRST].as_ref().unwrap().clone();
        "View a user".clone_into(&mut renamed.name);
        let mut duplicate_name = group.requests[FIRST].as_ref().unwrap().clone();
        MIDDLE.clone_into(&mut duplicate_name.id);
        let edits = BTreeMap::from([
            (FIRST.into(), Some(&renamed)),
            (MIDDLE.into(), Some(&duplicate_name)),
        ]);
        let saved = edit(Some(input.as_bytes()), None, &edits).unwrap();
        let text = std::str::from_utf8(&saved).unwrap();
        assert!(text.contains(&neighbor));
        assert!(text.find("\"get-user\"").unwrap() < text.find("\"get-user-2\"").unwrap());
        assert_eq!(saved, edit(Some(input.as_bytes()), None, &edits).unwrap());
        let parsed = parse(&saved).unwrap();
        assert_eq!(parsed.requests[FIRST].as_ref().unwrap().name, "View a user");
        assert_eq!(parsed.requests[MIDDLE].as_ref().unwrap().name, "Get user");
    }

    #[test]
    fn duplicate_readable_keys_and_duplicate_uuids_are_reported_by_identity() {
        let input = format!(
            r#"{{"requests":{{
            "health":{{"id":"{FIRST}","name":"Health","method":"GET","url":"/"}},
            "he\u0061lth":{{"id":"{MIDDLE}","name":"Other","method":"GET","url":"/"}},
            "other":{{"id":"{FIRST}","name":"Other","method":"GET","url":"/"}},
            "zebra":{{"id":"{LAST}","name":"Zebra","method":"GET","url":"/"}}
        }}}}"#
        );
        let group = parse(input.as_bytes()).unwrap();
        assert_eq!(
            group.requests[FIRST].as_ref().unwrap_err(),
            "duplicate request ID"
        );
        assert!(group.duplicate_keys.contains("health"));
        assert_eq!(group.requests[MIDDLE].as_ref().unwrap().name, "Other");
        assert!(group.requests[LAST].is_ok());
    }
}
