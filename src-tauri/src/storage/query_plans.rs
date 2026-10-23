//! Enforces "every query path needs an index" (docs/01-architecture/01-performance.md).
//!
//! Collects every SQL string literal from the non-test source, asks SQLite for
//! its query plan against the real schema, and fails on a full scan of a table
//! or index, or on an automatic index. A new query is covered as soon as it is
//! written. Foreign-key actions show up in the plan too, so a delete whose
//! cascade scans a child table fails here.

use std::{fs, path::Path};

use rusqlite::Connection;

use super::test_connection;

/// Queries that read a whole table on purpose, matched by a fragment of their
/// SQL. Each needs a reason a user with a large workspace won't feel it.
const INTENTIONAL_SCANS: &[(&str, &str)] = &[
    (
        "FROM collections ORDER BY updated_at DESC",
        "lists every collection for the sidebar; one row per collection",
    ),
    (
        "WHERE collection_id = collections.id) FROM collections ORDER BY updated_at DESC",
        "the settings page lists every collection with its counts; one row per \
         collection, and each count searches its child table by collection_id",
    ),
    (
        "FROM environments ORDER BY name",
        "lists every environment for the picker; one row per environment",
    ),
];

/// Scans that should get an index but haven't yet. Fix these rather than
/// adding to them; the test fails once a fix makes an entry unnecessary.
const KNOWN_GAPS: &[(&str, &str)] = &[
    (
        "JOIN subtree ON child.parent_id = subtree.id",
        "folder delete: idx_collection_nodes_parent leads with collection_id, so \
         SQLite builds a temporary index over every node on each delete",
    ),
    (
        "INSERT INTO environments (id, collection_id, name, created_at, updated_at) SELECT",
        "duplicating an environment checks variables by environment_id, which \
         only a partial index covers, so it scans idx_variables_context",
    ),
    (
        "DELETE FROM environments WHERE id = ?",
        "the cascade to variables scans idx_variables_context for the same reason",
    ),
    (
        "DELETE FROM collections WHERE id = ?",
        "the cascade to variables by collection_id has only the partial \
         ux_variables_collection_key, so it scans idx_variables_context",
    ),
];

