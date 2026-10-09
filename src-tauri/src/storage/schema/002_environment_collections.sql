-- Environments used to be shared by every collection; now each belongs to
-- one. An existing environment has no owner to pick, so it is copied into
-- every collection, variables included, and the shared original removed.
-- With no collections at all, environments have nowhere to go and are dropped.
--
-- Runs with foreign keys off (SQLite's table-rebuild procedure): dropping the
-- old table with them on would cascade-delete every environment variable.

CREATE TEMP TABLE environment_copies AS
SELECT
    environments.id AS old_id,
    collections.id AS collection_id,
    lower(hex(randomblob(16))) AS new_id
FROM environments
CROSS JOIN collections;

CREATE TABLE environments_new (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE
);

INSERT INTO environments_new (id, collection_id, name, created_at, updated_at)
SELECT copies.new_id, copies.collection_id, environments.name,
       environments.created_at, environments.updated_at
FROM environment_copies AS copies
JOIN environments ON environments.id = copies.old_id;

INSERT INTO variables
    (scope, environment_id, key, current_value, enabled, sensitive, created_at, updated_at)
SELECT 'environment', copies.new_id, variables.key, variables.current_value,
       variables.enabled, variables.sensitive, variables.created_at, variables.updated_at
FROM environment_copies AS copies
JOIN variables
    ON variables.scope = 'environment' AND variables.environment_id = copies.old_id;

DELETE FROM variables
WHERE scope = 'environment'
  AND environment_id IN (SELECT id FROM environments);

DROP TABLE environments;
ALTER TABLE environments_new RENAME TO environments;

CREATE INDEX IF NOT EXISTS idx_environments_collection
    ON environments(collection_id);

DROP TABLE environment_copies;
