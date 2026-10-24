// Runs one perf scenario in the webview, in perf harness builds only (see
// main.tsx). The harness passes the scenario and its fixtures through Rust's
// `perf_harness` module, which records each checkpoint.
import { emit, once } from "@tauri-apps/api/event";
import { requestIdsForNode } from "@/features/collections/tree";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";
import { useWorkspaceUiStore } from "@/features/workspace/workspaceUiStore";
import type { CollectionNode } from "@/features/types";
import type { FixtureName } from "./fixtures.ts";
import type { Scenario, ScenarioContext, StepMetrics } from "./scenario.ts";

/** Written by `measureScenario` in scripts/perf/harness.ts. */
type Config = {
  scenario: string;
  phase: "prepare" | "measure";
  /** After each step: long enough for WebKit to finish layout and return
   * freed pages. JS garbage collection cannot be forced from a page. */
  settleMs: number;
  idleMs: number;
  fixtures: Record<string, unknown>;
};

// Names the harness itself records under each step.
const RESERVED_METRICS = new Set([
  "time",
  "rust-heap",
  "rust-heap-peak",
  "wakeups-per-second",
]);
const KEYSTROKE_INTERVAL_MS = 50;
const FIND_TIMEOUT_MS = 2000;

const scenarios = import.meta.glob<{ default: Scenario<FixtureName> }>(
  "./scenarios/*.ts",
);

// Plain string events, not typed tauri-specta ones, on purpose: typed events
// are registered in `specta_builder()`, which generates src/bindings.ts, and
// the harness only exists in `perf-harness` builds, so the bindings would
// depend on the build. See src-tauri/src/perf_harness.rs.
async function request<T>(event: string, reply: string, payload?: unknown) {
  let resolve: (value: T) => void = () => {};
  const answer = new Promise<T>((done) => (resolve = done));
  await once<T>(reply, (reply) => resolve(reply.payload));
  await emit(event, payload);
  return answer;
}

const nextFrame = () => new Promise(requestAnimationFrame);

const wait = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms));

/** Two frames: the first runs before the update paints, the second after. */
async function painted() {
  await nextFrame();
  await nextFrame();
}

/**
 * Resolves just after the next paint: a message posted from a frame callback
 * runs once that frame is done, without setTimeout's clamping.
 */
function afterNextPaint() {
  return new Promise<void>((resolve) =>
    requestAnimationFrame(() => {
      const channel = new MessageChannel();
      channel.port1.onmessage = () => resolve();
      channel.port2.postMessage(null);
    }),
  );
}

