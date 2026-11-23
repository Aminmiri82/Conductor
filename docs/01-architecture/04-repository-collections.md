---
title: Repository Collections
description: How Conductor stores a shared collection in Git and keeps local values in SQLite.
---

A repository can carry one collection in its `.conductor` directory. Developers
edit it in Conductor or in their editor and use Git to share the changes.

## Files And Identity

- `collection.json` identifies the collection and declares `conductorFormat: 1`.
  It holds the collection name, collection auth, and folder definitions.
- `requests/root.json` holds requests at the collection root.
  Each folder, including each nested folder, has one
  `requests/<slug>-<folder-uuid>.json` file. Folder parents live in the manifest,
  so nesting or renaming a folder does not rename or rewrite its request file.
- `environments/<slug>-<uuid>.json` holds an environment and its public defaults.
  Start with an empty defaults map and add values intended for the team.

For example, `collection.json` can contain:

```json
{
  "conductorFormat": 1,
  "id": "cc09ff96-52a8-411a-9c1b-188801196f11",
  "name": "Example API",
  "folders": [
    {
      "id": "ed31c60a-7823-44f1-94d5-2faa28c87c4f",
      "name": "Users",
      "position": 1024
    }
  ]
}
```

The matching request file is `requests/users-ed31c60a-7823-44f1-94d5-2faa28c87c4f.json`:

```json
{
  "folderId": "ed31c60a-7823-44f1-94d5-2faa28c87c4f",
  "requests": {
    "086a9364-27c9-4cde-9b84-4d3682946aae": {
      "name": "List users",
      "position": 1024,
      "method": "GET",
      "url": "{{baseUrl}}/users"
    }
  }
}
```

The key is a request's permanent ID. In a file Conductor creates, new entries
go in UUID order and edits preserve the bytes of untouched entries. Conductor
keeps the entry order of a hand-edited map that is already unsorted. Git can
merge changes to separate regions of one file. Additions at the same insertion
point, including two appends, can still conflict. Sidebar ordering uses each
request's `position`, then its ID for ties.

Each link has its own SQLite IDs. Separate worktrees of the same repository can
be linked independently.

## Shared And Local Values

Request definitions are shared. Structured auth credentials and common
credential headers require variable references. Free text such as URLs and
bodies can contain arbitrary data and should be reviewed before committing.
Conductor strips file upload selections. Environment values edited in Conductor
are local overrides. Edit the defaults in the repository to share them
deliberately. Publishing a copy of a local collection keeps its existing
variable values in SQLite.

## Refresh And Conflicts

The first screen reads the SQLite cache. Conductor checks linked files after
startup and on window focus. Unchanged path, size, and modification metadata
let it skip reading file contents. A save checks the bytes and request revision
before it atomically replaces a folder file. Moving a request between folders
changes both files. An interrupted multi-file operation can be reconciled
from the repository.

A missing repository or unreadable manifest leaves the last good cache
available. Relink to a folder with the same collection ID to keep local values.
An invalid request in otherwise valid JSON is reported by file and ID while
valid siblings refresh. Invalid JSON leaves that group at its last good state.
A dirty draft keeps its original revision so it can be resolved against the
change found on disk.
