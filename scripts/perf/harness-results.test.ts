import { describe, expect, it } from "vitest";
import {
  compareResults,
  comparisonWarnings,
  medians,
  SCHEMA,
  verdict,
  type Metrics,
  type Result,
  type Settings,
} from "./harness-results.ts";

const result = (values: number[]) => {
  const runs = values.map((value): Metrics => ({ "s/import/time": value }));
  return { runs, median: medians(runs) };
};

describe("verdict", () => {
  it("calls a change real when every run on one side beats every run on the other", () => {
    expect(
      verdict([100, 101, 102, 103, 104], [110, 111, 112, 113, 114]),
    ).toMatchObject({
      kind: "worse",
    });
    expect(
      verdict([110, 111, 112, 113, 114], [100, 101, 102, 103, 104]),
    ).toMatchObject({
      kind: "better",
    });
  });

  it("calls overlapping runs noise, however far apart the medians are", () => {
    expect(
      verdict([100, 101, 102, 103, 150], [120, 121, 122, 123, 124]).kind,
    ).toBe("noise");
  });

  it("does not call a change from three runs a side, which separate by chance one time in ten", () => {
    expect(verdict([100, 101, 102], [110, 111, 112]).kind).toBe("noise");
  });
});

describe("medians", () => {
  it("leaves a run that lacks a metric out of its median instead of counting zero", () => {
    expect(medians([{ a: 10 }, { a: 12 }, {}]).a).toBe(11);
  });
});

describe("compareResults", () => {
  it("lists changed metrics before the ones within noise", () => {
    const before = result([100, 101, 102, 103, 104]);
    const after = result([110, 111, 112, 113, 114]);
    before.runs.forEach((run) => (run["s/idle/time"] = 15000));
    after.runs.forEach((run) => (run["s/idle/time"] = 15000));
    before.median = medians(before.runs);
    after.median = medians(after.runs);

    expect(
      compareResults(before, after).map(({ name, verdict }) => [name, verdict]),
    ).toEqual([
      ["s/import/time", "worse"],
      ["s/idle/time", "noise"],
    ]);
  });
});

describe("comparisonWarnings", () => {
  const withSettings = (settings: Partial<Settings>): Result => ({
    ...result([100, 101, 102, 103, 104]),
    schema: SCHEMA,
    meta: {
      app: {
        root: "",
        sha: "",
        dirty: false,
        changesHash: "",
        scenarioHashes: {},
      },
      machine: {
        macOS: "26.0",
        chip: "M4",
        memoryBytes: 1,
        power: "ac",
        lowPower: false,
      },
      settings: {
        settleMs: 3000,
        idleMs: 15000,
        quick: false,
        runs: 5,
        warmup: 1,
        ...settings,
      },
      date: "",
    },
  });

  it("warns when the two sides idled in the background for different lengths", () => {
    expect(
      comparisonWarnings(
        withSettings({ longIdleMs: 60_000 }),
        withSettings({ longIdleMs: 600_000 }),
      ),
    ).toContain("They used different longIdleMs (60000 vs 600000).");
  });
});
