// The interface a perf scenario is written against. Each scenario is one file
// in ./scenarios/, named as `pnpm perf:run --scenario <file name>` selects it,
// whose default export is `defineScenario({ ... })`.
//
// Scenario files import only this file and ./fixtures.ts, because
// scripts/perf/harness.ts imports them in Node to list them and write their
// fixtures. Everything that touches the app comes through `ctx`
// (./runtime.ts).
import type { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import type { useWorkspaceUiStore } from "@/features/workspace/workspaceUiStore";
import type { FixtureInfo, FixtureName } from "./fixtures.ts";

/** Milliseconds a step measured itself, recorded as `<step>/<name>`. */
export type StepMetrics = Record<string, number>;

export type ScenarioContext<Fixtures extends FixtureName> = {
  fixture: <Name extends Fixtures>(name: Name) => FixtureInfo<Name>;
  /** The stores, as a click would reach them. */
  workspace: typeof useWorkspaceStore.getState;
  workspaceUi: typeof useWorkspaceUiStore.getState;
  /**
   * Runs `action`, waits for its result to paint, settles, then records a
   * checkpoint: memory, CPU, and wakeups of every process, the Rust heap,
   * the step's time, the longest frame during it, and what `action`
   * returned. Fails the run if a store action reported an error.
   */
  step: (
    label: string,
    action: () => Promise<StepMetrics | void>,
  ) => Promise<void>;
  /** Waits with nothing scheduled, then checkpoints; CPU should be near 0. */
  idle: (label: string) => Promise<void>;
  /** The request in the active collection with this name. */
  requestId: (name: string) => string;
  /** Every request in the active collection, in sidebar order. */
  requestIds: () => string[];
  /** Waits for an element to render, for up to two seconds. */
  find: <E extends Element = HTMLElement>(selector: string) => Promise<E>;
  /**
   * Types `text` at the end of a field, one character every 50 ms, and
   * returns `keystroke-p50`, `keystroke-p95`, and `keystroke-max`: the time
   * from each input event until the frame after it painted.
   */
  type: (
    field: HTMLInputElement | HTMLTextAreaElement,
    text: string,
  ) => Promise<StepMetrics>;
};

export type Scenario<Fixtures extends FixtureName> = {
  /** One line, shown by `node scripts/perf/harness.ts list`. */
  description: string;
  fixtures: readonly Fixtures[];
  /**
   * Runs in a launch of its own, before the measured one, on the same data
   * directory. Its steps are not measured. Use it for state the measured
   * launch should start with.
   */
  prepare?: (ctx: ScenarioContext<Fixtures>) => Promise<void>;
  /** Runs after the `started` checkpoint, which the harness records. */
  run: (ctx: ScenarioContext<Fixtures>) => Promise<void>;
};

export function defineScenario<Fixtures extends FixtureName = never>(
  scenario: Scenario<Fixtures>,
) {
  return scenario;
}
