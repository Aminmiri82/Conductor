// What the perf harness does with the numbers: turns an app run's output into
// flat metrics, summarizes runs, and compares two result files. Pure, so it
// is unit tested (harness-results.test.ts). See scripts/perf/harness.ts.

/** Bumped when the result file changes shape; `compare` refuses others. */
export const SCHEMA = 1;

export const ROLES = ["app", "WebContent", "Networking", "GPU", "other"];

/** Metric name to value; a run may lack a metric another run has. */
export type Metrics = Record<string, number | undefined>;

/** A checkpoint line of the app's output; see `Checkpoint` in perf_harness.rs. */
export type Checkpoint = {
  label: string;
  stepMs: number;
  atMs: number;
  rustHeap: number;
  rustHeapPeak: number;
  processes: {
    pid: number;
    name: string;
    footprint: number;
    cpuNs: number;
    wakeups: number;
  }[];
  metrics: Record<string, number>;
};

/** What a build was made from, written next to it as `<app>.build.json`. */
export type Stamp = {
  root: string;
  sha: string;
  dirty: boolean;
  changesHash: string;
  scenarioHashes: Record<string, string>;
  builtAt?: string;
};

export type Machine = {
  macOS: string;
  chip: string;
  memoryBytes: number;
  power: "battery" | "ac";
  lowPower: boolean;
};

export type Settings = {
  settleMs: number;
  idleMs: number;
  longIdleMs?: number;
  quick: boolean;
  runs: number;
  warmup: number;
};

/** A result file. */
export type Result = {
  schema: number;
  meta: { app: Stamp; machine: Machine; settings: Settings; date: string };
  runs: Metrics[];
  median: Metrics;
};

type Verdict = "worse" | "better" | "new" | "removed" | "noise";

type Row = {
  name: string;
  before: number | undefined;
  after: number | undefined;
  change: number | undefined;
  percent: number | undefined;
  verdict: Verdict;
  p: number;
};

// Two-sided p-value below which `compare` calls a change real.
const SIGNIFICANCE = 0.05;

// The app process is listed first. WebKit may run several WebContent
// processes, which are summed; macOS helpers it attributes to the app go
// under `other`.
function roleOf(name: string, index: number) {
  if (index === 0) return "app";
  return ROLES.find((role) => name.endsWith(`.${role}`)) ?? "other";
}

// Flat metric names, so runs can be combined and compared name by name:
//   <scenario>/startup                      ms from process start until the
//                                           first load has painted
//   <scenario>/<step>/time                  ms until the step's result painted
//   <scenario>/<step>/memory/<role|total>   footprint bytes at the checkpoint
//   <scenario>/<step>/rust-heap(-peak)      bytes
//   <scenario>/<step>/cpu/<role|total>      ms of CPU since the previous
//                                           checkpoint
//   <scenario>/<step>/wakeups-per-second    all processes, since the previous
//                                           checkpoint
//   <scenario>/<step>/<name>                what the step measured itself,
//                                           such as longest-frame or
//                                           keystroke-p95 (ms)
export function toMetrics(
  scenario: string,
  {
    startupMs,
    checkpoints,
  }: { startupMs: number | undefined; checkpoints: Checkpoint[] },
) {
  const metrics: Metrics = { [`${scenario}/startup`]: startupMs };
  const previous = new Map<number, { cpuNs: number; wakeups: number }>();
  let previousAt: number | undefined;
  for (const checkpoint of checkpoints) {
    const { label, stepMs, atMs, rustHeap, rustHeapPeak, processes } =
      checkpoint;
    const prefix = `${scenario}/${label}`;
    const add = (name: string, value: number) =>
      (metrics[`${prefix}/${name}`] =
        (metrics[`${prefix}/${name}`] ?? 0) + value);
    metrics[`${prefix}/time`] = stepMs;
    let wakeups = 0;
    processes.forEach(
      ({ pid, name, footprint, cpuNs, wakeups: total }, index) => {
        const role = roleOf(name, index);
        // Per pid, so a process that exits between checkpoints is not a
        // negative delta.
        const before = previous.get(pid) ?? { cpuNs: 0, wakeups: 0 };
        add(`memory/${role}`, footprint);
        add("memory/total", footprint);
        add(`cpu/${role}`, (cpuNs - before.cpuNs) / 1e6);
        add("cpu/total", (cpuNs - before.cpuNs) / 1e6);
        wakeups += total - before.wakeups;
        previous.set(pid, { cpuNs, wakeups: total });
      },
    );
    // The counters start with the process, so the first checkpoint
    // (`started`) includes the launch.
    metrics[`${prefix}/wakeups-per-second`] =
      wakeups / ((atMs - (previousAt ?? 0)) / 1000);
    metrics[`${prefix}/rust-heap`] = rustHeap;
    metrics[`${prefix}/rust-heap-peak`] = rustHeapPeak;
    for (const [name, value] of Object.entries(checkpoint.metrics))
      metrics[`${prefix}/${name}`] = value;
    previousAt = atMs;
  }
  return metrics;
}

