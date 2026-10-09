---
name: writing-tests
description: Add or revise Conductor tests, including regression tests for a bug fix. Not needed just to run existing checks.
---

# Writing Conductor Tests

Every test answers: **if this fails, does someone using Conductor have a
problem?** If it only fails when the code's internal shape changes, do not
write it. Also ask what it catches that `tsc`, clippy, the bindings check, or
an existing test would not. Grow the suite with regression tests for real
bugs, not by back-filling tests that restate the current code.

## Choose The Level

Pick the cheapest one that shows the behavior, and copy the house style of
existing tests at that level.

- **Pure Rust rule** (variable resolution, body/auth building, Postman
  parsing, tree ordering): `#[cfg(test)] mod tests` in the same file, as in
  `commands/variables.rs`. If the logic is tangled with
  `State<AppState>` or HTTP, extract a pure function and test that.
- **Depends on the database** (moves, deletes, duplicates, variable scoping):
  `storage::test_connection()`, an in-memory database with the real schema,
  plus the row builders in `src-tauri/src/fixtures.rs`, as in
  `commands/collections/tree.rs`. Commands are thin wrappers over a function
  taking a connection (`reparent_node`, `copy_request`); test that function.
  If a command still does its work inside the `#[tauri::command]`, split it
  the same way. Never hand-write tables a test needs.
- **Sends** (what goes on the wire, what a response changes): seed a
  `fixtures::TestApp`, start a `test_server::TestServer`, and call
  `sender::send`, as in `commands/requests/sender.rs`. Assert on what the
  server received or what was stored, not on intermediate values.
- **Pure frontend logic**: a colocated `*.test.ts` run by Vitest, as in
  `src/features/requests/urlParams.test.ts`. No jsdom; extract the logic.
- **Only visible in the running app**: not a test. Use the
  [verify-in-app skill](../verify-in-app/SKILL.md). If what you checked there
  can be stated as "this send puts X on the wire" or "this change stores Y",
  also add it as a send or database test so it outlives your run.

## Do Not

- Assert that a function was called, or test private helpers through their
  internals.
- Mock our own code or the database. Only true external boundaries may be
  faked, such as the HTTP target of a send. Lint rejects `vi.mock` of our
  modules and of `@tauri-apps/api` (`tools/oxlint/conductor.mjs`).
- Touch the network. Tests that send start a local server inside the test.
- Write snapshot tests, component render tests, or tests of framework
  behavior (serde renames, Zustand storing values).
- Chase coverage, or pin a known bug as expected behavior.
- Weaken an assertion to get a pass. If the rule changed, change the test to
  state the new rule and say so. If a test blocks a behavior-preserving
  refactor, the test is wrong; flag it for deletion.

A good test's name reads as a spec (`"keeps a disabled query row when the
user edits the URL"`), it is deterministic, and it fails for one reason.

## Completion

For a regression test, show it failing before the fix and passing after.
Run `pnpm check` and report what actually ran. For a nontrivial test, state
in one line each:

- **Promise:** what must stay true.
- **Failure:** what the user would hit if it regressed.
- **Boundary:** why this level is the cheapest that proves it.
