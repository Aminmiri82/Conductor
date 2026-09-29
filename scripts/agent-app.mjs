import { spawn, execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  openSync,
  closeSync,
  realpathSync,
  lstatSync,
  rmSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const stateDir = join(root, ".agent-app");
const stateFile = join(stateDir, "run.json");
const dataDir = join(stateDir, "data");
const log = join(stateDir, "dev.log");
const bundle = join(
  root,
  "src-tauri/target/debug/bundle/macos/Conductor Agent.app",
);
const executable = join(bundle, "Contents/MacOS/conductor");
const identifier = "com.yaramiri.conductor.agent";
const [command, option] = process.argv.slice(2);

function run(file, args) {
  return execFileSync(file, args, { cwd: root, encoding: "utf8" }).trim();
}

function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    if (error.code === "ESRCH") return false;
    throw error;
  }
}

function startedAt(pid) {
  return run("ps", ["-p", String(pid), "-o", "lstart="]);
}

function bundleValue(key) {
  return run("/usr/libexec/PlistBuddy", [
    "-c",
    `Print :${key}`,
    join(bundle, "Contents/Info.plist"),
  ]);
}

function readState() {
  if (!existsSync(stateFile))
    throw new Error(
      "No tracked agent. Run pnpm agent:start; do not launch an app by name.",
    );
  return JSON.parse(readFileSync(stateFile, "utf8"));
}

function checkOtherAgents(expectedPid) {
  const agents = run("ps", ["-axo", "pid=,comm="])
    .split("\n")
    .map((line) => line.trim().match(/^(\d+)\s+(.+)$/))
    .filter((match) =>
      match?.[2].endsWith("/Conductor Agent.app/Contents/MacOS/conductor"),
    );
  if (agents.some((match) => Number(match[1]) !== expectedPid)) {
    throw new Error(
      "Another Conductor Agent bundle is running. Refusing an ambiguous computer-use target; stop its own tracked run first.",
    );
  }
}

function checkProcess(state) {
  if (!alive(state.pid))
    throw new Error(
      "The tracked agent has exited. Run pnpm agent:stop, then pnpm agent:start.",
    );
  if (
    state.executable !== executable ||
    state.dataDir !== realpathSync(dataDir) ||
    startedAt(state.pid) !== state.startedAt ||
    run("ps", ["-p", String(state.pid), "-o", "comm="]) !== executable
  ) {
    throw new Error(
      "Agent process identity does not match. Refusing to target or stop it.",
    );
  }
}

function inspect(state) {
  checkProcess(state);
  checkOtherAgents(state.pid);
  if (
    bundleValue("CFBundleIdentifier") !== identifier ||
    bundleValue("CFBundleShortVersionString") !== state.version
  ) {
    throw new Error(
      "Agent bundle changed since launch. Restart with pnpm agent:start.",
    );
  }
  const files = run("lsof", ["-a", "-p", String(state.pid), "-Fn"]);
  if (
    !files.split("\n").includes(`n${join(state.dataDir, "conductor.sqlite3")}`)
  ) {
    throw new Error("Agent has not opened the isolated database.");
  }
  const listeners = run("lsof", [
    "-nP",
    "-a",
    "-p",
    String(state.pid),
    "-iTCP",
    "-sTCP:LISTEN",
    "-Fn",
  ]);
  const port = listeners.match(/^n127\.0\.0\.1:(\d+)$/m)?.[1];
  if (!port) throw new Error("Agent bridge is not listening on loopback.");
  return { ...state, bridge: `127.0.0.1:${port}` };
}

async function stop() {
  if (!existsSync(stateFile)) {
    console.log("No tracked agent to stop.");
    return;
  }
  const state = readState();
  if (alive(state.pid)) {
    checkProcess(state);
    process.kill(state.pid, "SIGTERM");
    for (let attempt = 0; attempt < 40 && alive(state.pid); attempt++)
      await delay(100);
    if (alive(state.pid)) {
      checkProcess(state);
      process.kill(state.pid, "SIGKILL");
    }
  }
  rmSync(stateFile);
  console.log("Stopped the tracked agent.");
}