/** Every value a metric had across runs; runs that lack it are left out. */
export function valuesOf(runs: Metrics[], name: string) {
  return runs.map((run) => run[name]).filter((value) => value !== undefined);
}

function median(values: number[]) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2
    ? sorted[middle]
    : (sorted[middle - 1] + sorted[middle]) / 2;
}

export function medians(runs: Metrics[]) {
  const names = [...new Set(runs.flatMap(Object.keys))];
  return Object.fromEntries(
    names.map((name) => [name, median(valuesOf(runs, name))]),
  );
}

/** Half the spread across runs: a change smaller than this is noise. */
export function spread(values: number[]) {
  return (Math.max(...values) - Math.min(...values)) / 2;
}

function binomial(n: number, k: number) {
  let result = 1;
  for (let i = 1; i <= k; i++) result = (result * (n - k + i)) / i;
  return result;
}

/**
 * Whether two sets of runs differ beyond noise: every run on one side is
 * below every run on the other (a Mann-Whitney U of 0), and that is unlikely
 * by chance. The p-value of that ordering is 2 / C(n + m, n), so 3 runs a side
 * can never pass (0.1) and 5 a side can (0.008).
 */
export function verdict(
  before: number[],
  after: number[],
): { kind: "worse" | "better" | "noise"; p: number } {
  const p = 2 / binomial(before.length + after.length, before.length);
  const separated =
    Math.max(...before) < Math.min(...after) ||
    Math.max(...after) < Math.min(...before);
  if (!separated || p >= SIGNIFICANCE) return { kind: "noise", p };
  // Every metric is a cost, so lower is better.
  return { kind: median(after) > median(before) ? "worse" : "better", p };
}

/** Rows for every metric in either file, changed ones first. */
export function compareResults(
  before: Pick<Result, "runs" | "median">,
  after: Pick<Result, "runs" | "median">,
) {
  const names = [
    ...new Set([...Object.keys(before.median), ...Object.keys(after.median)]),
  ];
  const rows = names.map((name): Row => {
    const [a, b] = [before.median[name], after.median[name]];
    const result = verdict(
      valuesOf(before.runs, name),
      valuesOf(after.runs, name),
    );
    return {
      name,
      before: a,
      after: b,
      change: a === undefined || b === undefined ? undefined : b - a,
      percent: a && b !== undefined ? ((b - a) / a) * 100 : undefined,
      verdict:
        a === undefined ? "new" : b === undefined ? "removed" : result.kind,
      p: result.p,
    };
  });
  const rank: Record<Verdict, number> = {
    worse: 0,
    better: 1,
    new: 2,
    removed: 3,
    noise: 4,
  };
  return rows.sort((x, y) => rank[x.verdict] - rank[y.verdict]);
}

/** Why two result files may not be comparable, as sentences. */
export function comparisonWarnings(before: Result, after: Result) {
  const warnings: string[] = [];
  const machine = (result: Result) => result.meta.machine;
  for (const key of ["macOS", "chip", "memoryBytes"] as const)
    if (machine(before)[key] !== machine(after)[key])
      warnings.push(
        `They ran on different machines (${key}: ${machine(before)[key]} vs ${machine(after)[key]}).`,
      );
  for (const key of ["settleMs", "idleMs", "longIdleMs"] as const)
    if (
      key in before.meta.settings &&
      key in after.meta.settings &&
      before.meta.settings[key] !== after.meta.settings[key]
    )
      warnings.push(
        `They used different ${key} (${before.meta.settings[key]} vs ${after.meta.settings[key]}).`,
      );
  for (const [side, result] of [
    ["before", before],
    ["after", after],
  ] as const) {
    if (machine(result).power === "battery")
      warnings.push(`The ${side} run was on battery power.`);
    if (machine(result).lowPower)
      warnings.push(`The ${side} run was in Low Power Mode.`);
    if (result.runs.length < 5)
      warnings.push(
        `The ${side} file has ${result.runs.length} run(s); with fewer than 5 a side, no change can be told apart from noise.`,
      );
  }
  const hashes = (result: Result) => result.meta.app.scenarioHashes;
  for (const [scenario, hash] of Object.entries(hashes(after))) {
    const old = hashes(before)[scenario];
    if (old && old !== hash)
      warnings.push(
        `Scenario ${scenario} or its fixtures changed between the two builds, so part of its change is the scenario's.`,
      );
  }
  return warnings;
}

