---
title: Storage
description: Where Conductor keeps its data, how the schema changes before 1.0, and how to run against a throwaway database.
---

Conductor keeps everything in one SQLite database, `conductor.sqlite3`, in the
app data directory.

## Where The Data Lives

On macOS the installed release app uses
`~/Library/Application Support/com.yaramiri.conductor/`. A debug build, including
`pnpm tauri dev`, uses
`~/Library/Application Support/com.yaramiri.conductor.dev/` instead. The dev
directory starts empty; existing release collections, environments, and secrets
stay in the release directory. This selection is made in
[`lib.rs`](../../src-tauri/src/lib.rs).

Set `CONDUCTOR_DATA_DIR` to point any run, debug or release, somewhere else:

```bash
CONDUCTOR_DATA_DIR="$(mktemp -d)" pnpm tauri dev
```

Agents use `pnpm agent:start`, which does this with `.agent-app/data` (see
the [verify-in-app skill](../../.agents/skills/verify-in-app/SKILL.md)).

## Changing The Schema

Conductor is pre-production and does not preserve backwards compatibility,
including the database. To change the schema:

1. Edit [`schema/001_initial.sql`](../../src-tauri/src/storage/schema/001_initial.sql)
   in place.
2. Start from an empty data directory (a fresh `CONDUCTOR_DATA_DIR`) to pick
   it up. Existing databases are not upgraded.

Do not add new migration steps to
[`migrations.rs`](../../src-tauri/src/storage/migrations.rs) unless Yara asks.
When she does, `001_initial.sql` stays the full current schema for new
databases, and the step only upgrades older ones. For example,
`002_environment_collections.sql` moved environments under collections.
Steps check the schema's shape (does a column exist?) rather than
`user_version`, because earlier builds left version numbers up to 3 that no
longer mean anything. A
step that rebuilds a table other tables reference runs with foreign keys off
(see `rebuild_without_foreign_keys`), because dropping that table with them
on would cascade-delete its dependants.

When a schema change breaks Yara's existing database, say so in the change
description so she knows to reset it.
