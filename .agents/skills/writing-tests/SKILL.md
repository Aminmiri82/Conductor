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
  `commands/requests/variables.rs`. If the logic is tangled with
  `State<AppState>` or HTTP, extract a pure function and test that.
- **Depends on the database** (moves, deletes, duplicates, variable scoping):
  real SQLite in a temp directory via `Database::open(dir)`. None exists yet;
  the first one should move the logic out of the `#[tauri::command]` into a
  function taking `&Database` and add a small temp-dir helper.
- **Pure frontend logic**: a colocated `*.test.ts` run by Vitest, as in
  `src/features/requests/urlParams.test.ts`. No jsdom; extract the logic.
- **Only visible in the running app**: not a test. Use the
  [verify-in-app skill](../verify-in-app/SKILL.md).

## Do Not

- Assert that a function was called, or test private helpers through their
  internals.
- Mock our own code or the database. Only true external boundaries may be
  faked, such as the HTTP target of a send.
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
