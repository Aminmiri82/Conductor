use rusqlite::Connection;

use super::StorageError;

/// The current schema, for new databases. Keep it in step with the
/// upgrades below.
const INITIAL_SCHEMA: &str = include_str!("schema/001_initial.sql");
const ENVIRONMENT_COLLECTIONS: &str = include_str!("schema/002_environment_collections.sql");
const REPOSITORY_COLLECTIONS: &str = include_str!("schema/003_repository_collections.sql");

/// Upgrades are keyed on the schema's shape, not `user_version`: earlier
/// builds numbered their own migrations up to 3 before the schema was reset
/// to `001_initial.sql`, so a version number says nothing about the tables.
pub fn run(connection: &Connection) -> Result<(), StorageError> {
    let current_version =
        connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;

    if current_version < 1 {
        connection.execute_batch(INITIAL_SCHEMA)?;
        connection.pragma_update(None, "user_version", 1)?;
        return Ok(());
    }

    let (has_environment_owner, has_repository_links) = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('environments') WHERE name = 'collection_id'),
                EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'repository_links')",
        [],
        |row| Ok((row.get::<_, bool>(0)?, row.get::<_, bool>(1)?)),
    )?;
    if !has_environment_owner {
        rebuild_without_foreign_keys(
            connection,
            "environment_collections",
            ENVIRONMENT_COLLECTIONS,
        )?;
    }

    if !has_repository_links {
        let tx = connection.unchecked_transaction()?;
        tx.execute_batch(REPOSITORY_COLLECTIONS)?;
        tx.commit()?;
    }

    Ok(())
}

