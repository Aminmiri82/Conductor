CREATE TABLE IF NOT EXISTS collections (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'postman',
    auth_json TEXT,
    raw_postman_file_path TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS requests (
    id INTEGER PRIMARY KEY,
    collection_id INTEGER NOT NULL,
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
    id INTEGER PRIMARY KEY,
    collection_id INTEGER NOT NULL,
    parent_id INTEGER,
    sort_order INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('folder', 'request')),
    name TEXT NOT NULL,
    request_id INTEGER,
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

CREATE TABLE IF NOT EXISTS environments (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'manual',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS variables (
    scope TEXT NOT NULL CHECK (scope IN ('global', 'collection', 'environment')),
    collection_id INTEGER,
    environment_id INTEGER,
    key TEXT NOT NULL,
    initial_value TEXT,
    current_value TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    sensitive INTEGER NOT NULL DEFAULT 0,
    variable_type TEXT,
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

CREATE TABLE IF NOT EXISTS request_history (
    id INTEGER PRIMARY KEY,
    request_id INTEGER,
    collection_id INTEGER,
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

CREATE TABLE IF NOT EXISTS learned_index_models (
    index_name TEXT NOT NULL DEFAULT '',
    table_name TEXT NOT NULL,
    column_name TEXT NOT NULL,
    root_page INTEGER NOT NULL,
    model_json TEXT NOT NULL,
    row_count INTEGER NOT NULL,
    max_error INTEGER NOT NULL,
    built_at TEXT NOT NULL,
    stale INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (table_name, column_name)
);
