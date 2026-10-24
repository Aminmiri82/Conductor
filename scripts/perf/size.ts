// Measures what Conductor ships (bundle chunks, dependency counts, and
// optionally the binary and DMG) and compares two measurements against
// perf/budgets.json. See docs/01-architecture/01-performance.md#measuring.
//
//   node scripts/perf/size.ts measure [--dist dist] [--binary path] [--dmg path] [--out file]
//   node scripts/perf/size.ts compare <base.json> <head.json> [--budgets perf/budgets.json]
//
// `compare` exits 1 when a budgeted metric grows past its budget, unless
// PERF_OVERRIDE=1 (set by CI when Yara adds the perf-regression-approved label).
import {
  appendFileSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { parseArgs } from "node:util";

type Sizes = Record<string, number>;
type Budget = { max?: number; maxGrowth?: number; maxGrowthPercent?: number };

const [command, ...rest] = process.argv.slice(2);
const { values: options, positionals } = parseArgs({
  args: rest,
  allowPositionals: true,
  options: {
    dist: { type: "string", default: "dist" },
    binary: { type: "string" },
    dmg: { type: "string" },
    out: { type: "string" },
    budgets: { type: "string", default: "perf/budgets.json" },
  },
});

if (command === "measure") measure();
else if (command === "compare") compare(positionals[0], positionals[1]);
else {
  console.error("usage: size.ts measure|compare (see the header comment)");
  process.exit(2);
}

function measure() {
  // Assets are embedded in the app and read from memory, never downloaded, so
  // raw bytes (what WebKit parses) matter, not gzip.
  const metrics: Sizes = {};
  const add = (name: string, bytes: number) =>
    (metrics[name] = (metrics[name] ?? 0) + bytes);
  const assets = join(options.dist, "assets");
  const html = readFileSync(join(options.dist, "index.html"), "utf8");
  // `js/entry` is everything that loads at startup: the entry script plus the
  // chunks index.html modulepreloads. Rolldown splits shared code out of the
  // entry, so counting the entry script alone would let startup code escape
  // the budget.
  const startup = new Set(
    [
      ...html.matchAll(
        /<(?:script[^>]+src|link[^>]+rel="modulepreload"[^>]+href)="\/assets\/([^"]+\.js)"/g,
      ),
    ].map((match) => match[1]),
  );
  if (!startup.size)
    throw new Error(`no entry script in ${options.dist}/index.html`);

  for (const file of readdirSync(assets)) {
    const bytes = statSync(join(assets, file)).size;
    // Vite names chunks `<name>-<8 char hash>.<ext>`; drop the hash so a chunk
    // keeps its name across builds. Same-named chunks are summed.
    const name = file.replace(/-[\w-]{8}(?=\.\w+$)/, "");
    if (file.endsWith(".js")) {
      add("js/total", bytes);
      add(
        startup.has(file) ? "js/entry" : `js/lazy/${name.replace(/\.js$/, "")}`,
        bytes,
      );
    } else if (file.endsWith(".css")) add("css/total", bytes);
    else add("other/total", bytes);
  }

  const lock = readFileSync("src-tauri/Cargo.lock", "utf8");
  metrics["deps/cargo-crates"] = lock.match(/^\[\[package\]\]$/gm)!.length;
  const manifest: { dependencies?: Record<string, string> } = JSON.parse(
    readFileSync("package.json", "utf8"),
  );
  metrics["deps/npm-direct"] = Object.keys(manifest.dependencies ?? {}).length;

  if (options.binary) metrics["app/binary"] = statSync(options.binary).size;
  if (options.dmg) metrics["app/dmg"] = statSync(options.dmg).size;

  const json = `${JSON.stringify(metrics, null, 2)}\n`;
  if (options.out) writeFileSync(options.out, json);
  else process.stdout.write(json);
}

function compare(basePath: string, headPath: string) {
  if (!basePath || !headPath)
    throw new Error("compare needs <base.json> <head.json>");
  const read = <T>(path: string): T => JSON.parse(readFileSync(path, "utf8"));
  const base = read<Sizes>(basePath);
  const head = read<Sizes>(headPath);
  const budgets = read<{ metrics: Record<string, Budget> }>(
    options.budgets,
  ).metrics;
  const override = process.env.PERF_OVERRIDE === "1";

  const failures: string[] = [];
  const rows: string[] = [];
  const names = [
    ...new Set([
      ...Object.keys(budgets),
      ...Object.keys(base),
      ...Object.keys(head),
    ]),
  ];
  for (const name of names.sort()) {
    const before = base[name];
    const after = head[name];
    if (before === undefined && after === undefined) continue;
    const budget = budgets[name];
    const problems: string[] = [];
    if (budget && after !== undefined) {
      if (budget.max !== undefined && after > budget.max)
        problems.push(`over the ${format(name, budget.max)} cap`);
      if (
        budget.maxGrowth !== undefined &&
        before !== undefined &&
        after - before > budget.maxGrowth
      )
        problems.push(`grew more than ${format(name, budget.maxGrowth)}`);
      if (
        budget.maxGrowthPercent !== undefined &&
        before !== undefined &&
        before > 0 &&
        ((after - before) / before) * 100 > budget.maxGrowthPercent
      )
        problems.push(`grew more than ${budget.maxGrowthPercent}%`);
    }
    if (problems.length) failures.push(`${name}: ${problems.join(", ")}`);
    const delta =
      before !== undefined && after !== undefined ? after - before : undefined;
    const percent =
      delta !== undefined && before > 0
        ? ` (${signed((delta / before) * 100, 1)}%)`
        : "";
    const status = problems.length
      ? override
        ? "⚠️ approved"
        : "❌"
      : budget
        ? "✅"
        : "report only";
    rows.push(
      `| ${name} | ${before === undefined ? "new" : format(name, before)} | ${
        after === undefined ? "removed" : format(name, after)
      } | ${delta === undefined ? "" : `${signed(delta)}${unit(name)}${percent}`} | ${status} |`,
    );
  }

  const summary = [
    "### Size budgets",
    "",
    "| Metric | Base | Head | Change | Budget |",
    "| --- | ---: | ---: | ---: | --- |",
    ...rows,
    "",
    failures.length && override
      ? "Over budget, but approved with the `perf-regression-approved` label."
      : failures.length
        ? "Over budget. Shrink the change, raise the budget in `perf/budgets.json` with Yara's sign-off, or have Yara add the `perf-regression-approved` label."
        : "All budgets met.",
    "",
  ].join("\n");
  console.log(summary);
  if (process.env.GITHUB_STEP_SUMMARY)
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${summary}\n`);
  for (const failure of failures)
    console.error(`${override ? "approved" : "over budget"}: ${failure}`);
  if (failures.length && !override) process.exit(1);
}

function isCount(name: string) {
  return name.startsWith("deps/");
}

function unit(name: string) {
  return isCount(name) ? "" : " B";
}

function format(name: string, value: number) {
  return isCount(name) ? String(value) : `${(value / 1024).toFixed(1)} KB`;
}

function signed(value: number, digits = 0) {
  return `${value >= 0 ? "+" : ""}${value.toFixed(digits)}`;
}
