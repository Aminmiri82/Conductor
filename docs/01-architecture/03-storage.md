---
title: Storage
description: Where Conductor keeps its data, how to change the schema without losing anyone's data, and how to run against a throwaway database.
---

Conductor keeps everything in one SQLite database, `conductor.sqlite3`, in the
app data directory. A schema change must upgrade
existing databases in place.

## Where The Data Lives

Release and debug builds use separate data directories, so a dev run never
touches installed collections or secrets. [`lib.rs`](../../src-tauri/src/lib.rs)
picks the directory. On macOS:

- Release: `~/Library/Application Support/com.yaramiri.conductor/`
- Debug, including `pnpm app:dev`: `~/Library/Application Support/com.yaramiri.conductor.dev/`

Set `CONDUCTOR_DATA_DIR` to point any run somewhere else:

```bash
CONDUCTOR_DATA_DIR="$(mktemp -d)" pnpm app:dev
```

Agents use `pnpm agent:start`, which does this for them (see the
[verify-in-app skill](../../.agents/skills/verify-in-app/SKILL.md)).

## Changing The Schema

[`migrations.rs`](../../src-tauri/src/storage/migrations.rs) creates new
databases and upgrades old ones. Every schema change needs both:

1. Update [`schema/001_initial.sql`](../../src-tauri/src/storage/schema/001_initial.sql),
   which stays the full current schema for new databases.
2. Add an upgrade step in `schema/` and run it from `migrations.rs`, so
   existing databases reach the same shape without losing rows.
3. Add a test that builds a database in the old shape, upgrades it, and
   checks the data survived.

The upgrade must leave the database as it was if it fails, and a database
that is already up to date must not change. `migrations.rs` explains how
steps decide whether to run and how to rebuild a table other tables reference.
