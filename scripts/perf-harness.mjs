// The perf harness: measures Conductor's memory and CPU through a fixed
// scenario, on macOS. See docs/01-architecture/01-performance.md#perf-harness.
//
//   node scripts/perf-harness.mjs run [--runs 3] [--out file] [--skip-build]
//   node scripts/perf-harness.mjs compare <before.json> <after.json>
//
// `run` builds a release app with the `perf-harness` feature (its own name,
// identifier, and target dir, so it never touches the real app or its data),
// then launches it on a throwaway data directory against a local echo server.
// The webview runs src/app/perfScenario.ts and the app records a checkpoint
// after each step. Numbers are the median across runs.
//
// `compare` only reports: memory and timings move between runs, so there
// are no budgets yet.
import { execFileSync, spawn } from "node:child_process";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const targetDir = join(root, "src-tauri/target/perf-harness");
const productName = "Conductor Perf Harness";
const bundle = join(targetDir, `release/bundle/macos/${productName}.app`);
const executable = join(bundle, "Contents/MacOS/conductor");
const SCENARIO_TIMEOUT_MS = 3 * 60_000;
const COLLECTION = { folders: 40, requestsPerFolder: 50 };
const LARGE_RESPONSE_BYTES = 10_000_000;
const ROLES = ["app", "WebContent", "Networking", "GPU", "other"];

const [command, ...rest] = process.argv.slice(2);
const { values: options, positionals } = parseArgs({
  args: rest,
  allowPositionals: true,
  options: {
    runs: { type: "string", default: "3" },
    out: { type: "string" },
    "skip-build": { type: "boolean", default: false },
  },
});

if (process.platform !== "darwin") {
  console.error("The perf harness measures macOS only.");
  process.exit(2);
}
if (command === "run") await run();
else if (command === "compare") compare(...positionals);
else {
  console.error("usage: perf-harness.mjs run|compare (see the header comment)");
  process.exit(2);
}

async function run() {
  if (!options["skip-build"]) await build();
  const runs = [];
  for (let index = 0; index < Number(options.runs); index++) {
    console.log(`Run ${index + 1} of ${options.runs}...`);
    runs.push(await measureOnce());
  }
  const metrics = median(runs);
  report(metrics, runs);
  const json = `${JSON.stringify(metrics, null, 2)}\n`;
  if (options.out) writeFileSync(options.out, json);
}

async function build() {
  console.log(`Building ${productName}.app (release, perf-harness)...`);
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
    { CARGO_TARGET_DIR: targetDir, VITE_PERF_HARNESS: "1" },
  );
}

async function measureOnce() {
  const work = mkdtempSync(join(tmpdir(), "conductor-perf-"));
  const port = await freePort();
  const echo = spawn(
    process.execPath,
    [join(root, "scripts/echo-server.mjs"), "--port", String(port)],
    { stdio: ["ignore", "pipe", "inherit"] },
  );
  try {
    await new Promise((ready, fail) => {
      echo.stdout.once("data", ready);
      echo.once("exit", () => fail(new Error("echo server exited")));
    });
    echo.stdout.resume();
    const collectionPath = join(work, "collection.json");
    writeFileSync(collectionPath, generateCollection(port));
    const out = join(work, "checkpoints.jsonl");
    return toMetrics(
      await launch(out, join(work, "app.log"), {
        CONDUCTOR_DATA_DIR: join(work, "data"),
        CONDUCTOR_PERF_OUT: out,
        CONDUCTOR_PERF_COLLECTION: collectionPath,
      }),
    );
  } finally {
    echo.kill();
    rmSync(work, { recursive: true, force: true });
  }
}

