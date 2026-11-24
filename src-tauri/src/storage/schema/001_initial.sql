CREATE TABLE IF NOT EXISTS collections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    auth_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS requests (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL,
    method TEXT NOT NULL,
    url TEXT NOT NULL,
    headers_json TEXT NOT NULL DEFAULT '[]',
    query_json TEXT NOT NULL DEFAULT '[]',
    path_params_json TEXT NOT NULL DEFAULT '[]',
    auth_json TEXT,
    body_json TEXT,
    pre_request_script_json TEXT,
    test_script_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_requests_collection
    ON requests(collection_id);

CREATE UNIQUE INDEX IF NOT EXISTS ux_requests_id_collection
    ON requests(id, collection_id);

CREATE TABLE IF NOT EXISTS collection_nodes (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL,
    parent_id TEXT,
    sort_order INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('folder', 'request')),
    name TEXT NOT NULL,
    request_id TEXT,
    auth_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (
        (kind = 'folder' AND request_id IS NULL)
        OR
        (kind = 'request' AND request_id IS NOT NULL)
    ),
    FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_id, collection_id) REFERENCES collection_nodes(id, collection_id) ON DELETE CASCADE,
    FOREIGN KEY (request_id, collection_id) REFERENCES requests(id, collection_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_collection_nodes_parent
    ON collection_nodes(collection_id, parent_id, sort_order);

CREATE UNIQUE INDEX IF NOT EXISTS ux_collection_nodes_id_collection
    ON collection_nodes(id, collection_id);

CREATE UNIQUE INDEX IF NOT EXISTS ux_collection_nodes_request_id
    ON collection_nodes(request_id)
    WHERE request_id IS NOT NULL;

-- An environment belongs to exactly one collection; there are no shared ones.
CREATE TABLE IF NOT EXISTS environments (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_environments_collection
    ON environments(collection_id);

CREATE TABLE IF NOT EXISTS variables (
    scope TEXT NOT NULL CHECK (scope IN ('global', 'collection', 'environment')),
    collection_id TEXT,
    environment_id TEXT,
    key TEXT NOT NULL,
    current_value TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    sensitive INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (
        (scope = 'global' AND collection_id IS NULL AND environment_id IS NULL)
        OR
        (scope = 'collection' AND collection_id IS NOT NULL AND environment_id IS NULL)
        OR
        (scope = 'environment' AND collection_id IS NULL AND environment_id IS NOT NULL)
    ),
    FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE,
    FOREIGN KEY (environment_id) REFERENCES environments(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_variables_global_key
    ON variables(key)
    WHERE scope = 'global';

CREATE UNIQUE INDEX IF NOT EXISTS ux_variables_collection_key
    ON variables(collection_id, key)
    WHERE scope = 'collection';

CREATE UNIQUE INDEX IF NOT EXISTS ux_variables_environment_key
    ON variables(environment_id, key)
    WHERE scope = 'environment';

CREATE INDEX IF NOT EXISTS idx_variables_context
    ON variables(scope, collection_id, environment_id, enabled);

CREATE TABLE IF NOT EXISTS workspace_state (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS repository_links (
    collection_id TEXT PRIMARY KEY NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    root TEXT NOT NULL UNIQUE,
    manifest_id TEXT NOT NULL,
    issue TEXT,
    watch_stamp TEXT
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS repository_files (
    collection_id TEXT NOT NULL REFERENCES repository_links(collection_id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('manifest', 'request', 'environment')),
    entity_id TEXT NOT NULL,
    contents BLOB NOT NULL,
    revision TEXT NOT NULL,
    PRIMARY KEY (collection_id, path)
);

CREATE INDEX IF NOT EXISTS idx_repository_files_entity
    ON repository_files(kind, entity_id, collection_id);

CREATE TABLE IF NOT EXISTS repository_request_groups (
    collection_id TEXT NOT NULL REFERENCES repository_links(collection_id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    folder_id TEXT NOT NULL,
    byte_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (collection_id, path)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_repository_groups_folder
    ON repository_request_groups(collection_id, folder_id);

CREATE TABLE IF NOT EXISTS repository_variables (
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

CREATE TABLE IF NOT EXISTS repository_pending_publishes (
    root TEXT PRIMARY KEY NOT NULL,
    manifest_id TEXT NOT NULL,
    source_collection_id TEXT
) WITHOUT ROWID;
