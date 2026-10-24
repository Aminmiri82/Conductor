---
name: verify-in-app
description: Run the real Conductor app on isolated data and drive it (Claude via the Tauri MCP tools, Codex via its own computer use) to confirm a UI or end-to-end change works. Use after `pnpm check` passes on any visual change (layout, styling, focus, what shows when), or when Yara asks.
---

# Verify In The Running App

Tests prove logic; this proves the assembled app. A successful launch is not
proof that a feature works, and this does not replace `pnpm check`.

## Which Tools Drive The App

- **Codex:** use native computer use.
- **Claude:** use the `tauri` MCP server from `.mcp.json`. The bridge is
  compiled only with the `mcp-bridge` feature and listens on loopback. It
  reaches the webview, IPC, and window management; it does not reach native
  UI: the menu bar, native file dialogs, and the title bar. To test a menu
  action's effect, send its event with `ipc_emit_event`. A session picks up
  `.mcp.json` changes only when it restarts.

If neither is available, say so clearly and stop. Do not fall back to anything else.

## 1. Start And Verify Identity

```bash
pnpm agent:start                    # isolated database in .agent-app/data
pnpm agent:start --copy-real-data   # only when Yara explicitly requests a snapshot
pnpm agent:status
```

The macOS launcher builds a fresh **Conductor Agent.app** with the frontend
bundled, then starts it in a detached process session. It survives the tool or
terminal that launched it and does not need Vite. After code changes, stop and
start again to rebuild; there is no hot reload.

`agent:status` must succeed **before the first computer-use call, when resuming
verification, and whenever the app appears to have closed**. It checks the
tracked PID and start time, executable, bundle identity, isolated database,
and bridge listener. Its JSON reports the exact bundle path, identifier,
data directory, PID, and bridge address. Use the reported bridge for MCP.

For native computer use, target only the reported identifier
`com.yaramiri.conductor.agent`. The window title must say
**Conductor Agent — isolated data**. If the identity or title differs, stop
interacting and inspect the launcher state.

Never target `Conductor`, `conductor`, `com.yaramiri.conductor`, or
`/Applications/Conductor.app`. Never use computer use to launch an app: some
state-reading APIs automatically launch missing apps, and macOS may resolve a
name or old registration to the installed app or a stale bundle. If status
fails, fix the tracked run with `agent:stop` / `agent:start`; do not try a
broader name. Do not copy executables into old bundles or launch another app
manually to work around a failed check.

Agent builds refuse to open a database without an explicit absolute data
directory, and reject release/development directories and symlinks into them.
This is a second guard; it does not make targeting the installed app safe.

## 2. Get Data In

Each run starts with empty data, because `agent:stop` removes it. Seed
collections through IPC when the scenario needs them: `ipc_execute_command`
with `import_postman_collection` and
`{"path": "<absolute path to the collection JSON>"}`. Then reload the webview
with `webview_execute_js` running `location.reload()` (it returns `null`) and
take a fresh `webview_dom_snapshot`.
Commands and arguments are in `src/bindings.ts`. Empty data is sufficient for
checks such as menu placement or settings that do not depend on collections.

For anything that sends, start the echo server and import its collection:

```bash
node scripts/echo/echo-server.ts             # http://127.0.0.1:18766; note the PID it prints
```

`scripts/echo/echo-collection.json` has one request per send path (query rows,
folder-inherited bearer auth, JSON, urlencoded and multipart bodies, a test
script that saves a variable, a 500, a slow reply, a binary download), all
using `{{baseUrl}}`. The echo reply shows exactly what Conductor sent. The
server's header comment lists its `_status`, `_delay` and `_bytes` knobs.

When a scenario needs an endpoint the echo cannot fake, write one: pass
`--routes .agent-app/<name>.ts` (format in the server's header comment), or
start another loopback server. Never send to a real environment. If the
endpoint would help later runs too, propose adding it to the echo server and
collection rather than keeping it to yourself.

The agent updater already uses `http://127.0.0.1:18765/latest.json`; use a
local fixture for update checks.

## 3. Drive And Observe

With MCP, call `driver_session` with `action: "start"` and the `port` from the
`bridge` that `agent:status` reports. Never rely on the default port; another
Tauri app with a bridge may be listening there. Then call `driver_session`
with `action: "status"` and confirm the identifier is
`com.yaramiri.conductor.agent`. If it is anything else, stop the session and
do not interact. After that, use `webview_dom_snapshot` and its `[ref=eN]`
handles.

With native computer use, obtain the app state and use its accessibility
elements. Refresh state after actions before reusing indexes. Observe the
actual result: response text, dialog, focus, or changed layout. Shortcuts are
in `src/app/hotkeys.ts`.

Response bodies are CodeMirror editors, which the DOM snapshot does not read.
Read the text with `webview_execute_js` (the response editor's `.cm-content`
`innerText`); it holds only rendered lines, so keep fixture responses small.
Read the status and time as separate elements: in a parent's `innerText` they
run together (`200` and `58 ms` become `20058 ms`).

`webview_screenshot` captures only the webview. Keep `allowScreenCapture`
false; true can raise a macOS screen-recording prompt. For frontend errors use
`read_logs` with `source: "console"`; Rust errors go only to the log reported
by `agent:status`. Report tools or checks that were unavailable.

## 4. Stop

Leave the app running only if Yara wants to inspect it. Otherwise:

```bash
pnpm agent:stop              # stops only the verified tracked PID, then removes isolated data
pnpm agent:stop --no-clean   # keeps the data, use when needed
```

Never pass `--no-clean` after `--copy-real-data`.

Stop local fixture servers by their recorded PIDs. A stale PID is never
permission to stop another process or kill by name.

## Reporting

Say what you did, what proved it, and what you did not verify. Name any
unexpected app launch or target mismatch plainly.