// Launches the app this script built and waits for the scenario to quit it.
// It goes through LaunchServices (`open`) rather than spawn: started from a
// shell, macOS holds the terminal responsible for the app and its WebKit
// processes, so the app could not find them. Returns the app's output lines.
async function launch(out, log, env) {
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
      bundle,
    ],
    { stdio: "inherit" },
  );
  const timer = setTimeout(stopHungApp, SCENARIO_TIMEOUT_MS, out);
  await new Promise((done, fail) => {
    opener.once("error", fail);
    opener.once("exit", done);
  });
  clearTimeout(timer);
  const lines = existsSync(out)
    ? readFileSync(out, "utf8")
        .trim()
        .split("\n")
        .map((line) => JSON.parse(line))
    : [];
  const failure = lines.find((line) => "error" in line);
  if (failure) throw new Error(`scenario failed: ${failure.error}`);
  const checkpoints = lines.filter((line) => "label" in line);
  if (!checkpoints.length)
    throw new Error(`no checkpoints recorded; app output:\n${readLog(log)}`);
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
// is still the profiling build.
function stopHungApp(out) {
  console.error("Scenario timed out; stopping the app this run started.");
  const pid = existsSync(out)
    ? JSON.parse(readFileSync(out, "utf8").split("\n")[0]).pid
    : undefined;
  if (!pid) return;
  try {
    const command = execFileSync("ps", ["-p", String(pid), "-o", "comm="], {
      encoding: "utf8",
    }).trim();
    if (command === executable) process.kill(pid, "SIGKILL");
  } catch {
    // Already gone.
  }
}

function readLog(log) {
  return existsSync(log) ? readFileSync(log, "utf8").slice(-4000) : "";
}

function generateCollection(port) {
  const base = `http://127.0.0.1:${port}`;
  const item = [
    // First, so the scenario finds it at the top of the tree.
    {
      name: "Large response",
      request: {
        method: "GET",
        url: `${base}/large?_json=${LARGE_RESPONSE_BYTES}`,
      },
    },
  ];
  for (let folder = 0; folder < COLLECTION.folders; folder++) {
    const requests = [];
    for (let index = 0; index < COLLECTION.requestsPerFolder; index++) {
      requests.push({
        name: `Request ${folder}.${index}`,
        request: {
          method: index % 2 ? "POST" : "GET",
          header: [
            { key: "Accept", value: "application/json" },
            { key: "X-Trace", value: `${folder}-${index}` },
          ],
          url: `${base}/folders/${folder}/items/${index}?page=1&limit=50`,
          ...(index % 2 && {
            body: {
              mode: "raw",
              raw: JSON.stringify({ folder, index, note: "x".repeat(200) }),
              options: { raw: { language: "json" } },
            },
          }),
        },
      });
    }
    item.push({ name: `Folder ${folder}`, item: requests });
  }
  return JSON.stringify({
    info: {
      name: "Perf harness",
      schema:
        "https://schema.getpostman.com/json/collection/v2.1.0/collection.json",
    },
    item,
  });
}

// The app process is listed first. WebKit may run several WebContent
// processes, which are summed; macOS helpers it attributes to the app go
// under `other`.
function roleOf(name, index) {
  if (index === 0) return "app";
  return ROLES.find((role) => name.endsWith(`.${role}`)) ?? "other";
}

// Flat metric names, so runs can be combined and compared name by name:
//   startup                      ms from process start to first paint
//   <step>/time                  ms until the step's result painted
//   <step>/memory/<role|total>   footprint bytes at the checkpoint
//   <step>/rust-heap(-peak)      bytes
//   <step>/cpu/<role|total>      ms of CPU since the previous checkpoint
//   <step>/wakeups-per-second    all processes, since the previous checkpoint
function toMetrics({ startupMs, checkpoints }) {
  const metrics = { startup: startupMs };
  const previous = new Map();
  let previousAt = 0;
  for (const checkpoint of checkpoints) {
    const { label, stepMs, atMs, rustHeap, rustHeapPeak, processes } =
      checkpoint;
    const add = (name, value) =>
      (metrics[`${label}/${name}`] =
        (metrics[`${label}/${name}`] ?? 0) + value);
    metrics[`${label}/time`] = stepMs;
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
    metrics[`${label}/wakeups-per-second`] =
      wakeups / ((atMs - previousAt) / 1000);
    metrics[`${label}/rust-heap`] = rustHeap;
    metrics[`${label}/rust-heap-peak`] = rustHeapPeak;
    previousAt = atMs;
  }
  return metrics;
}

function median(runs) {
  const names = [...new Set(runs.flatMap(Object.keys))];
  return Object.fromEntries(
    names.map((name) => {
      const values = runs.map((run) => run[name] ?? 0).sort((a, b) => a - b);
      return [name, values[Math.floor(values.length / 2)]];
    }),
  );
}

