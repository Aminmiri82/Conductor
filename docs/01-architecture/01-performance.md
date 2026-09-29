---
title: Performance
description: Why speed and low memory outrank features in Conductor, and the rules that keep it that way.
---

Conductor exists because Postman got slow and heavy. Being fast and light is
the product, not a nice-to-have. A change that makes the app start slower, type
slower, or hold more memory is a regression even when every test passes, and it
needs Yara's sign-off before it lands.

## What "Fast" Means Here

- **Instant interaction.** Typing in the URL bar, switching tabs, and opening
  a request must never wait on the backend or re-render the whole app.
- **Fast cold start.** Nothing loads at startup that the first screen does not
  need.
- **Low idle memory.** An idle Conductor should be close to an empty webview
  plus the Rust process. No background polling, no timers running while idle.
- **Large collections stay smooth.** Hundreds of requests and multi-megabyte
  responses are normal, not edge cases.

We do not have measured budgets yet. Until we do, compare before and after for
anything performance-sensitive (see [Measuring](#measuring)).

## Rules

### Work Belongs In Rust

Parsing, Postman import, variable resolution, HTTP, and storage live in Rust.
The frontend renders and edits. If something loops over a whole collection or
a whole response body in TypeScript, it probably belongs behind a command.

### Keep IPC Small And Few

Every command call serializes JSON across the webview boundary.

- Return the smallest shape the screen needs. The collection tree returns
  nodes, not full request details; a request's detail loads when opened.
- Do not call commands in a loop. Add a command that does the batch in Rust.
- Do not round-trip data the frontend already has just to reformat it.

### SQLite

- The database runs in WAL mode with one write connection and a small pool of
  read-only connections ([`connection.rs`](../../src-tauri/src/storage/connection.rs)).
  Reads go through `with_read_connection` so they never queue behind writes;
  writes go through `with_connection`.
- Every query path needs an index; check `schema/001_initial.sql` when adding
  a query.
- Multi-row writes happen in one transaction.

### Frontend State And Rendering

- State is split across small Zustand stores (`workspaceStore`, `draftStore`,
  `responseStore`, `workspaceUiStore`) so a keystroke only re-renders what it
  touches. Components select narrow slices, never a whole store.
- Large values are not copied through state. `responseStore` keeps responses
  in a `Map` and bumps a revision counter instead of cloning bodies.
- Heavy editors load lazily. The CodeMirror-based `ResponseViewer` is a
  `React.lazy` chunk; keep new heavy UI behind the same pattern.
- Persistence is debounced. Workspace UI state flushes after a short delay,
  not on every change.

### Dependencies

Every dependency costs binary size, bundle size, or startup time. Prefer the
standard library, the platform, or a few lines of our own code. A new
dependency needs a reason that is stated in the change description.

## Measuring

Dev builds are unoptimized, so measure release builds for memory and startup:

- `pnpm build:mac`, launch the app, and read its memory in Activity Monitor
  (or `ps -o rss= -p <pid>`) at idle and after opening a large collection.
- `pnpm build` prints frontend chunk sizes; a jump in the main chunk means
  something is no longer lazy.
- For Rust hot paths (import, resolution, sending), time the operation before
  and after on a large collection.

Report the numbers in the change description, including when a change made
something slower on purpose.
