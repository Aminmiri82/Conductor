// The perf harness: measures Conductor's memory, CPU, and responsiveness
// through scripted scenarios, on macOS. See
// docs/01-architecture/01-performance.md#perf-harness.
//
//   node scripts/perf/harness.ts run [--scenario name]... [--runs 5]
//       [--warmup 1] [--quick] [--out file] [--skip-build]
//   node scripts/perf/harness.ts list
//   node scripts/perf/harness.ts compare <before.json> <after.json> [--markdown]
//
// CI only (the Perf workflow's harness job):
//   node scripts/perf/harness.ts build [--root dir] [--target-dir dir] --app-out <path.app>
//   node scripts/perf/harness.ts run --app <path.app> [...run options]
//   node scripts/perf/harness.ts ab --base <a.app> --head <b.app>
//       --out-base <file> --out-head <file> [--scenario name]... [--runs 5]
//
// `run` builds a release app with the `perf-harness` feature (its own name,
// identifier, and target dir, so it never touches the real app or its data),
// then launches it once per scenario and run, each on a throwaway data
// directory against a local echo server. Scenarios live in
// src/app/perf/scenarios/. A warm-up run is measured and thrown away, and
// every result is saved under src-tauri/target/perf-harness/results/.
//
// `ab` runs two builds alternately on one machine (base, head, head, base,
// ...), so drift and heat land on both sides. `compare` calls a change real
// only when the runs separate (see harness-results.ts); nothing here fails a check.
import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  lstatSync,
  readlinkSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createServer, type AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import type { FixtureName } from "../../src/app/perf/fixtures.ts";
import {
  ROLES,
  SCHEMA,
  comparisonMarkdown,
  comparisonText,
  mb,
  medians,
  ms,
  signed,
  spread,
  textTable,
  toMetrics,
  valuesOf,
  type Checkpoint,
  type Machine,
  type Metrics,
  type Result,
  type Settings,
  type Stamp,
} from "./harness-results.ts";

// Must match PROTOCOL in src-tauri/src/perf_harness.rs.
const PROTOCOL = 1;
const here = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const productName = "Conductor Perf Harness";
const SCENARIO_TIMEOUT_MS = 5 * 60_000;
const SETTINGS = { settleMs: 3000, idleMs: 15_000 };
const QUICK_SETTINGS = { settleMs: 500, idleMs: 3000 };
// What goes into a build, so editing docs or this script does not make the
// last build look stale.
const APP_SOURCES = [
  "src",
  "src-tauri",
  "index.html",
  "package.json",
  "pnpm-lock.yaml",
  "vite.config.ts",
  "tsconfig.json",
];

const [command, ...rest] = process.argv.slice(2);
const { values: options, positionals } = parseArgs({
  args: rest,
  allowPositionals: true,
  options: {
    scenario: { type: "string", multiple: true },
    runs: { type: "string" },
    warmup: { type: "string" },
    quick: { type: "boolean", default: false },
    out: { type: "string" },
    "skip-build": { type: "boolean", default: false },
    root: { type: "string" },
    "app-out": { type: "string" },
    "target-dir": { type: "string" },
    app: { type: "string" },
    base: { type: "string" },
    head: { type: "string" },
    "out-base": { type: "string" },
    "out-head": { type: "string" },
    markdown: { type: "boolean", default: false },
  },
});

// A build this script made; `stamp` says what it was made from.
type App = { bundle: string; stamp: Stamp };
// A build in a source tree, before `buildApp` or `warnIfStale` stamps it.
type LocalApp = {
  root: string;
  targetDir: string;
  bundle: string;
  stamp?: Stamp;
};
type LoadedScenario = Awaited<ReturnType<typeof loadScenarios>>[number];
// The parts of a scenario module the harness reads. Not `Scenario` from
// scenario.ts, whose `ctx` type would pull the app's stores into this
// program; loadScenarios checks the module at runtime instead.
type ScenarioModule = {
  default?: {
    description: string;
    fixtures: readonly FixtureName[];
    prepare?: unknown;
    run?: unknown;
  };
};
// The JSON lines the app writes; see `append` in perf_harness.rs. Apps from
// before the protocol existed write no `protocol`.
type OutputLine =
  | { pid: number; protocol?: number }
  | { startupMs: number }
  | Checkpoint
  | { error: string };

