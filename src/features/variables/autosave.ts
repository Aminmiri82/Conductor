/**
 * When editor changes reach the database. Typing waits for a short pause
 * (`delayMs`) so a burst of keystrokes is one save, but never longer than
 * `maxWaitMs`, so steady typing still lands. `flush` saves now (a toggle, a
 * delete, leaving the editor). Saves never overlap: a change made while one
 * runs is picked up by one more run right after it.
 *
 * `run` reads the latest state itself and saves whatever differs, so a
 * skipped or merged run loses nothing.
 */
export function createAutosaver(
  run: () => Promise<void>,
  { delayMs, maxWaitMs }: { delayMs: number; maxWaitMs: number },
) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pendingSince: number | undefined;
  let running: Promise<void> | undefined;
  let runAgain = false;

  function cancel() {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
    pendingSince = undefined;
  }

  function flush(): Promise<void> {
    cancel();
    if (running) {
      runAgain = true;
      return running;
    }
    running = (async () => {
      try {
        do {
          runAgain = false;
          await run();
        } while (runAgain);
      } finally {
        running = undefined;
      }
    })();
    return running;
  }

  function schedule() {
    const now = Date.now();
    pendingSince ??= now;
    const wait = Math.min(delayMs, pendingSince + maxWaitMs - now);
    if (timer !== undefined) clearTimeout(timer);
    timer = setTimeout(() => void flush(), Math.max(0, wait));
  }

  return { schedule, flush, cancel };
}

/** Typing saves after this pause, and at least this often. */
export const VARIABLE_AUTOSAVE = { delayMs: 400, maxWaitMs: 2000 };

// Sending, saving and copying a request read variables from the database.
// The variable editors register their `flush` here so those can first wait
// for an edit that is still inside its typing pause.
const flushers = new Set<() => Promise<void>>();

/** Returns the function that takes `flush` out again. */
export function registerVariableAutosave(flush: () => Promise<void>) {
  flushers.add(flush);
  return () => {
    flushers.delete(flush);
  };
}

/** Saves variable edits that are still waiting. Never rejects. */
export async function flushVariableEdits() {
  await Promise.all([...flushers].map((flush) => flush()));
}