async function start() {
  checkOtherAgents();
  if (existsSync(stateFile))
    throw new Error(
      "An agent run is already recorded. Use pnpm agent:status or pnpm agent:stop first.",
    );
  // An older launcher may still own a dev process. Never overwrite its tracking.
  const oldPidFile = join(stateDir, "launcher.pid");
  if (
    existsSync(oldPidFile) &&
    alive(Number(readFileSync(oldPidFile, "utf8").trim()))
  ) {
    throw new Error(
      "An old agent launcher is still running. Stop that tracked run before switching launchers.",
    );
  }
  mkdirSync(dataDir, { recursive: true });
  if (realpathSync(dataDir) !== dataDir)
    throw new Error("Agent data directory must not be a symlink.");
  if (option === "--copy-real-data") {
    const source = join(
      process.env.HOME,
      "Library/Application Support/com.yaramiri.conductor/conductor.sqlite3",
    );
    for (const suffix of ["", "-shm", "-wal"])
      rmSync(join(dataDir, `conductor.sqlite3${suffix}`), { force: true });
    run("sqlite3", [
      `file:${source}?mode=ro`,
      `VACUUM INTO '${join(dataDir, "conductor.sqlite3").replaceAll("'", "''")}'`,
    ]);
    console.log(
      "Copied real data into the isolated directory. Use pnpm agent:stop --clean when finished.",
    );
  }
  console.log("Building a fresh Conductor Agent.app with bundled frontend...");
  await new Promise((resolveBuild, reject) => {
    const build = spawn(
      "pnpm",
      [
        "tauri",
        "build",
        "--debug",
        "--features",
        "mcp-bridge",
        "--bundles",
        "app",
        "--config",
        "src-tauri/tauri.agent.conf.json",
      ],
      { cwd: root, stdio: "inherit" },
    );
    build.on("error", reject);
    build.on("exit", (code) =>
      code === 0
        ? resolveBuild()
        : reject(new Error(`Agent build exited with ${code}`)),
    );
  });
  if (bundleValue("CFBundleIdentifier") !== identifier)
    throw new Error("Build produced the wrong app identifier.");
  const version = JSON.parse(
    readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"),
  ).version;
  if (bundleValue("CFBundleShortVersionString") !== version)
    throw new Error("Build produced a stale app version.");
  const fd = openSync(log, "w", 0o600);
  // A new process session plus file-backed stdio survives the launching terminal/tool.
  const child = spawn(executable, [], {
    cwd: root,
    detached: true,
    stdio: ["ignore", fd, fd],
    env: { ...process.env, CONDUCTOR_DATA_DIR: dataDir },
  });
  closeSync(fd);
  await new Promise((resolveSpawn, reject) => {
    child.once("spawn", resolveSpawn);
    child.once("error", reject);
  });
  child.unref();
  const state = {
    pid: child.pid,
    startedAt: startedAt(child.pid),
    executable,
    bundle,
    identifier,
    dataDir,
    version,
    log,
  };
  writeFileSync(stateFile, JSON.stringify(state, null, 2) + "\n", {
    mode: 0o600,
  });
  let lastError;
  for (let attempt = 0; attempt < 100; attempt++) {
    if (!alive(state.pid)) break;
    try {
      console.log(JSON.stringify(inspect(state), null, 2));
      console.log(
        "Before computer use, run pnpm agent:status. Target only the reported identifier; never Conductor by name.",
      );
      return;
    } catch (error) {
      lastError = error;
    }
    await delay(200);
  }
  await stop();
  throw new Error(
    `Agent did not become ready: ${lastError?.message ?? "process exited"}\n${readFileSync(log, "utf8").slice(-4000)}`,
  );
}

try {
  if (process.platform !== "darwin")
    throw new Error("The agent bundle launcher currently supports macOS only.");
  if (
    !["start", "stop", "status"].includes(command) ||
    (option &&
      !(command === "start" && option === "--copy-real-data") &&
      !(command === "stop" && option === "--clean"))
  ) {
    throw new Error(
      "Usage: agent-app.mjs start [--copy-real-data] | status | stop [--clean]",
    );
  }
  if (existsSync(stateDir) && lstatSync(stateDir).isSymbolicLink())
    throw new Error(".agent-app must not be a symlink.");
  if (command === "start") await start();
  if (command === "status")
    console.log(JSON.stringify(inspect(readState()), null, 2));
  if (command === "stop") {
    await stop();
    if (option === "--clean")
      rmSync(stateDir, { recursive: true, force: true });
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