function formatterFor(name: string): (value: number) => string {
  if (name.includes("/memory/") || name.includes("rust-heap"))
    return (bytes) => `${mb(bytes)} MB`;
  if (name.endsWith("wakeups-per-second")) return (rate) => rate.toFixed(1);
  return ms;
}

export function mb(bytes: number) {
  return (bytes / 1024 / 1024).toFixed(1);
}

export function ms(value: number) {
  return `${Number(value).toFixed(0)} ms`;
}

// Adds a + to a positive number or a formatted one such as "12 ms".
export function signed(value: number | string, digits?: number) {
  const text =
    typeof value === "number" && digits !== undefined
      ? value.toFixed(digits)
      : String(value);
  return parseFloat(text) > 0 ? `+${text}` : text;
}

function cells(row: Row) {
  const format = formatterFor(row.name);
  return [
    row.name,
    row.before === undefined ? "" : format(row.before),
    row.after === undefined ? "" : format(row.after),
    row.change === undefined
      ? ""
      : `${signed(format(row.change))}${row.percent === undefined ? "" : ` (${signed(row.percent, 1)}%)`}`,
    row.verdict === "worse" || row.verdict === "better"
      ? `${row.verdict} (p=${row.p.toFixed(3)})`
      : row.verdict,
  ];
}

const HEADERS = ["metric", "before", "after", "change", "verdict"];

export function textTable(headers: string[], rows: string[][]) {
  const widths = headers.map((header, column) =>
    Math.max(header.length, ...rows.map((row) => String(row[column]).length)),
  );
  const line = (cells: string[]) =>
    cells
      .map((cell, column) =>
        column
          ? String(cell).padStart(widths[column])
          : String(cell).padEnd(widths[column]),
      )
      .join("  ");
  return [line(headers), ...rows.map(line)].join("\n");
}

export function comparisonText(before: Result, after: Result) {
  const rows = compareResults(before, after);
  const changed = rows.filter(
    ({ verdict }) => verdict === "worse" || verdict === "better",
  );
  return [
    ...comparisonWarnings(before, after).map(
      (warning) => `warning: ${warning}`,
    ),
    textTable(HEADERS, rows.map(cells)),
    "",
    `${changed.length} metric(s) changed beyond run-to-run noise.`,
  ].join("\n");
}

function markdownTable(rows: Row[]) {
  return [
    `| ${HEADERS.join(" | ")} |`,
    `| ${HEADERS.map((_, index) => (index ? "--:" : ":--")).join(" | ")} |`,
    ...rows.map((row) => `| ${cells(row).join(" | ")} |`),
  ].join("\n");
}

/** For a PR or the CI step summary: changed metrics, then all of them. */
export function comparisonMarkdown(before: Result, after: Result) {
  const rows = compareResults(before, after);
  const changed = rows.filter(
    ({ verdict }) => verdict === "worse" || verdict === "better",
  );
  const warnings = comparisonWarnings(before, after);
  return [
    "### Perf harness",
    "",
    `${before.runs.length} run(s) of \`${before.meta.app.sha.slice(0, 7)}\` against ${after.runs.length} of \`${after.meta.app.sha.slice(0, 7)}\`. A metric counts as changed only when every run on one side beat every run on the other (p < ${SIGNIFICANCE}). Report only; nothing here fails the check.`,
    "",
    ...warnings.map((warning) => `> ⚠️ ${warning}`),
    ...(warnings.length ? [""] : []),
    changed.length
      ? markdownTable(changed)
      : "No metric moved beyond run-to-run noise.",
    "",
    "<details><summary>All metrics</summary>",
    "",
    markdownTable(rows),
    "",
    "</details>",
    "",
  ].join("\n");
}
