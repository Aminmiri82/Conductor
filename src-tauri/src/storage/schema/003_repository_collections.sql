DROP INDEX IF EXISTS idx_collection_nodes_parent;
CREATE INDEX idx_collection_nodes_parent
    ON collection_nodes(collection_id, parent_id, sort_order, id);

CREATE TABLE repository_links (
    collection_id TEXT PRIMARY KEY NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    root TEXT NOT NULL UNIQUE,
    manifest_id TEXT NOT NULL,
    issue TEXT,
    watch_stamp TEXT
);

CREATE TABLE repository_files (
    collection_id TEXT NOT NULL REFERENCES repository_links(collection_id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('manifest', 'request', 'environment')),
    entity_id TEXT NOT NULL,
    contents BLOB NOT NULL,
    revision TEXT NOT NULL,
    PRIMARY KEY (collection_id, path)
);

CREATE INDEX idx_repository_files_entity
    ON repository_files(kind, entity_id, collection_id);

CREATE TABLE repository_request_groups (
    collection_id TEXT NOT NULL REFERENCES repository_links(collection_id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    folder_id TEXT NOT NULL,
    contents BLOB NOT NULL,
    PRIMARY KEY (collection_id, path)
);

CREATE TABLE repository_environments (
    environment_id TEXT PRIMARY KEY NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    active INTEGER NOT NULL CHECK (active IN (0, 1))
);

CREATE TABLE repository_defaults (
    environment_id TEXT NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (environment_id, key)
);

CREATE TABLE repository_default_hides (
    environment_id TEXT NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    PRIMARY KEY (environment_id, key)
);

CREATE VIEW repository_default_values AS
    SELECT defaults.environment_id, defaults.key, defaults.value
    FROM repository_defaults AS defaults
    WHERE NOT EXISTS (
        SELECT 1 FROM variables
        WHERE scope = 'environment'
          AND environment_id = defaults.environment_id AND key = defaults.key
    ) AND NOT EXISTS (
        SELECT 1 FROM repository_default_hides AS hides
        WHERE hides.environment_id = defaults.environment_id AND hides.key = defaults.key
    );
