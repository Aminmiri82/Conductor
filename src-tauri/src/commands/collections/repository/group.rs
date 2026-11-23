use std::{
    collections::{BTreeMap, HashSet},
    ops::Range,
};

use crate::storage::StorageError;
use serde::{de::IgnoredAny, Deserialize};

use super::format::{self, Request};

pub(super) struct Group {
    pub(super) folder_id: Option<String>,
    pub(super) requests: BTreeMap<String, Result<Request, String>>,
}

struct Member {
    key: String,
    separator: Range<usize>,
    value: Range<usize>,
}

pub(super) fn parse(contents: &[u8]) -> Result<Group, StorageError> {
    let (folder_id, entries) = envelope(contents)?;
    let map = &contents[entries];
    let mut requests = BTreeMap::new();
    for member in object(map)? {
        let request = request(&map[member.value], &member.key, folder_id.as_deref())
            .map_err(|error| error.to_string());
        requests
            .entry(member.key)
            .and_modify(|previous| *previous = Err("duplicate request ID".into()))
            .or_insert(request);
    }
    Ok(Group {
        folder_id,
        requests,
    })
}

fn request(raw: &[u8], id: &str, folder_id: Option<&str>) -> Result<Request, StorageError> {
    if raw.first() != Some(&b'{') {
        return Err(invalid("request must be a JSON object"));
    }
    // The map key and the folder file are the only sources of identity and
    // parent. Inject both before typed deserialization checks duplicate fields.
    let mut bytes = Vec::with_capacity(raw.len() + 128);
    bytes.extend_from_slice(b"{\"id\":");
    serde_json::to_writer(&mut bytes, id)?;
    bytes.extend_from_slice(b",\"parentId\":");
    serde_json::to_writer(&mut bytes, &folder_id)?;
    if raw[whitespace(raw, 1)] != b'}' {
        bytes.push(b',');
    }
    bytes.extend_from_slice(&raw[1..]);
    format::parse_request(&bytes)
}

