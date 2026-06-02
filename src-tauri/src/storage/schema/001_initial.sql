CREATE TABLE IF NOT EXISTS collections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'postman',
    auth_json TEXT,
    raw_postman_file_path TEXT,
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
    position INTEGER NOT NULL,
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
    ON collection_nodes(collection_id, parent_id, position);

CREATE UNIQUE INDEX IF NOT EXISTS ux_collection_nodes_id_collection
    ON collection_nodes(id, collection_id);

CREATE UNIQUE INDEX IF NOT EXISTS ux_collection_nodes_request_id
    ON collection_nodes(request_id)
    WHERE request_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS variables (
    scope TEXT NOT NULL CHECK (scope IN ('global', 'collection')),
    collection_id TEXT,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    sensitive INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (
        (scope = 'global' AND collection_id IS NULL)
        OR
        (scope = 'collection' AND collection_id IS NOT NULL)
    ),
    FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_variables_global_key
    ON variables(key)
    WHERE scope = 'global';

CREATE UNIQUE INDEX IF NOT EXISTS ux_variables_collection_key
    ON variables(collection_id, key)
    WHERE scope = 'collection';

CREATE INDEX IF NOT EXISTS idx_variables_context
    ON variables(scope, collection_id, enabled);

CREATE TABLE IF NOT EXISTS workspace_state (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS request_history (
    id TEXT PRIMARY KEY,
    request_id TEXT,
    collection_id TEXT,
    method TEXT NOT NULL,
    url TEXT NOT NULL,
    status_code INTEGER,
    duration_ms INTEGER,
    request_json TEXT NOT NULL,
    response_meta_json TEXT,
    response_body_path TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_request_history_created_at
    ON request_history(created_at DESC);