const commands: Partial<Record<string, () => Promise<void> | void>> = {
  run,
  build,
  ab,
  list,
  compare,
};
if (!commands[command]) {
  console.error(
    "usage: harness.ts run|list|compare|build|ab (see the header comment)",
  );
  process.exit(2);
}
if (["run", "build", "ab"].includes(command) && process.platform !== "darwin") {
  console.error("The perf harness measures macOS only.");
  process.exit(2);
}
try {
  await commands[command]();
} catch (error) {
  console.error(`perf-harness: ${(error as Error).message}`);
  process.exit(1);
}

async function run() {
  let app;
  if (options.app) {
    // A build from `build --app-out`, measured as it is.
    app = appAt(resolve(options.app));
  } else {
    app = defaultApp(here);
    if (!options["skip-build"]) await buildApp(here, app);
    warnIfStale(app);
  }
  const scenarios = selectScenarios(await loadScenarios(app.stamp.root));
  const settings = settingsFromOptions();
  warnAboutPower();
  const runs = [];
  for (let index = -settings.warmup; index < settings.runs; index++) {
    const label =
      index < 0 ? "Warm-up run" : `Run ${index + 1} of ${settings.runs}`;
    runs.push(await measureRun(app, scenarios, settings, label));
  }
  const result = makeResult(app, settings, runs.slice(settings.warmup));
  report(result, scenarios);
  const out =
    options.out ??
    join(
      here,
      "src-tauri/target/perf-harness/results",
      `${fileStamp()}-${app.stamp.sha.slice(0, 7)}.json`,
    );
  save(out, result);
  console.log(`\nSaved ${out}`);
}

async function build() {
  const root = resolve(options.root ?? here);
  if (!options["app-out"]) throw new Error("build needs --app-out <path.app>");
  const app = defaultApp(root);
  await buildApp(root, app);
  const out = resolve(options["app-out"]);
  rmSync(out, { recursive: true, force: true });
  execFileSync("ditto", [app.bundle, out]);
  writeFileSync(stampPath(out), readFileSync(stampPath(app.bundle)));
  console.log(`Copied the build to ${out}`);
}

async function ab() {
  for (const name of ["base", "head", "out-base", "out-head"] as const)
    if (!options[name]) throw new Error(`ab needs --${name}`);
  const sides = await Promise.all(
    [options.base!, options.head!].map(async (bundle) => {
      const app = appAt(resolve(bundle));
      return { app, scenarios: await loadScenarios(app.stamp.root) };
    }),
  );
  // Only scenarios both builds have can be compared.
  const common = sides[0].scenarios.filter(({ name }) =>
    sides[1].scenarios.some((other) => other.name === name),
  );
  const selected = selectScenarios(common);
  for (const side of sides)
    side.scenarios = selected.map(({ name }) =>
      side.scenarios.find((scenario) => scenario.name === name)!,
    );
  const settings = settingsFromOptions();
  warnAboutPower();
  const runs: Metrics[][] = [[], []];
  for (let index = -settings.warmup; index < settings.runs; index++) {
    // base, head, head, base, ...: drift over the job lands on both.
    const order = index % 2 === 0 ? [0, 1] : [1, 0];
    for (const side of order) {
      const label = `${side ? "Head" : "Base"} ${index < 0 ? "warm-up" : `run ${index + 1} of ${settings.runs}`}`;
      runs[side].push(
        await measureRun(
          sides[side].app,
          sides[side].scenarios,
          settings,
          label,
        ),
      );
    }
  }
  const results = sides.map(({ app }, side) =>
    makeResult(app, settings, runs[side].slice(settings.warmup)),
  );
  save(resolve(options["out-base"]!), results[0]);
  save(resolve(options["out-head"]!), results[1]);
  console.log(`\n${comparisonText(results[0], results[1])}`);
}

async function list() {
  for (const { name, description } of await loadScenarios(here))
    console.log(`${name.padEnd(28)} ${description}`);
}

function compare() {
  const [beforePath, afterPath] = positionals;
  if (!beforePath || !afterPath)
    throw new Error("compare needs <before.json> <after.json>");
  const [before, after] = [beforePath, afterPath].map(load);
  if (options.markdown) console.log(comparisonMarkdown(before, after));
  else console.log(comparisonText(before, after));
}