pub(super) fn edit(
    previous: Option<&[u8]>,
    folder_id: Option<&str>,
    edits: &BTreeMap<String, Option<&Request>>,
) -> Result<Vec<u8>, StorageError> {
    let empty = match folder_id {
        Some(id) => format!(
            "{{\n  \"folderId\": {},\n  \"requests\": {{}}\n}}\n",
            serde_json::to_string(id)?
        ),
        None => "{\n  \"requests\": {}\n}\n".to_string(),
    };
    let contents = previous.unwrap_or(empty.as_bytes());
    let (old_folder, entries) = envelope(contents)?;
    if old_folder.as_deref() != folder_id {
        return Err(invalid("request group folderId changed on disk"));
    }
    let map = &contents[entries.clone()];
    let members = object(map)?;
    let mut existing = HashSet::new();
    let mut order = Vec::new();
    for member in &members {
        if !existing.insert(member.key.as_str()) && edits.contains_key(&member.key) {
            return Err(invalid(format!("request ID {} is duplicated", member.key)));
        }
        if !matches!(edits.get(&member.key), Some(None)) {
            order.push((member.key.as_str(), Some(member)));
        }
    }
    order.extend(
        edits
            .iter()
            .filter(|(id, request)| request.is_some() && !existing.contains(id.as_str()))
            .map(|(id, _)| (id.as_str(), None)),
    );
    if members.windows(2).all(|pair| pair[0].key <= pair[1].key) {
        order.sort_by_key(|(id, _)| *id);
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
    for (position, (id, member)) in order.iter().enumerate() {
        let separator = member.map_or(new_separator, |member| &map[member.separator.clone()]);
        write_separator(&mut output, separator, position > 0);
        let inset = separator
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map(|newline| &separator[newline + 1..]);
        match (member, edits.get(*id).copied().flatten()) {
            (Some(member), None) => {
                output.extend_from_slice(&map[member.separator.end..member.value.end]);
            }
            (member, Some(request)) => {
                if request.id != *id || request.parent_id.as_deref() != folder_id {
                    return Err(invalid("request ID or parent does not match its group"));
                }
                if let Some(member) = member {
                    output.extend_from_slice(&map[member.separator.end..member.value.start]);
                } else {
                    serde_json::to_writer(&mut output, id)?;
                    output.extend_from_slice(if inset.is_some() { b": " } else { b":" });
                }
                write_request(&mut output, request, inset, newline)?;
            }
            (None, None) => unreachable!("only new requests can have no existing member"),
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
    fields.shift_remove("id");
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

fn write_separator(output: &mut Vec<u8>, bytes: &[u8], needs_comma: bool) {
    match (needs_comma, bytes.iter().position(|byte| *byte == b',')) {
        (true, None) => {
            output.push(b',');
            output.extend_from_slice(bytes);
        }
        (false, Some(comma)) => {
            output.extend_from_slice(&bytes[..comma]);
            output.extend_from_slice(&bytes[comma + 1..]);
        }
        _ => output.extend_from_slice(bytes),
    }
}

fn envelope(bytes: &[u8]) -> Result<(Option<String>, Range<usize>), StorageError> {
    let mut folder = None;
    let mut requests = None;
    for member in object(bytes)? {
        match member.key.as_str() {
            "folderId" if folder.is_none() => {
                let id: Option<String> = serde_json::from_slice(&bytes[member.value])?;
                if let Some(id) = &id {
                    format::validate_id(id)
                        .map_err(|_| invalid("folderId must be a lowercase hyphenated UUID"))?;
                }
                folder = Some(id);
            }
            "requests" if requests.is_none() => requests = Some(member.value),
            _ => {
                return Err(invalid(format!(
                    "unknown or duplicate group field {}",
                    member.key
                )))
            }
        }
    }
    Ok((
        folder.flatten(),
        requests.ok_or_else(|| invalid("request group needs a requests object"))?,
    ))
}

// serde_json finds the exact end of each JSON string/value and validates nested
// values. The offsets preserve unsaved formatting in neighboring members.
fn object(bytes: &[u8]) -> Result<Vec<Member>, StorageError> {
    let open = whitespace(bytes, 0);
    if bytes.get(open) != Some(&b'{') {
        return Err(invalid("expected a JSON object"));
    }
    let mut separator_start = open + 1;
    let mut cursor = whitespace(bytes, separator_start);
    let mut members = Vec::new();
    if bytes.get(cursor) != Some(&b'}') {
        loop {
            let start = cursor;
            let key = token::<String>(bytes, &mut cursor)?;
            cursor = whitespace(bytes, cursor);
            if bytes.get(cursor) != Some(&b':') {
                return Err(invalid("expected ':' after JSON object key"));
            }
            cursor = whitespace(bytes, cursor + 1);
            let value_start = cursor;
            token::<IgnoredAny>(bytes, &mut cursor)?;
            members.push(Member {
                key,
                separator: separator_start..start,
                value: value_start..cursor,
            });
            separator_start = cursor;
            cursor = whitespace(bytes, cursor);
            match bytes.get(cursor) {
                Some(b',') => cursor = whitespace(bytes, cursor + 1),
                Some(b'}') => break,
                _ => return Err(invalid("expected ',' or '}' after JSON object member")),
            }
        }
    }
    if whitespace(bytes, cursor + 1) != bytes.len() {
        return Err(invalid("unexpected content after JSON object"));
    }
    Ok(members)
}

fn token<'a, T: Deserialize<'a>>(bytes: &'a [u8], cursor: &mut usize) -> Result<T, StorageError> {
    let mut reader = serde_json::Deserializer::from_slice(&bytes[*cursor..]).into_iter();
    let token = reader
        .next()
        .ok_or_else(|| invalid("unexpected end of JSON"))??;
    *cursor += reader.byte_offset();
    Ok(token)
}

fn whitespace(bytes: &[u8], mut cursor: usize) -> usize {
    while matches!(bytes.get(cursor), Some(b' ' | b'\r' | b'\n' | b'\t')) {
        cursor += 1;
    }
    cursor
}

fn invalid(message: impl Into<String>) -> StorageError {
    StorageError::InvalidInput(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: &str = "1bfa5325-d738-4fb0-8f64-5ea8947f80be";
    const MIDDLE: &str = "624e0ded-113b-4d78-8e10-f566bd77b347";
    const LAST: &str = "e2e71012-4617-4508-b139-18d38fd74c67";
    const FOLDER: &str = "9044034c-c9f3-420f-aa98-666181dc97e4";
    const DETAIL: &str = r#"{"name":"Health","method":"GET","url":"{{baseUrl}}/health"}"#;

    #[test]
    fn editing_one_entry_preserves_the_raw_bytes_of_its_neighbor() {
        let input = format!(
            "{{\r\n  \"requests\": {{\r\n    \"{FIRST}\": {DETAIL},\r\n    \"{LAST}\" :  {DETAIL}\r\n  }}\r\n}}\r\n"
        );
        let mut request = request(DETAIL.as_bytes(), FIRST, None).unwrap();
        request.name = "Readiness".into();
        let next = edit(
            Some(input.as_bytes()),
            None,
            &BTreeMap::from([(FIRST.into(), Some(&request))]),
        )
        .unwrap();
        let expected = format!(
            "{{\r\n  \"requests\": {{\r\n    \"{FIRST}\": {{\r\n      \"name\": \"Readiness\",\r\n      \"position\": 0,\r\n      \"method\": \"GET\",\r\n      \"url\": \"{{{{baseUrl}}}}/health\"\r\n    }},\r\n    \"{LAST}\" :  {DETAIL}\r\n  }}\r\n}}\r\n"
        );
        assert_eq!(next, expected.as_bytes());
        assert_eq!(parse(&next).unwrap().requests.len(), 2);
    }

    #[test]
    fn inserting_then_deleting_an_entry_restores_the_original_bytes() {
        let first = request(DETAIL.as_bytes(), FIRST, Some(FOLDER)).unwrap();
        let last = request(DETAIL.as_bytes(), LAST, Some(FOLDER)).unwrap();
        let middle = request(DETAIL.as_bytes(), MIDDLE, Some(FOLDER)).unwrap();
        let original = edit(
            None,
            Some(FOLDER),
            &BTreeMap::from([(LAST.into(), Some(&last)), (FIRST.into(), Some(&first))]),
        )
        .unwrap();
        let next = edit(
            Some(&original),
            Some(FOLDER),
            &BTreeMap::from([(MIDDLE.into(), Some(&middle))]),
        )
        .unwrap();
        let text = std::str::from_utf8(&next).unwrap();
        assert!(text.find(FIRST).unwrap() < text.find(MIDDLE).unwrap());
        assert!(text.find(MIDDLE).unwrap() < text.find(LAST).unwrap());
        let group = parse(&next).unwrap();
        assert_eq!(group.folder_id.as_deref(), Some(FOLDER));
        assert_eq!(group.requests[MIDDLE].as_ref().unwrap().id, MIDDLE);
        assert_eq!(
            edit(
                Some(&next),
                Some(FOLDER),
                &BTreeMap::from([(MIDDLE.into(), None)]),
            )
            .unwrap(),
            original
        );
    }

    #[test]
    fn a_bad_or_duplicate_member_leaves_valid_requests_editable() {
        let input = format!(
            r#"{{"requests":{{"{FIRST}":17,"{MIDDLE}":{DETAIL},"{MIDDLE}":{DETAIL},"{LAST}":{DETAIL}}}}}"#
        );
        let group = parse(input.as_bytes()).unwrap();
        assert!(group.requests[FIRST].is_err());
        assert!(group.requests[MIDDLE].is_err());
        let mut last = group.requests[LAST].as_ref().unwrap().clone();
        last.name = "Different".into();
        let next = edit(
            Some(input.as_bytes()),
            None,
            &BTreeMap::from([(LAST.into(), Some(&last))]),
        )
        .unwrap();
        let next = std::str::from_utf8(&next).unwrap();
        assert!(next.contains(&format!(
            r#""{FIRST}":17,"{MIDDLE}":{DETAIL},"{MIDDLE}":{DETAIL}"#
        )));
        assert_eq!(
            parse(next.as_bytes()).unwrap().requests[LAST]
                .as_ref()
                .unwrap()
                .name,
            "Different"
        );
    }

    #[test]
    fn the_group_key_and_folder_cannot_be_overridden_by_request_fields() {
        let input = format!(
            r#"{{"requests":{{"{FIRST}":{{"id":"{MIDDLE}","name":"Health","method":"GET","url":"/"}},"{LAST}":{DETAIL}}}}}"#
        );
        let group = parse(input.as_bytes()).unwrap();
        assert!(group.requests[FIRST].is_err());
        assert!(group.requests[LAST].is_ok());
    }
}
