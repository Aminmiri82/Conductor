---
name: measuring-performance
description: Measure a change's effect on Conductor's startup, memory, CPU, and typing latency with the perf harness, add a scenario or a test budget, and report the numbers.
disable-model-invocation: true
---

# Measuring Performance

The [Performance page](../../../docs/01-architecture/01-performance.md) is the
reference: what fast means here, the harness, and the budgets. This is the
order to work in.

## 1. Pick The Cheapest Proof

- **A number that is the same on every run** (heap bytes, payload bytes,
  rows, counts): a budget test in Rust. See "Budgets In Tests" on the
  Performance page. It runs in `pnpm check` on every platform.
- **Startup, memory, CPU, frames, keystrokes**: the perf harness. macOS only,
  and it needs a release build (several minutes the first time).
- Bundle size and dependencies are checked in CI already (`pnpm perf:size`).

## 2. Run The Harness

```bash
pnpm perf:list                                   # what each scenario measures
pnpm perf:run --scenario <name> --quick          # does it still finish? (~1 min after the build)
pnpm perf:run --scenario <name> --out /tmp/before.json
# make the change
pnpm perf:run --scenario <name> --out /tmp/after.json
pnpm perf:compare /tmp/before.json /tmp/after.json
```

- Run only the scenarios your change can affect; each takes a few minutes
  at the default 5 runs.
- Measure "before" on the commit without your change, from a clean tree,
  then "after" on the same Mac, plugged in. Do not trust `--quick` numbers;
  it is a smoke test.
- The harness never touches the real app or its data, and only stops the
  pid it launched. Never run it against a real environment.

## 3. Read The Comparison

`compare` calls a metric `worse` or `better` only when the runs separate
(p < 0.05). Everything else is `noise`; do not report a noise row as an
improvement or a regression. Read its warnings first: battery power, fewer
than 5 runs a side, different Macs, or a scenario that changed between the
two builds all make the numbers suspect.

To find out *why* a number moved, see "Finding Out Why" on the Performance
page.

## 4. Add A Scenario When Nothing Measures Your Path

Add a file to `src/app/perf/scenarios/` (copy the closest one) and, if it
needs a new file to import, a generator in `src/app/perf/fixtures.ts`.
Scenario files import only `../scenario.ts` and `../fixtures.ts`; everything
else comes through `ctx`. Wrap each measured action in `ctx.step()`, and use
`ctx.idle()` for idle windows. Then:

1. `pnpm perf:list` imports it and fails with the reason if a rule is broken.
2. `pnpm perf:run --scenario <name> --quick` must finish.
3. If you rename a step, say so: the CI comparison shows its old metrics as
   removed and the new ones as new.

## 5. Report

Put the comparison in the change description, the rows that changed and the
warnings, and say which scenarios you ran and how many runs. Report a
regression even when it is deliberate; it needs Yara's sign-off (see
[AGENTS.md](../../../AGENTS.md)). If you could not run the harness (not on a
Mac, the build failed), say so plainly rather than estimating. In CI, the
Perf workflow's `harness` job posts the same table in its summary, but it
only reports.
