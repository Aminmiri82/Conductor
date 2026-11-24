---
title: Repository Collections
description: Share request definitions and public defaults through Git while keeping credentials in an ignored local file.
---

A team can keep one collection in its repository. Conductor or a text editor
can edit the shared files, and Git carries them across clones and branches.

## First Request From A Fresh Clone

The shared directory looks like this:

```text
.conductor/
  .gitignore
  collection.json
  requests/
    root.json
    users.json
  environments/
    development.json
  local.json
```

Open the repository root or its `.conductor` directory with **Open from
repository…**. This example uses a local server that accepts
`GET /health`. The team commits the following manifest, request, environment,
and ignore rules.

`collection.json` identifies the collection and can include shared collection
variables and auth.

```json
{
  "conductorFormat": 2,
  "id": "cc09ff96-52a8-411a-9c1b-188801196f11",
  "name": "Example API",
  "values": { "apiVersion": "v1" }
}
```

`requests/root.json` holds requests at the collection root.

```json
{
  "requests": {
    "health": {
      "id": "086a9364-27c9-4cde-9b84-4d3682946aae",
      "name": "Health",
      "method": "GET",
      "url": "{{baseUrl}}/health",
      "auth": { "authType": "bearer", "token": "{{token}}" }
    }
  }
}
```

`environments/development.json` supplies a public URL and declares a required
secret by key. Both the manifest and environment files support `values` and
`secrets`.

```json
{
  "id": "9ce8609b-e932-4678-9d57-5ff4f50b9736",
  "name": "Development",
  "values": { "baseUrl": "http://127.0.0.1:18766" },
  "secrets": ["token"]
}
```

`.gitignore` belongs in `.conductor`. Conductor ensures these rules exist
when creating or linking the repository. After a branch change, refresh restores a
missing rules file if private values are still present. Keep the rules on
shared branches so Git protects the file before Conductor refreshes.

```gitignore
/local.json
/.local.json-*.tmp
.conductor-*.tmp
.*.conductor-deleted-*.tmp
```

Create `local.json` with your own token or enter it in the Variables tab.
Environment entries use the filename stem. For
`environments/development.json`, use `development` as the key.

```json
{
  "environments": {
    "development": { "token": "your-local-token" }
  }
}
```

Select **Development**, open **Health**, and send the request. A
`collection` map at the top level of `local.json` can override
collection-scope defaults. Strings are sufficient for ordinary values. An
entry such as `{ "value": "...", "enabled": false }` disables a key at
that scope. Add `"sensitive": true` to mask a private entry in the app.
Declared secrets are masked automatically. **Use shared value** removes the
local key and shows the team default again. Removing the key in a text editor
has the same effect on refresh. **Edit shared value** writes a public default
to the collection or environment file. **Declare required secret** in
Settings → Variables adds the key to the shared file. Teammates enter their
own values locally.

Conductor keeps the filename when you change an environment's display name.
If you rename or remove its file on disk, Conductor reports the old key as
orphaned on refresh and leaves the entry in place. Rename the key in
`local.json` to move those values to the new stem. The file at a matching
stem receives its local overrides, including when a branch restores that file.
Each stem identifies one file in the flat `environments/` directory. Conductor
adds a numeric suffix to avoid collisions with existing files and local keys
when creating environments. Repeated keys make `local.json` invalid for
Conductor, which reports a parse error and leaves the file intact. Variable
edits preserve the bytes of untouched entries.

Conductor verifies that the local file is ignored and untracked before
writing. It blocks private writes if `.conductor` is itself a Git checkout.
It syncs new private bytes outside the Git working tree and installs them
atomically. Share private values through a password manager when needed.
Structured auth fields and recognized credential headers require variable
references in the shared files. Before committing, review free-form fields
such as URLs, bodies, and scripts, as well as custom or unrecognized headers.
Conductor strips file upload paths.

Publish stages its files in a sibling directory so it can install the
collection with one rename. The staging directory writes its own ignore
rules before placing collection files there.