function report(metrics, runs) {
  const steps = [
    ...new Set(
      Object.keys(metrics)
        .filter((name) => name.endsWith("/time"))
        .map((name) => name.slice(0, -"/time".length)),
    ),
  ];
  // Half the spread across runs: a change smaller than this is noise.
  const spread = (name) => {
    const values = runs.map((run) => run[name] ?? 0);
    return (Math.max(...values) - Math.min(...values)) / 2;
  };
  const value = (step, name) => metrics[`${step}/${name}`] ?? 0;

  console.log(
    `\nStartup: ${ms(metrics.startup)} from process start to first paint (±${ms(spread("startup"))})`,
  );

  console.log("\nMemory at each checkpoint, MB");
  table(
    ["step", ...ROLES, "total", "±", "change", "Rust heap", "Rust peak"],
    steps.map((step, index) => [
      step,
      ...ROLES.map((role) => mbNumber(value(step, `memory/${role}`))),
      mbNumber(value(step, "memory/total")),
      mbNumber(spread(`${step}/memory/total`)),
      index
        ? signed(
            mbNumber(
              value(step, "memory/total") -
                value(steps[index - 1], "memory/total"),
            ),
          )
        : "",
      mbNumber(value(step, "rust-heap")),
      mbNumber(value(step, "rust-heap-peak")),
    ]),
  );
  const first = steps[0];
  const last = steps.at(-1);
  console.log(
    `Still held at the end, compared with ${first}: ${signed(
      mbNumber(value(last, "memory/total") - value(first, "memory/total")),
    )} MB`,
  );

  console.log(
    "\nCPU per step (ms of CPU time since the previous checkpoint, settle time included)",
  );
  table(
    ["step", "step ms", "±", ...ROLES, "total", "wakeups/s"],
    steps.map((step) => [
      step,
      value(step, "time").toFixed(0),
      spread(`${step}/time`).toFixed(0),
      ...ROLES.map((role) => value(step, `cpu/${role}`).toFixed(0)),
      value(step, "cpu/total").toFixed(0),
      value(step, "wakeups-per-second").toFixed(1),
    ]),
  );
}

function compare(beforePath, afterPath) {
  if (!beforePath || !afterPath)
    throw new Error("compare needs <before.json> <after.json>");
  const before = JSON.parse(readFileSync(beforePath, "utf8"));
  const after = JSON.parse(readFileSync(afterPath, "utf8"));
  const rows = [];
  for (const name of new Set([...Object.keys(before), ...Object.keys(after)])) {
    const [a, b] = [before[name], after[name]];
    const format = formatterFor(name);
    rows.push([
      name,
      a === undefined ? "new" : format(a),
      b === undefined ? "removed" : format(b),
      a === undefined || b === undefined
        ? ""
        : `${signed(format(b - a))}${a ? ` (${signed(((b - a) / a) * 100, 1)}%)` : ""}`,
    ]);
  }
  table(["metric", "before", "after", "change"], rows);
}

function formatterFor(name) {
  if (name.includes("/memory/") || name.includes("rust-heap"))
    return (bytes) => `${mbNumber(bytes)} MB`;
  if (name.endsWith("wakeups-per-second")) return (rate) => rate.toFixed(1);
  return ms;
}

function table(headers, rows) {
  const widths = headers.map((header, column) =>
    Math.max(header.length, ...rows.map((row) => String(row[column]).length)),
  );
  const line = (cells) =>
    cells
      .map((cell, column) =>
        column
          ? String(cell).padStart(widths[column])
          : String(cell).padEnd(widths[column]),
      )
      .join("  ");
  console.log(line(headers));
  for (const row of rows) console.log(line(row));
}

function mbNumber(bytes) {
  return (bytes / 1024 / 1024).toFixed(1);
}

function ms(value) {
  return `${Number(value).toFixed(0)} ms`;
}

// Adds a + to a positive number or a formatted one such as "12 ms".
function signed(value, digits) {
  const text = digits === undefined ? String(value) : value.toFixed(digits);
  return parseFloat(text) > 0 ? `+${text}` : text;
}

function freePort() {
  return new Promise((done, fail) => {
    const server = createServer();
    server.once("error", fail);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => done(port));
    });
  });
}

function exec(file, args, env) {
  return new Promise((done, fail) => {
    const child = spawn(file, args, {
      cwd: root,
      stdio: "inherit",
      env: { ...process.env, ...env },
    });
    child.once("error", fail);
    child.once("exit", (code) =>
      code === 0 ? done() : fail(new Error(`${file} exited with ${code}`)),
    );
  });
}