#[test]
fn every_query_uses_an_index() {
    let connection = test_connection();
    let queries = source_queries();
    assert!(
        queries.len() > 20,
        "found only {} queries; the SQL literal scanner is probably broken",
        queries.len()
    );

    let mut problems = Vec::new();
    let mut gaps_seen = vec![false; KNOWN_GAPS.len()];
    for (location, sql) in &queries {
        let sql = squash(sql);
        if INTENTIONAL_SCANS
            .iter()
            .any(|(fragment, _)| sql.contains(fragment))
        {
            continue;
        }
        let gap = KNOWN_GAPS
            .iter()
            .position(|(fragment, _)| sql.contains(fragment));
        for step in query_plan(&connection, &sql) {
            if !is_unindexed(&sql, &step) {
                continue;
            }
            match gap {
                Some(index) => gaps_seen[index] = true,
                None => problems.push(format!("{location}: {step}\n    {sql}")),
            }
        }
    }
    for (seen, (fragment, _)) in gaps_seen.iter().zip(KNOWN_GAPS) {
        if !seen {
            problems.push(format!(
                "KNOWN_GAPS entry no longer scans; remove it: {fragment}"
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "queries without a usable index:\n{}",
        problems.join("\n")
    );
}

fn is_unindexed(sql: &str, step: &str) -> bool {
    if step.contains("AUTOMATIC") {
        return true;
    }
    // `SCAN x USING INDEX i` still reads every row; only SEARCH narrows. Plans
    // name a table by its alias when it has one, so anything scanned that
    // isn't a CTE of this query or a pragma's virtual table is stored data.
    step.strip_prefix("SCAN ")
        .and_then(|rest| rest.split_whitespace().next())
        .is_some_and(|name| !is_cte(sql, name) && !step.contains("VIRTUAL TABLE"))
}

/// Scanning a CTE reads rows the query just produced, not a stored table.
fn is_cte(sql: &str, name: &str) -> bool {
    [
        format!("WITH {name}"),
        format!("WITH RECURSIVE {name}"),
        format!(", {name} AS ("),
        format!(", {name}("),
    ]
    .iter()
    .any(|pattern| sql.contains(pattern))
}

fn query_plan(connection: &Connection, sql: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .unwrap_or_else(|error| panic!("cannot plan `{}`: {error}", squash(sql)));
    // Plans don't depend on parameter values, so bind them all to NULL.
    let nulls = vec![rusqlite::types::Null; statement.parameter_count()];
    let steps = statement
        .query_map(rusqlite::params_from_iter(nulls), |row| {
            row.get::<_, String>(3)
        })
        .expect("run query plan");
    steps.collect::<Result<_, _>>().expect("read query plan")
}

/// Modules compiled only under `#[cfg(test)]` in lib.rs and storage/mod.rs.
const TEST_ONLY_FILES: &[&str] = &[
    "bindings.rs",
    "fixtures.rs",
    "test_server.rs",
    "storage/query_plans.rs",
];

/// `(file:line, sql)` for every string literal in `src/` that starts with a
/// DML keyword, skipping test code.
fn source_queries() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rust_files(&root, &mut files);

    let mut queries = Vec::new();
    for path in files {
        let name = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .display()
            .to_string()
            // Windows paths use backslashes; TEST_ONLY_FILES uses slashes.
            .replace('\\', "/");
        if TEST_ONLY_FILES.contains(&name.as_str()) {
            continue;
        }
        let source = fs::read_to_string(&path).expect("read source file");
        // Each file's tests sit in a `mod tests` at the bottom.
        let source = source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or_default();
        for (offset, literal) in string_literals(source) {
            let keyword = literal.split_whitespace().next();
            if matches!(
                keyword,
                Some("SELECT" | "INSERT" | "UPDATE" | "DELETE" | "WITH")
            ) {
                let line = source[..offset].lines().count();
                queries.push((format!("{name}:{line}"), literal));
            }
        }
    }
    queries
}

fn collect_rust_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).expect("read source directory") {
        let path = entry.expect("read directory entry").path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

/// Plain `"..."` and raw `r#"..."#` literals with their byte offsets. Good
/// enough for our SQL, which never builds statements with `format!`.
fn string_literals(source: &str) -> Vec<(usize, String)> {
    let bytes = source.as_bytes();
    let mut literals = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index = source[index..]
                    .find('\n')
                    .map_or(bytes.len(), |end| index + end);
            }
            b'\'' => {
                // A char literal such as '"' must not open a string; lifetimes
                // like 'a have no closing quote within three bytes.
                index += match (bytes.get(index + 1), bytes.get(index + 2)) {
                    (Some(b'\\'), _) => source[index + 2..].find('\'').map_or(1, |end| end + 3),
                    (Some(_), Some(b'\'')) => 3,
                    _ => 1,
                };
            }
            b'r' if matches!(bytes.get(index + 1), Some(b'"' | b'#'))
                && !bytes.get(index.wrapping_sub(1)).is_some_and(|previous| {
                    previous.is_ascii_alphanumeric() || *previous == b'_'
                }) =>
            {
                let hashes =
                    source[index + 1..].len() - source[index + 1..].trim_start_matches('#').len();
                let open = index + 1 + hashes;
                if bytes.get(open) != Some(&b'"') {
                    index += 1;
                    continue;
                }
                let close = format!("\"{}", "#".repeat(hashes));
                let end = source[open + 1..]
                    .find(&close)
                    .map_or(bytes.len(), |end| open + 1 + end);
                literals.push((index, source[open + 1..end].to_string()));
                index = end + close.len();
            }
            b'"' => {
                let mut literal = String::new();
                let start = index;
                index += 1;
                while index < bytes.len() && bytes[index] != b'"' {
                    if bytes[index] == b'\\' {
                        index += 1;
                        match bytes.get(index) {
                            // A trailing backslash continues the string on the next line.
                            Some(b'\n') => {
                                while bytes.get(index + 1).is_some_and(u8::is_ascii_whitespace) {
                                    index += 1;
                                }
                            }
                            Some(b'n') => literal.push('\n'),
                            Some(b't') => literal.push('\t'),
                            Some(&other) => literal.push(other as char),
                            None => {}
                        }
                    } else {
                        let character = source[index..].chars().next().expect("char boundary");
                        literal.push(character);
                        index += character.len_utf8() - 1;
                    }
                    index += 1;
                }
                literals.push((start, literal));
                index += 1;
            }
            _ => index += 1,
        }
    }
    literals
}

/// Collapses whitespace so a query reads on one line and matches fragments
/// regardless of how it was indented in the source.
fn squash(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ")
}