// Imports each scenario file in Node (it strips the types), which is why
// scenario files import only ../scenario.ts and ../fixtures.ts.
async function loadScenarios(root: string) {
  const directory = join(root, "src/app/perf/scenarios");
  const files = readdirSync(directory)
    .filter((file) => file.endsWith(".ts"))
    .sort();
  const { fixtures }: typeof import("../../src/app/perf/fixtures.ts") =
    await import(pathToFileURL(join(root, "src/app/perf/fixtures.ts")).href);
  return Promise.all(
    files.map(async (file) => {
      const name = basename(file, ".ts");
      let scenario: ScenarioModule["default"];
      try {
        ({ default: scenario } = (await import(
          pathToFileURL(join(directory, file)).href
        )) as ScenarioModule);
      } catch (error) {
        throw new Error(
          `could not import scenario ${name} (${(error as Error).message}). Scenario files may import only ../scenario.ts and ../fixtures.ts.`,
          { cause: error },
        );
      }
      if (typeof scenario?.run !== "function")
        throw new Error(
          `scenario ${name} must default-export defineScenario({ ... })`,
        );
      return {
        name,
        description: scenario.description,
        fixtures: scenario.fixtures,
        prepares: typeof scenario.prepare === "function",
        generators: fixtures,
      };
    }),
  );
}

function selectScenarios(available: LoadedScenario[]) {
  const wanted = (options.scenario ?? []).flatMap((value) => value.split(","));
  if (!wanted.length) return available;
  return wanted.map((name) => {
    const scenario = available.find((candidate) => candidate.name === name);
    if (!scenario)
      throw new Error(
        `no scenario named ${name}; there are ${available.map(({ name }) => name).join(", ")}`,
      );
    return scenario;
  });
}

function settingsFromOptions() {
  const count = (name: "runs" | "warmup", fallback: number) => {
    const value = Number(options[name] ?? fallback);
    if (!Number.isInteger(value) || value < 0)
      throw new Error(`--${name} must be a whole number`);
    return value;
  };
  const quick = options.quick;
  const settings = {
    ...(quick ? QUICK_SETTINGS : SETTINGS),
    quick,
    runs: count("runs", quick ? 1 : 5),
    warmup: count("warmup", quick ? 0 : 1),
  };
  if (!settings.runs) throw new Error("--runs must be at least 1");
  return settings;
}

// --target-dir lets CI build the base branch into the PR's target dir, so
// compiled dependencies are reused.
function defaultApp(root: string): LocalApp {
  const targetDir = options["target-dir"]
    ? resolve(options["target-dir"])
    : join(root, "src-tauri/target/perf-harness");
  return {
    root,
    targetDir,
    bundle: join(targetDir, `release/bundle/macos/${productName}.app`),
  };
}

function appAt(bundle: string): App {
  if (!existsSync(stampPath(bundle)))
    throw new Error(`${bundle} has no build stamp; build it with this script`);
  return {
    bundle,
    stamp: JSON.parse(readFileSync(stampPath(bundle), "utf8")),
  };
}

function stampPath(bundle: string) {
  return `${bundle}.build.json`;
}

async function buildApp(root: string, app: LocalApp) {
  console.log(
    `Building ${productName}.app (release, perf-harness) from ${root}...`,
  );
  const config = {
    productName,
    identifier: "com.yaramiri.conductor.perfharness",
    // Replaces the window list, so it repeats tauri.conf.json's window.
    app: {
      windows: [
        {
          label: "main",
          title: productName,
          width: 850,
          height: 625,
          dragDropEnabled: false,
        },
      ],
    },
    // Never reach the real update feed.
    plugins: {
      updater: {
        endpoints: ["http://127.0.0.1:18765/latest.json"],
        dangerousInsecureTransportProtocol: true,
      },
    },
    bundle: { macOS: { signingIdentity: "-" } },
  };
  // Stamped before building, so it describes the sources the build read.
  const stamp = sourceStamp(root);
  await exec(
    "pnpm",
    [
      "tauri",
      "build",
      "--bundles",
      "app",
      "--features",
      "perf-harness",
      "--config",
      JSON.stringify(config),
    ],
    // VITE_PERF_HARNESS reaches `pnpm build` through beforeBuildCommand.
    {
      cwd: root,
      env: { CARGO_TARGET_DIR: app.targetDir, VITE_PERF_HARNESS: "1" },
    },
  );
  writeFileSync(
    stampPath(app.bundle),
    `${JSON.stringify({ ...stamp, builtAt: new Date().toISOString() }, null, 2)}\n`,
  );
  app.stamp = stamp;
}

