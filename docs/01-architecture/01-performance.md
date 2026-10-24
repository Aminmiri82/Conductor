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
counts (see [Budgets In CI](#budgets-in-ci)). For startup, typing, memory,
and CPU, compare before and after for anything performance-sensitive (see
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

- For memory, CPU, startup, and step timings, run `pnpm perf:run` (see
  [Perf Harness](#perf-harness)).
- `pnpm build` prints frontend chunk sizes; a jump in the main chunk means
  something is no longer lazy.
- For Rust hot paths (import, resolution, sending), time the operation before
  and after on a large collection.

Report the numbers in the change description, including when a change made
something slower on purpose.

## Perf Harness

The perf harness runs Conductor through a fixed scenario and measures each
step. It is macOS only.

Two things make Conductor hard to measure by hand. First, its memory and CPU
are split across several processes, and the Rust process is usually the
smallest. WebKit runs the webview (React, the stores, CodeMirror, response
bodies on screen) in its own WebContent process, plus a Networking and a GPU
process. Their parent is launchd, not Conductor, so `ps -p <pid>` on the app
misses them. Second, memory should be counted as **footprint**, which is what
Activity Monitor shows as Memory, not RSS: RSS counts shared pages and leaves
out compressed ones.

### `pnpm perf:run`

[`perf-harness.mjs`](../../scripts/perf-harness.mjs) builds a release app
with the `perf-harness` Cargo feature. The build is called Conductor Perf
Harness and has its own identifier and target directory, so it never touches
the installed app. It launches that app through LaunchServices on a throwaway
data directory against the local [echo server](../../scripts/echo-server.mjs).
The webview runs [`perfScenario.ts`](../../src/app/perfScenario.ts): start,
stay idle for 15 seconds, import a 2,000-request collection, open 20 tabs,
receive a 10 MB JSON response, close every tab, then stay idle for 15 seconds
again. After each step, [`perf_harness.rs`](../../src-tauri/src/perf_harness.rs)
records:

- for the app process and each WebKit process macOS holds the app
  responsible for (found by pid, never by name): footprint, CPU time, and
  wakeups;
- the live Rust heap, and its peak since the previous checkpoint, from a
  counting allocator. The peak shows what a step such as the import needed at
  its worst.

The scenario also times each step until its result has painted, and startup
from process start to the first painted frame.

It prints the median of three runs as two tables, one row per step:

- **Memory** per process at the checkpoint, what the step added, and the Rust
  heap. The line under the table is what is still held at the end compared
  with the start, which is where leaks show.
- **CPU** time per process for each step, the step's duration, and wakeups
  per second across all processes. During the idle steps, CPU and wakeups
  should be close to zero (see [Low idle memory](#what-fast-means-here)).

The `±` columns are half the spread across runs. Treat a change smaller than
that as noise. Use `--runs` to change the count, `--skip-build` to reuse the
last build, and `--out <file>` to save the result. To see a change's effect,
save a result before and after it and run
`node scripts/perf-harness.mjs compare before.json after.json`. Nothing here
fails a check yet, because the numbers move between runs.

A step you want to measure belongs in the scenario. Wrap it in `step()`,
which drives it through the stores as a click would, times it, and records a
checkpoint.

Only `perf-harness` builds contain any of this. The scenario's chunk is
dropped from every other build, and the counting allocator and the private
macOS calls never ship.

### Finding Out Why

The harness shows which process grew or worked, and in which step. To find
out what is responsible:

- **Webview memory and CPU.** Run `pnpm tauri dev`, open Web Inspector
  (right-click > Inspect Element), and use the Timelines tab. JavaScript
  Allocations with heap snapshots before and after a step shows what is still
  reachable, such as a response body kept by a closed tab. The JavaScript &
  Events instrument shows where a slow step spends its time. WebKit has no
  `performance.memory`.
- **Rust CPU.** `sample <pid> 10` records stacks for 10 seconds while you
  repeat the slow step. It ships with macOS.
- **Any process's memory.** `footprint -p <pid>` breaks memory down by
  category, and `--sample 1` tracks it over time. `vmmap --summary <pid>` shows
  regions, `heap <pid>` shows malloc objects by class, and `leaks <pid>` finds
  unreachable allocations. These tools ship with macOS. Instruments needs the
  full Xcode.

Use pids from a build you started yourself, as [AGENTS.md](../../AGENTS.md)
says.

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
