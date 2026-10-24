// The steps `pnpm perf:run` measures (scripts/perf-harness.mjs). Loaded only
// in a perf harness build; Rust's `perf_harness` module records each
// checkpoint. Steps go through the stores, as a click would, so React renders
// what a user would see.
import { emit, once } from "@tauri-apps/api/event";
import { requestIdsForNode } from "@/features/collections/tree";
import { useWorkspaceStore } from "@/features/workspace/workspaceStore";

const TABS = 20;
// Long enough for WebKit to finish layout and return freed pages; JS garbage
// collection cannot be forced from a page, so this is the best available.
const SETTLE_MS = 3000;
// Idle windows, where CPU time and wakeups should be close to zero.
const IDLE_MS = 15_000;

async function request<T>(event: string, reply: string, payload?: unknown) {
  let resolve: (value: T) => void = () => {};
  const answer = new Promise<T>((done) => (resolve = done));
  await once<T>(reply, (reply) => resolve(reply.payload));
  await emit(event, payload);
  return answer;
}

function nextFrame() {
  return new Promise(requestAnimationFrame);
}

const wait = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms));

// Store actions report failure in `error` instead of throwing.
function throwStoreError(step: string) {
  const { error } = useWorkspaceStore.getState();
  if (error) throw new Error(`${step}: ${error}`);
}

/** Times `action` until its result has painted, settles, then checkpoints. */
async function step(label: string, action: () => Promise<void>) {
  const started = performance.now();
  await action();
  throwStoreError(label);
  await nextFrame();
  await nextFrame();
  const stepMs = performance.now() - started;
  await wait(SETTLE_MS);
  await request("perf:checkpoint", "perf:recorded", { label, stepMs });
}

async function scenario() {
  const { collectionPath } = await request<{ collectionPath: string | null }>(
    "perf:hello",
    "perf:config",
  );
  if (!collectionPath) throw new Error("no collection to import");
  const store = useWorkspaceStore.getState;

  await nextFrame();
  await nextFrame();
  await emit("perf:ready");
  await step("started", async () => {});
  await step("idle", () => wait(IDLE_MS));

  await step("import", () => store().importCollection(collectionPath));

  // The generator puts the large response request first, then folders.
  const [largeResponse, ...folders] = store().tree;
  const requestIds = folders.flatMap(requestIdsForNode).slice(0, TABS);
  await step(`open-${TABS}-tabs`, async () => {
    // One at a time, as a user opens tabs.
    for (const requestId of requestIds) {
      await store().selectRequest(requestId);
      throwStoreError("open tab");
    }
  });

  if (largeResponse?.kind !== "request")
    throw new Error("the first node is not the large response request");
  await store().selectRequest(largeResponse.requestId);
  await step("large-response", () => store().sendActiveRequest());

  await step("close-tabs", async () => {
    for (const { requestId } of store().tabs) {
      await store().closeRequestTab(requestId);
    }
  });

  await step("idle-after", () => wait(IDLE_MS));
}

export async function runPerfScenario() {
  try {
    await scenario();
    await emit("perf:done");
  } catch (error) {
    await emit("perf:failed", String(error));
  }
}