/**
 * What a build was made from: the commit, whether the tree was dirty, a hash
 * of uncommitted changes to the app's sources (so a stale build can be
 * noticed), and a hash of each scenario with the files it shares, so
 * `compare` can tell when the scenario itself changed.
 */
function sourceStamp(root: string): Stamp {
  const git = (...args: string[]) =>
    execFileSync("git", ["-C", root, ...args], { encoding: "utf8" });
  const changes = createHash("sha256");
  changes.update(git("diff", "HEAD", "--binary", "--", ...APP_SOURCES));
  const untracked = git(
    "ls-files",
    "--others",
    "--exclude-standard",
    "-z",
    "--",
    ...APP_SOURCES,
  )
    .split("\0")
    .filter(Boolean);
  for (const file of untracked) {
    const path = join(root, file);
    changes.update(file);
    // A symlink is hashed as where it points; it may point at a directory.
    changes.update(
      lstatSync(path).isSymbolicLink()
        ? readlinkSync(path)
        : readFileSync(path),
    );
  }
  const hashOf = (...files: string[]) => {
    const hash = createHash("sha256");
    for (const file of files) hash.update(readFileSync(join(root, file)));
    return hash.digest("hex").slice(0, 16);
  };
  const shared = [
    "src/app/perf/fixtures.ts",
    "src/app/perf/runtime.ts",
    "src/app/perf/scenario.ts",
  ];
  const scenarioHashes = Object.fromEntries(
    readdirSync(join(root, "src/app/perf/scenarios"))
      .filter((file) => file.endsWith(".ts"))
      .map((file) => [
        basename(file, ".ts"),
        hashOf(`src/app/perf/scenarios/${file}`, ...shared),
      ]),
  );
  return {
    root,
    sha: git("rev-parse", "HEAD").trim(),
    dirty: git("status", "--porcelain").trim() !== "",
    changesHash: changes.digest("hex").slice(0, 16),
    scenarioHashes,
  };
}

// With --skip-build the last build is reused; say so when the tree moved on.
function warnIfStale(app: LocalApp): asserts app is LocalApp & App {
  const built = appAt(app.bundle).stamp;
  app.stamp = built;
  const now = sourceStamp(app.root);
  if (built.sha !== now.sha || built.changesHash !== now.changesHash)
    console.warn(
      `warning: the build is from ${built.sha.slice(0, 7)}${built.dirty ? " with uncommitted changes" : ""}, and the sources have changed since. Run without --skip-build to measure them.`,
    );
}

/** One launch (two for a scenario with `prepare`) per scenario. */
async function measureRun(
  app: App,
  scenarios: LoadedScenario[],
  settings: Settings,
  label: string,
) {
  const metrics: Metrics = {};
  for (const scenario of scenarios) {
    console.log(`${label}: ${scenario.name}...`);
    Object.assign(metrics, await measureScenario(app, scenario, settings));
  }
  return metrics;
}

async function measureScenario(
  app: App,
  scenario: LoadedScenario,
  settings: Settings,
) {
  const work = mkdtempSync(join(tmpdir(), "conductor-perf-"));
  const port = await freePort();
  const echo = spawn(
    process.execPath,
    [join(here, "scripts/echo/echo-server.ts"), "--port", String(port)],
    { stdio: ["ignore", "pipe", "inherit"] },
  );
  try {
    await new Promise((ready, fail) => {
      echo.stdout.once("data", ready);
      echo.once("exit", () => fail(new Error("echo server exited")));
    });
    echo.stdout.resume();
    const env = { echoUrl: `http://127.0.0.1:${port}` };
    const fixtures: Record<string, unknown> = {};
    for (const name of scenario.fixtures) {
      const { file, info } = scenario.generators[name](env);
      const path = join(work, `${name}.json`);
      writeFileSync(path, file);
      fixtures[name] = { ...info, path };
    }
    const config = (phase: "prepare" | "measure") => ({
      scenario: scenario.name,
      phase,
      settleMs: settings.settleMs,
      idleMs: settings.idleMs,
      fixtures,
    });
    const data = join(work, "data");
    if (scenario.prepares)
      await launch(app, work, "prepare", data, config("prepare"));
    const output = await launch(app, work, "measure", data, config("measure"));
    if (!output.checkpoints.length)
      throw new Error(`scenario ${scenario.name} recorded no checkpoints`);
    return toMetrics(scenario.name, output);
  } finally {
    echo.kill();
    rmSync(work, { recursive: true, force: true });
  }
}

