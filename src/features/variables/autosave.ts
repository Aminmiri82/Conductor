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
