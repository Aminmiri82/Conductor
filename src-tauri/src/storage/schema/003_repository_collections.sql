ALTER TABLE environments ADD COLUMN archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1));

CREATE TABLE repository_links (
    collection_id TEXT PRIMARY KEY NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    root TEXT NOT NULL UNIQUE,
    manifest_id TEXT NOT NULL,
    issue TEXT,
    watch_stamp TEXT
) WITHOUT ROWID;

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
    byte_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (collection_id, path)
) WITHOUT ROWID;

CREATE INDEX idx_repository_groups_folder
    ON repository_request_groups(collection_id, folder_id);

CREATE TABLE repository_variables (
    collection_id TEXT NOT NULL REFERENCES repository_links(collection_id) ON DELETE CASCADE,
    scope_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT,
    enabled INTEGER NOT NULL,
    sensitive INTEGER NOT NULL,
    local INTEGER NOT NULL,
    shared_value TEXT,
    required_secret INTEGER NOT NULL,
    PRIMARY KEY (collection_id, scope_id, key)
) WITHOUT ROWID;

CREATE TABLE repository_pending_publishes (
    root TEXT PRIMARY KEY NOT NULL,
    manifest_id TEXT NOT NULL,
    source_collection_id TEXT
) WITHOUT ROWID;