// Launches the app and waits for the scenario to quit it. It goes through
// LaunchServices (`open`) rather than spawn: started from a shell, macOS
// holds the terminal responsible for the app and its WebKit processes, so
// the app could not find them.
async function launch(
  app: App,
  work: string,
  phase: "prepare" | "measure",
  data: string,
  config: { scenario: string },
) {
  const out = join(work, `${phase}.jsonl`);
  const log = join(work, `${phase}.log`);
  const env = {
    CONDUCTOR_DATA_DIR: data,
    CONDUCTOR_PERF_OUT: out,
    CONDUCTOR_PERF_CONFIG: JSON.stringify(config),
  };
  const opener = spawn(
    "open",
    [
      "-n",
      "-W",
      "--stdout",
      log,
      "--stderr",
      log,
      ...Object.entries(env).flatMap(([key, value]) => [
        "--env",
        `${key}=${value}`,
      ]),
      app.bundle,
    ],
    { stdio: "inherit" },
  );
  const timer = setTimeout(stopHungApp, SCENARIO_TIMEOUT_MS, app, out);
  await new Promise((done, fail) => {
    opener.once("error", fail);
    opener.once("exit", done);
  });
  clearTimeout(timer);
  const lines = existsSync(out)
    ? readFileSync(out, "utf8")
        .trim()
        .split("\n")
        .map((line): OutputLine => JSON.parse(line))
    : [];
  if (!lines.length)
    throw new Error(`the app wrote nothing; its output:\n${readLog(log)}`);
  const protocol = "protocol" in lines[0] ? lines[0].protocol : undefined;
  if (protocol !== PROTOCOL)
    throw new Error(
      `the app speaks perf protocol ${protocol ?? "0"} and this script ${PROTOCOL}; build both from compatible commits`,
    );
  const failure = lines.find((line) => "error" in line);
  if (failure)
    throw new Error(`${config.scenario} (${phase}) failed: ${failure.error}`);
  const checkpoints = lines.filter((line) => "label" in line);
  if (
    !checkpoints.every(({ processes }) =>
      processes.some(({ name }) => name.endsWith("WebContent")),
    )
  )
    throw new Error("a checkpoint is missing the WebContent process");
  return {
    startupMs: lines.find((line) => "startupMs" in line)?.startupMs,
    checkpoints,
  };
}

// Stops only the pid the app reported in this run's output, and only if it
// is still this build.
function stopHungApp(app: App, out: string) {
  console.error("Scenario timed out; stopping the app this run started.");
  const pid: number | undefined = existsSync(out)
    ? JSON.parse(readFileSync(out, "utf8").split("\n")[0]).pid
    : undefined;
  if (!pid) return;
  try {
    const command = execFileSync("ps", ["-p", String(pid), "-o", "comm="], {
      encoding: "utf8",
    }).trim();
    if (command === join(app.bundle, "Contents/MacOS/conductor"))
      process.kill(pid, "SIGKILL");
  } catch {
    // Already gone.
  }
}

function readLog(log: string) {
  return existsSync(log) ? readFileSync(log, "utf8").slice(-4000) : "";
}

function machine(): Machine {
  const read = (file: string, ...args: string[]) => {
    try {
      return execFileSync(file, args, { encoding: "utf8" }).trim();
    } catch {
      return "";
    }
  };
  const battery = read("pmset", "-g", "batt");
  return {
    macOS: read("sw_vers", "-productVersion"),
    chip: read("sysctl", "-n", "machdep.cpu.brand_string"),
    memoryBytes: Number(read("sysctl", "-n", "hw.memsize")),
    power: battery.includes("'Battery Power'") ? "battery" : "ac",
    lowPower: /lowpowermode\s+1/.test(read("pmset", "-g")),
  };
}

function warnAboutPower() {
  const { power, lowPower } = machine();
  if (power === "battery")
    console.warn(
      "warning: on battery power; macOS may slow the CPU. Plug in for numbers you compare.",
    );
  if (lowPower)
    console.warn(
      "warning: Low Power Mode is on, which slows the CPU. Turn it off for numbers you compare.",
    );
}