/// SQLite's procedure for changing a table other tables reference: foreign
/// keys off (only possible outside a transaction), rebuild in a transaction,
/// check every reference still holds, then turn them back on.
fn rebuild_without_foreign_keys(
    connection: &Connection,
    name: &str,
    migration: &str,
) -> Result<(), StorageError> {
    connection.pragma_update(None, "foreign_keys", "OFF")?;
    let result = (|| {
        connection.execute_batch("BEGIN")?;
        connection.execute_batch(migration)?;
        let broken: i64 =
            connection.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })?;
        if broken > 0 {
            return Err(StorageError::InvalidInput(format!(
                "migration {name} left {broken} broken references"
            )));
        }
        connection.execute_batch("COMMIT")?;
        Ok(())
    })();
    if result.is_err() {
        // Leaves the database as it was.
        let _ = connection.execute_batch("ROLLBACK");
    }
    connection.pragma_update(None, "foreign_keys", "ON")?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database_before_repository_links() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(INITIAL_SCHEMA)
            .and_then(|()| {
                connection.execute_batch(
                    "DROP INDEX idx_collection_nodes_parent;
                     CREATE INDEX idx_collection_nodes_parent
                         ON collection_nodes(collection_id, parent_id, sort_order);
                     DROP VIEW repository_default_values;
                     DROP TABLE repository_default_hides;
                     DROP TABLE repository_defaults;
                     DROP TABLE repository_environments;
                     DROP TABLE repository_request_groups;
                     DROP TABLE repository_files;
                     DROP TABLE repository_links;
                     PRAGMA user_version = 1;",
                )
            })
            .unwrap();
        connection
    }

    /// A database from before environments belonged to collections: the
    /// current schema with the old shared table put back, at the version
    /// number an earlier build left (as high as 3).
    fn shared_environments_database(user_version: i64) -> Connection {
        let connection = database_before_repository_links();
        connection
            .execute_batch(&format!(
                "DROP TABLE environments;
                     CREATE TABLE environments (
                         id TEXT PRIMARY KEY,
                         name TEXT NOT NULL,
                         created_at TEXT NOT NULL,
                         updated_at TEXT NOT NULL
                     );
                     PRAGMA user_version = {user_version};"
            ))
            .unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        connection
    }

    #[test]
    fn a_shared_environment_is_copied_into_every_collection_with_its_variables() {
        let connection = shared_environments_database(3);
        connection
            .execute_batch(
                "INSERT INTO collections (id, name, created_at, updated_at)
                 VALUES ('a', 'A', '', ''), ('b', 'B', '', '');
                 INSERT INTO environments (id, name, created_at, updated_at)
                 VALUES ('staging', 'Staging', '', '');
                 INSERT INTO variables
                     (scope, environment_id, key, current_value, sensitive, created_at, updated_at)
                 VALUES ('environment', 'staging', 'token', 'secret', 1, '', ''),
                        ('environment', 'staging', 'host', 'h', 0, '', '');
                 INSERT INTO variables (scope, collection_id, key, current_value, created_at, updated_at)
                 VALUES ('collection', 'a', 'kept', 'yes', '', '');",
            )
            .unwrap();

        run(&connection).unwrap();

        let copies = connection
            .prepare(
                "SELECT environments.collection_id, environments.name,
                        group_concat(variables.key || '=' || variables.current_value
                                     || ':' || variables.sensitive, ',')
                 FROM environments
                 JOIN (SELECT * FROM variables ORDER BY key) AS variables
                     ON variables.environment_id = environments.id
                 GROUP BY environments.id
                 ORDER BY environments.collection_id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let expected = |collection: &str| {
            (
                collection.to_string(),
                "Staging".to_string(),
                "host=h:0,token=secret:1".to_string(),
            )
        };
        assert_eq!(copies, [expected("a"), expected("b")]);

        let others: Vec<String> = connection
            .prepare("SELECT key FROM variables WHERE scope != 'environment'")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(others, ["kept"]);

        // The next start finds the new shape and changes nothing.
        run(&connection).unwrap();
        let environments: i64 = connection
            .query_row("SELECT count(*) FROM environments", [], |row| row.get(0))
            .unwrap();
        assert_eq!(environments, 2);
    }

    /// The path a real start takes: a database file left by an earlier
    /// build is upgraded by `Database::open`, and a later start keeps it.
    #[test]
    fn an_old_database_file_is_upgraded_on_open_and_survives_a_restart() {
        let directory =
            std::env::temp_dir().join(format!("conductor-old-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let old = shared_environments_database(3);
        old.execute_batch(
            "INSERT INTO collections (id, name, created_at, updated_at) VALUES ('a', 'A', '', '');
             INSERT INTO environments (id, name, created_at, updated_at) VALUES ('e', 'E', '', '');
             INSERT INTO variables
                 (scope, environment_id, key, current_value, created_at, updated_at)
             VALUES ('environment', 'e', 'k', 'v', '', '');",
        )
        .unwrap();
        old.execute(
            "VACUUM INTO ?",
            [directory.join("conductor.sqlite3").to_str().unwrap()],
        )
        .unwrap();

        let rows = || {
            crate::storage::Database::open(&directory)
                .unwrap()
                .with_read_connection(|connection| {
                    Ok(connection.query_row(
                        "SELECT environments.collection_id, variables.key, variables.current_value
                         FROM environments JOIN variables ON variables.environment_id = environments.id",
                        [],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
                    )?)
                })
                .unwrap()
        };
        let expected = ("a".to_string(), "k".to_string(), "v".to_string());
        assert_eq!(rows(), expected);
        assert_eq!(rows(), expected);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_migrated_environment_is_deleted_with_its_collection() {
        let connection = shared_environments_database(1);
        connection
            .execute_batch(
                "INSERT INTO collections (id, name, created_at, updated_at) VALUES ('a', 'A', '', '');
                 INSERT INTO environments (id, name, created_at, updated_at) VALUES ('e', 'E', '', '');
                 INSERT INTO variables
                     (scope, environment_id, key, current_value, created_at, updated_at)
                 VALUES ('environment', 'e', 'k', 'v', '', '');",
            )
            .unwrap();
        run(&connection).unwrap();

        connection
            .execute("DELETE FROM collections WHERE id = 'a'", [])
            .unwrap();

        let left: i64 = connection
            .query_row(
                "SELECT (SELECT count(*) FROM environments) + (SELECT count(*) FROM variables)",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(left, 0);
    }

    #[test]
    fn repository_migration_preserves_existing_collection_data() {
        let connection = database_before_repository_links();
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                     INSERT INTO collections (id, name, created_at, updated_at)
                     VALUES ('col', 'Before', '', '');
                     INSERT INTO requests (id, collection_id, method, url, created_at, updated_at)
                     VALUES ('req', 'col', 'GET', '{{host}}', '', '');
                     INSERT INTO collection_nodes
                         (id, collection_id, sort_order, kind, name, request_id, created_at, updated_at)
                     VALUES ('node', 'col', 1, 'request', 'Request', 'req', '', '');
                     INSERT INTO environments (id, collection_id, name, created_at, updated_at)
                     VALUES ('env', 'col', 'Staging', '', '');
                     INSERT INTO variables
                         (scope, environment_id, key, current_value, sensitive, created_at, updated_at)
                     VALUES ('environment', 'env', 'token', 'private', 1, '', '');",
            )
            .unwrap();

        run(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO repository_links (collection_id, root, manifest_id)
                 VALUES ('col', '/repo', 'manifest')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO repository_request_groups (collection_id, path, folder_id, contents)
                 VALUES ('col', 'requests.json', '', ?)",
                [b"[]".as_slice()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO repository_defaults (environment_id, key, value)
                 VALUES ('env', 'host', 'shared')",
                [],
            )
            .unwrap();
        run(&connection).unwrap();

        let stored: (String, String, String, i64, String, Vec<u8>) = connection
            .query_row(
                "SELECT collections.name, requests.url, variables.current_value,
                        variables.sensitive, repository_defaults.value,
                        repository_request_groups.contents
                 FROM repository_links
                 JOIN repository_request_groups
                    ON repository_request_groups.collection_id = repository_links.collection_id
                 JOIN collections ON collections.id = repository_links.collection_id
                 JOIN collection_nodes ON collection_nodes.collection_id = collections.id
                 JOIN requests ON requests.id = collection_nodes.request_id
                 JOIN environments ON environments.collection_id = collections.id
                 JOIN variables ON variables.environment_id = environments.id
                 JOIN repository_defaults ON repository_defaults.environment_id = environments.id",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            stored,
            (
                "Before".to_string(),
                "{{host}}".to_string(),
                "private".to_string(),
                1,
                "shared".to_string(),
                b"[]".to_vec(),
            )
        );
    }
}
