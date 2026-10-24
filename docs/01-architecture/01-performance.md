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

Only what Conductor ships has budgets so far: bundle chunks and dependency
counts (see [Budgets In CI](#budgets-in-ci)). For startup, typing, and memory,
compare before and after for anything performance-sensitive (see
[Measuring](#measuring)).

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

Lint (`conductor/no-ipc-in-loop`) rejects an `api` or `commands` call inside a
loop or an array callback such as `map`.

### SQLite

- Reads use the pooled read-only connections so they never queue behind the
  single writer ([`connection.rs`](../../src-tauri/src/storage/connection.rs)).
- Queries on hot paths (opening a collection or request, sending, search)
  need an index. Check new ones with `EXPLAIN QUERY PLAN`. A scan is fine when
  the table is small by nature, such as listing every collection.
- Multi-row writes happen in one transaction.

### Frontend State And Rendering

- State is split across small Zustand stores so a keystroke only re-renders
  what it touches. Components select narrow slices, never a whole store.
- Large values such as response bodies are not copied through state.
- Heavy UI, such as code editors, loads lazily so it stays out of the main
  chunk.
- Persistence is debounced, never written on every change.

Lint enforces the first and third: `conductor/narrow-store-selector` rejects a
store hook with no selector, or one that returns the whole store or a new
object or array, and `no-restricted-imports` keeps CodeMirror and the updater
plugins out of every file but the lazily loaded one that uses them.

### Dependencies

Every dependency costs binary size, bundle size, or startup time. Prefer the
standard library, the platform, or a few lines of our own code. A new
dependency needs a reason that is stated in the change description.

## Measuring

Dev builds are unoptimized, so measure release builds for memory and startup:

- Build with `pnpm tauri build --bundles app --config '{"bundle":{"macOS":{"signingIdentity":"-"}}}'`,
  launch it with a throwaway `CONDUCTOR_DATA_DIR`, and read its memory
  (`ps -o rss= -p <pid>`) at idle and after opening a large collection.
- `pnpm build` prints frontend chunk sizes; a jump in the main chunk means
  something is no longer lazy.
- For Rust hot paths (import, resolution, sending), time the operation before
  and after on a large collection.

Report the numbers in the change description, including when a change made
something slower on purpose.

## Budgets In CI

The [Perf workflow](../../.github/workflows/perf.yml) builds each PR and its
base branch in the same job, measures both with
[`perf-size.mjs`](../../scripts/perf-size.mjs), and fails when a metric in
[`budgets.json`](../../perf/budgets.json) grows past its budget. Sizes are raw
bytes, because the assets are embedded in the app and parsed from memory, so
gzip size says nothing about startup. Any new crate in `Cargo.lock` or direct
npm dependency fails the check, which enforces [Dependencies](#dependencies).

A deliberate regression lands in one of two ways, both Yara's call: raise the
budget in the PR, with the numbers in its description, or add the
`perf-regression-approved` label for a one-off.

Each [release](../../.github/workflows/release.yml) also records its Mac
binary and DMG sizes as `perf-size.json` on the draft release and compares them
with the last published release. That comparison only reports.

Run it locally with `pnpm perf:size`, which builds the frontend and prints the
measurements.
