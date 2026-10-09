import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createAutosaver } from "./autosave";

describe("createAutosaver", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("saves a burst of keystrokes once, after the typing pauses", async () => {
    const saves: number[] = [];
    const autosaver = createAutosaver(
      async () => {
        saves.push(Date.now());
      },
      { delayMs: 400, maxWaitMs: 2000 },
    );

    for (let i = 0; i < 5; i += 1) {
      autosaver.schedule();
      await vi.advanceTimersByTimeAsync(100);
    }
    expect(saves).toHaveLength(0);

    await vi.advanceTimersByTimeAsync(400);
    expect(saves).toHaveLength(1);
  });

  it("still saves during steady typing that never pauses", async () => {
    let saves = 0;
    const autosaver = createAutosaver(
      async () => {
        saves += 1;
      },
      { delayMs: 400, maxWaitMs: 2000 },
    );

    for (let i = 0; i < 25; i += 1) {
      autosaver.schedule();
      await vi.advanceTimersByTimeAsync(100);
    }

    expect(saves).toBe(1);
  });

  it("never overlaps saves, and saves a change made during one right after it", async () => {
    let active = 0;
    let overlapped = false;
    let saves = 0;
    let finishFirst: () => void = () => undefined;
    const autosaver = createAutosaver(
      async () => {
        active += 1;
        overlapped ||= active > 1;
        saves += 1;
        if (saves === 1) {
          await new Promise<void>((resolve) => {
            finishFirst = resolve;
          });
        }
        active -= 1;
      },
      { delayMs: 400, maxWaitMs: 2000 },
    );

    const first = autosaver.flush();
    const second = autosaver.flush();
    expect(saves).toBe(1);

    finishFirst();
    await Promise.all([first, second]);

    expect(saves).toBe(2);
    expect(overlapped).toBe(false);
  });

  it("does not save a cancelled change", async () => {
    let saves = 0;
    const autosaver = createAutosaver(
      async () => {
        saves += 1;
      },
      { delayMs: 400, maxWaitMs: 2000 },
    );

    autosaver.schedule();
    autosaver.cancel();
    await vi.advanceTimersByTimeAsync(5000);

    expect(saves).toBe(0);
  });
});
