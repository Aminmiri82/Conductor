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

Budgets that fail a check cover what Conductor ships (bundle chunks and
dependency counts) and a few numbers that come out the same on every run, such
as the Rust heap an import needs (see [Budgets In CI](#budgets-in-ci)). For
startup, typing, memory, and CPU, which move between runs, compare before and
after for anything performance-sensitive (see [Measuring](#measuring)).

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

- For memory, CPU, startup, typing latency, and step timings, run
  `pnpm perf:run` (see [Perf Harness](#perf-harness)).
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
the installed app. For each scenario and run, it launches that app through
LaunchServices on a throwaway data directory against the local
[echo server](../../scripts/echo-server.mjs). The webview runs the scenario,
and after each step [`perf_harness.rs`](../../src-tauri/src/perf_harness.rs)
records a checkpoint:

- for the app process and each WebKit process macOS holds the app
  responsible for (found by pid, never by name): footprint, CPU time, and
  wakeups;
- the live Rust heap, and its peak since the previous checkpoint, from a
  counting allocator. The peak shows what a step such as the import needed at
  its worst.

The webview adds the step's time until its result painted, the longest gap
between frames during it (jank you would see), and for typing steps the
latency of each keystroke. Startup is from process start until the first
load of collections has finished and painted. Every launch starts with a
`started` checkpoint; its CPU and wakeups include the launch.

```bash
pnpm perf:run                                  # every scenario, 5 runs each
pnpm perf:run --scenario typing                # only some (repeat, or a,b)
pnpm perf:run --quick                          # 1 run, short waits: checks a scenario works
pnpm perf:list                                 # the scenarios and what they do
pnpm perf:compare before.json after.json
```

A full run of every scenario takes about 15 minutes after the build; use
`--scenario` for the ones a change can affect. Before the measured runs, one
warm-up run is measured and thrown away, because the first launch of a new
build pays for macOS registering it (`--warmup` changes this). It warns when
the Mac is on battery or in Low Power Mode, which slow the CPU.

It prints, per scenario, the median across runs in three tables:

- **Memory** per process at each checkpoint, what the step added, and the
  Rust heap. The line under the table is what is still held at the end
  compared with the start, which is where leaks show.
- **CPU** time per process for each step and wakeups per second across all
  processes. During idle steps, CPU and wakeups should be close to zero (see
  [Low idle memory](#what-fast-means-here)).
- **Responsiveness**: the step's time, its longest frame, and keystroke
  latency at the median, 95th percentile, and worst.

Each result is saved under `src-tauri/target/perf-harness/results/` (or
`--out <file>`) with every run, not just the median, and with what it was
measured on: the commit and whether the tree was dirty, the Mac, the power
state, and the settings. `--skip-build` reuses the last build and warns when
the sources have changed since it was made.

To see a change's effect, save a result before and after it and run
`compare`. It marks a metric as changed only when every run on one side beat
every run on the other, and that ordering is unlikely by chance (p < 0.05,
which takes at least 4 runs a side; the default 5 gives p = 0.008). Everything
else is noise, however far apart the medians look. It also warns when the two
files are not comparable: different Macs or settings, battery power, too few
runs, or a scenario that itself changed between the builds.

### Scenarios And Fixtures

A scenario is one file in
[`src/app/perf/scenarios/`](../../src/app/perf/scenarios/), and its file name
is what `--scenario` selects. It default-exports
`defineScenario({ description, fixtures, prepare?, run })` from
[`scenario.ts`](../../src/app/perf/scenario.ts), which documents everything
`ctx` offers: `step()` and `idle()` record checkpoints, `type()` types into a
field and returns keystroke latency, and `workspace()` reaches the stores as a
click would. A step can return numbers of its own, recorded under its name.
`prepare` runs in an unmeasured launch first, on the same data, for a
scenario that measures starting with data, such as
[`startup-large-collection`](../../src/app/perf/scenarios/startup-large-collection.ts).

Fixtures are the files a scenario needs, such as a Postman collection. Each
is a generator in [`fixtures.ts`](../../src/app/perf/fixtures.ts) that gets
the echo server's URL and returns the file's contents and an `info` object;
the scenario lists the fixture by name and reads `info` and the file's `path`
with `ctx.fixture(name)`. The script generates them in Node before launching,
so building a fixture never counts toward the app's memory.

Scenario files may import only `scenario.ts` and `fixtures.ts`, and both must
stay free of runtime imports. The script imports them in Node, which strips
TypeScript types but cannot resolve `@/` paths, to list scenarios and write
their fixtures. `pnpm check` runs `list`, so a scenario that breaks this rule
fails there with the reason.

Only `perf-harness` builds contain any of this. The scenario chunks are
dropped from every other build, and the counting allocator and the private
macOS calls never ship.

### In CI

The `harness` job in the [Perf workflow](../../.github/workflows/perf.yml)
builds each PR and its base branch, runs the scenarios both have on one
macOS runner alternating between the two builds (base, head, head, base, and
so on, so drift over the job lands on both), and writes the comparison to the
job's summary. The result files are uploaded as an artifact. It only reports:
hosted runners are noisy, so a regression never fails it, but a scenario that
no longer finishes does. A base branch from before scenarios existed is
skipped, and the PR is measured alone.

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

### Budgets In Tests

Some costs come out the same on every run, so ordinary tests can hold them
to a budget, and `cargo test` fails when one grows. Each is a ceiling about 5%
over what was measured, with the measured value in a comment beside it. Raise
one only when the growth is deliberate, and say so in the change.

- The Rust heap peak of importing the harness's 2,000-request collection, and
  the size of the JSON the import sends the sidebar
  ([`import.rs`](../../src-tauri/src/commands/collections/import.rs)).
  `large_postman_collection` in [`fixtures.rs`](../../src-tauri/src/fixtures.rs)
  builds the same collection as the harness's `largeCollection` fixture.
- The size of the JSON the webview parses for a 1 MB JSON response
  ([`sender.rs`](../../src-tauri/src/commands/requests/sender.rs)).

The heap is counted per thread by the counting allocator
([`alloc_counter.rs`](../../src-tauri/src/alloc_counter.rs)), so tests running
in parallel do not count, and SQLite's own allocations never do. A new budget
belongs here when a number is deterministic and a regression in it would cost
users memory or time; timings and footprints are not, and stay in the
harness.