function makeResult(app: App, settings: Settings, runs: Metrics[]): Result {
  return {
    schema: SCHEMA,
    meta: {
      app: app.stamp,
      machine: machine(),
      settings,
      date: new Date().toISOString(),
    },
    runs,
    median: medians(runs),
  };
}

function save(path: string, result: Result) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, `${JSON.stringify(result, null, 2)}\n`);
}

function load(path: string) {
  const result: Result = JSON.parse(readFileSync(path, "utf8"));
  if (result.schema !== SCHEMA)
    throw new Error(
      `${path} is a result file of schema ${result.schema ?? "0"}, and this script reads ${SCHEMA}; measure it again`,
    );
  return result;
}

function report(
  { median: metrics, runs }: Result,
  scenarios: LoadedScenario[],
) {
  const value = (name: string) => metrics[name] ?? 0;
  const noise = (name: string) => spread(valuesOf(runs, name));
  for (const { name: scenario } of scenarios) {
    const prefix = `${scenario}/`;
    const steps = [
      ...new Set(
        Object.keys(metrics)
          .filter((name) => name.startsWith(prefix) && name.endsWith("/time"))
          .map((name) => name.slice(prefix.length, -"/time".length)),
      ),
    ];
    const at = (step: string, name: string) =>
      value(`${prefix}${step}/${name}`);
    console.log(`\n== ${scenario} ==`);
    console.log(
      `Startup: ${ms(value(`${prefix}startup`))} from process start until the first load painted (±${ms(noise(`${prefix}startup`))})`,
    );

    console.log("\nMemory at each checkpoint, MB");
    console.log(
      textTable(
        ["step", ...ROLES, "total", "±", "change", "Rust heap", "Rust peak"],
        steps.map((step, index) => [
          step,
          ...ROLES.map((role) => mb(at(step, `memory/${role}`))),
          mb(at(step, "memory/total")),
          mb(noise(`${prefix}${step}/memory/total`)),
          index
            ? signed(
                mb(
                  at(step, "memory/total") -
                    at(steps[index - 1], "memory/total"),
                ),
              )
            : "",
          mb(at(step, "rust-heap")),
          mb(at(step, "rust-heap-peak")),
        ]),
      ),
    );
    console.log(
      `Still held at the end, compared with ${steps[0]}: ${signed(
        mb(at(steps.at(-1)!, "memory/total") - at(steps[0], "memory/total")),
      )} MB`,
    );

    console.log(
      "\nCPU per step (ms of CPU time since the previous checkpoint, settle time included)",
    );
    console.log(
      textTable(
        ["step", ...ROLES, "total", "wakeups/s"],
        steps.map((step) => [
          step,
          ...ROLES.map((role) => at(step, `cpu/${role}`).toFixed(0)),
          at(step, "cpu/total").toFixed(0),
          at(step, "wakeups-per-second").toFixed(1),
        ]),
      ),
    );

    console.log(
      "\nResponsiveness per step, ms (keystrokes only for typing steps)",
    );
    const typed = (step: string) => `${prefix}${step}/keystroke-p50` in metrics;
    console.log(
      textTable(
        [
          "step",
          "step ms",
          "±",
          "longest frame",
          "key p50",
          "key p95",
          "key max",
        ],
        steps.map((step) => [
          step,
          at(step, "time").toFixed(0),
          noise(`${prefix}${step}/time`).toFixed(0),
          `${prefix}${step}/longest-frame` in metrics
            ? at(step, "longest-frame").toFixed(0)
            : "",
          typed(step) ? at(step, "keystroke-p50").toFixed(1) : "",
          typed(step) ? at(step, "keystroke-p95").toFixed(1) : "",
          typed(step) ? at(step, "keystroke-max").toFixed(1) : "",
        ]),
      ),
    );
  }
}

function fileStamp() {
  return new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
}

function freePort() {
  return new Promise<number>((done, fail) => {
    const server = createServer();
    server.once("error", fail);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address() as AddressInfo;
      server.close(() => done(port));
    });
  });
}

function exec(
  file: string,
  args: string[],
  { cwd, env }: { cwd: string; env: Record<string, string> },
) {
  return new Promise<void>((done, fail) => {
    const child = spawn(file, args, {
      cwd,
      stdio: "inherit",
      env: { ...process.env, ...env },
    });
    child.once("error", fail);
    child.once("exit", (code) =>
      code === 0 ? done() : fail(new Error(`${file} exited with ${code}`)),
    );
  });
}