When publishing a copy, review which variable keys will carry shared values
and which will be declared as required secrets. You can move any proposed
shared key to the required list before confirming. When exporting a linked
collection to Postman, Conductor includes shared defaults and required
secret names. It omits values from `local.json`. Ordinary collections
continue to export their stored variables.

## Request And Folder Identity

The manifest defines folders by UUID and optional parent ID. A folder request
file carries its `folderId` and can live at a nested path such as
`requests/users/admin.json`. Conductor derives readable keys and paths from
the first real name. Placeholder requests and populated placeholder folders
use unique temporary keys until their first rename. Later renames preserve
the readable key. A request move preserves its UUID and uses its existing key or a numeric
suffix if that key is taken. Edits preserve the bytes of untouched request
entries. If a merge gives two request UUIDs the same key, refresh shows both
requests and reports the collision. The next save in that group assigns
distinct keys.

Only `conductorFormat: 2` is supported. Each linked checkout has its own
SQLite cache and app-side IDs. **Remove from this device** removes the cache
and link. The shared files and `local.json` remain in the repository folder.

When you have unsaved request drafts, Conductor asks before discarding them
as part of switching, importing, creating, or opening a collection. This
applies to local and linked collections.

## Write Order And Refresh

Each replacement compares the current bytes against the bytes read for the
edit. It syncs a temporary file before installing it. Conductor serializes
writers for a root. On Unix, an advisory directory lock also coordinates
separate Conductor processes using that worktree, and Conductor syncs
affected directories between dependent edits and before reporting success.

| Operation | Write order |
| --- | --- |
| Save, rename, add, or delete a request within one group | Replace that group. |
| Move a request between groups | Write the destination with an incremented `moveGeneration`, then remove the source copy. |
| Add a folder | Add its manifest definition, then write the group if it has a real name or contains requests. |
| Give a populated placeholder folder its first name | Copy the group to its named path, update the manifest, then remove the old path. |
| Delete a folder tree | Create recovery links for the affected groups, remove the groups, then remove the folder definitions from the manifest. Restore from the links if a later step fails. |
| Move an existing folder or edit collection or folder auth | Replace the manifest. Group paths and folder IDs stay stable. |
| Create, rename, or delete an environment | Update its shared file. Local entries remain when a file is removed. |
| Duplicate an environment with local overrides or import local environment values | Write local entries under the allocated filename stem, then write the shared file. Until the new environment exists, those local entries have no active scope. |
| Edit or reset linked variables, including a test-script batch | Replace `local.json` once for the repository. |
| Transfer a variable between repositories or into or out of global scope | Write or commit the destination before removing the source. |
| Publish a copy | Validate a staged shared directory and install it with one rename. Then copy private values to the ignored file and link the cache. A retry recognizes the installed manifest ID, fills missing local keys, and finishes linking. |

The first screen reads the SQLite cache. A refresh checks file paths and
metadata before reading contents. Git changes and hand-edits are reflected
on refresh. An unchanged refresh reads no file contents.

If an interrupted move leaves both copies, refresh takes the highest
`moveGeneration`. For ties, a named folder path takes precedence over a
placeholder path, then the lexically first path wins. Refresh reports the
other copies and preserves their bytes so the user can inspect them and
remove the duplicates. Requests with duplicate IDs are blocked from saving
until the duplicate is resolved.

An unreadable manifest keeps the last good cache. An unreadable request group
retains its last good requests, and bad filenames are reported by path while
other files refresh. Unknown auth and header fields report the offending field.
An open draft keeps its original revision. If the request disappeared, recover
the draft before sending or copying cURL with resolved values. The selected
environment ID is remembered when it disappears temporarily on another branch.

The format and file operations live in
[repository commands](../../src-tauri/src/commands/collections/repository/mod.rs).
The [linked variable code](../../src-tauri/src/commands/collections/repository/values.rs)
rebuilds the SQLite variable cache from the shared files and `local.json`.
