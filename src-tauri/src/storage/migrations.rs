use rusqlite::Connection;

use super::StorageError;

/// The current schema, for new databases. Keep it in step with the
/// upgrades below.
const INITIAL_SCHEMA: &str = include_str!("schema/001_initial.sql");
const ENVIRONMENT_COLLECTIONS: &str = include_str!("schema/002_environment_collections.sql");

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

    if !has_column(connection, "environments", "collection_id")? {
        rebuild_without_foreign_keys(
            connection,
            "environment_collections",
            ENVIRONMENT_COLLECTIONS,
        )?;
    }

    Ok(())
}

fn has_column(connection: &Connection, table: &str, column: &str) -> Result<bool, StorageError> {
    Ok(connection.query_row(
        "SELECT count(*) > 0 FROM pragma_table_info(?) WHERE name = ?",
        [table, column],
        |row| row.get(0),
    )?)
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

    /// A database from before environments belonged to collections: the
    /// current schema with the old shared table put back, at the version
    /// number an earlier build left (as high as 3).
    fn shared_environments_database(user_version: i64) -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(INITIAL_SCHEMA)
            .and_then(|()| {
                connection.execute_batch(&format!(
                    "DROP TABLE environments;
                     CREATE TABLE environments (
                         id TEXT PRIMARY KEY,
                         name TEXT NOT NULL,
                         created_at TEXT NOT NULL,
                         updated_at TEXT NOT NULL
                     );
                     PRAGMA user_version = {user_version};"
                ))
            })
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
}