/** The longest gap between frames until `stop`, which is jank on screen. */
function trackFrames() {
  let last = performance.now();
  let longest = 0;
  let running = true;
  const frame = (now: number) => {
    longest = Math.max(longest, now - last);
    last = now;
    if (running) requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
  return () => {
    running = false;
    return Math.max(longest, performance.now() - last);
  };
}

function percentile(sorted: number[], fraction: number) {
  return sorted[Math.floor(sorted.length * fraction)];
}

// Store actions report failure in `error` instead of throwing.
function throwStoreError(step: string) {
  const { error } = useWorkspaceStore.getState();
  if (error) throw new Error(`${step}: ${error}`);
}

/** Startup ends when the first load has finished and its result painted. */
async function loaded() {
  if (!useWorkspaceStore.getState().collectionsLoaded) {
    await new Promise<void>((resolve) => {
      const unsubscribe = useWorkspaceStore.subscribe((state) => {
        if (!state.collectionsLoaded) return;
        unsubscribe();
        resolve();
      });
    });
  }
  throwStoreError("startup");
  await painted();
}

function findRequest(
  nodes: CollectionNode[],
  name: string,
): string | undefined {
  for (const node of nodes) {
    if (node.kind === "request" && node.name === name) return node.requestId;
    if (node.kind === "folder") {
      const found = findRequest(node.children, name);
      if (found) return found;
    }
  }
}

function context(config: Config): ScenarioContext<FixtureName> {
  const measuring = config.phase === "measure";
  const labels = new Set<string>();

  async function checkpoint(
    label: string,
    stepMs: number,
    metrics: StepMetrics,
  ) {
    if (labels.has(label) || label.includes("/"))
      throw new Error(`step label "${label}" is repeated or has a "/"`);
    labels.add(label);
    for (const name of Object.keys(metrics))
      if (RESERVED_METRICS.has(name) || name.includes("/"))
        throw new Error(`step "${label}" returned a reserved metric "${name}"`);
    await request("perf:checkpoint", "perf:recorded", {
      label,
      stepMs,
      metrics,
    });
  }

  return {
    // The script wrote one entry per fixture the scenario lists, which is
    // all `name` can be.
    fixture: (name) =>
      config.fixtures[name] as ReturnType<
        ScenarioContext<FixtureName>["fixture"]
      >,
    workspace: useWorkspaceStore.getState,
    workspaceUi: useWorkspaceUiStore.getState,

    step: async (label, action) => {
      if (!measuring) {
        await action();
        throwStoreError(label);
        return;
      }
      const stopFrames = trackFrames();
      const started = performance.now();
      const metrics = (await action()) ?? {};
      throwStoreError(label);
      await painted();
      const stepMs = performance.now() - started;
      metrics["longest-frame"] = stopFrames();
      await wait(config.settleMs);
      await checkpoint(label, stepMs, metrics);
    },

    // No frame tracking here: its callbacks would be the CPU being measured.
    idle: async (label) => {
      const started = performance.now();
      await wait(config.idleMs);
      if (measuring) await checkpoint(label, performance.now() - started, {});
    },

    requestId: (name) => {
      const id = findRequest(useWorkspaceStore.getState().tree, name);
      if (!id) throw new Error(`no request named "${name}"`);
      return id;
    },
    requestIds: () =>
      useWorkspaceStore.getState().tree.flatMap(requestIdsForNode),

    find: async <E extends Element>(selector: string) => {
      const deadline = performance.now() + FIND_TIMEOUT_MS;
      for (;;) {
        const element = document.querySelector<E>(selector);
        if (element) return element;
        if (performance.now() > deadline)
          throw new Error(`nothing matches ${selector}`);
        await nextFrame();
      }
    },

    // execCommand goes through WebKit's editing path and fires the same input
    // events a key press does, so React's onChange runs as it would for a
    // user. What it skips is the native key event before that.
    type: async (field, text) => {
      field.focus();
      field.setSelectionRange(field.value.length, field.value.length);
      const expected = field.value + text;
      const latencies: number[] = [];
      for (const character of text) {
        const started = performance.now();
        if (!document.execCommand("insertText", false, character))
          throw new Error("the field did not accept typed text");
        await afterNextPaint();
        const latency = performance.now() - started;
        latencies.push(latency);
        await wait(Math.max(0, KEYSTROKE_INTERVAL_MS - latency));
      }
      // React resets a controlled field whose change did not reach the store.
      if (field.value !== expected)
        throw new Error("the typed text did not stay in the field");
      latencies.sort((a, b) => a - b);
      return {
        "keystroke-p50": percentile(latencies, 0.5),
        "keystroke-p95": percentile(latencies, 0.95),
        "keystroke-max": latencies[latencies.length - 1],
      };
    },
  };
}

async function runScenario() {
  const config = await request<Config>("perf:hello", "perf:config");
  const load = scenarios[`./scenarios/${config.scenario}.ts`];
  if (!load) throw new Error(`no scenario named ${config.scenario}`);
  await loaded();
  const ctx = context(config);
  if (config.phase === "prepare") {
    const { default: scenario } = await load();
    await scenario.prepare?.(ctx);
    return;
  }
  await emit("perf:ready");
  // Loaded after startup is recorded, so it does not count toward it.
  const { default: scenario } = await load();
  await ctx.step("started", async () => {});
  await scenario.run(ctx);
}

export async function runPerfScenario() {
  try {
    await runScenario();
    await emit("perf:done");
  } catch (error) {
    await emit("perf:failed", String(error));
  }
}
