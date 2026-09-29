---
name: verify-in-app
description: Run the real Conductor app on isolated data and drive it (Claude via the Tauri MCP tools, Codex via its own computer use) to confirm a UI or end-to-end change works. Use after `pnpm check` passes on any visual change (layout, styling, focus, what shows when), or when Yara asks.
---

# Verify In The Running App

Tests prove logic; this proves the assembled app: layout, focus, hotkeys,
dialogs, and flows that cross IPC. It does not replace `pnpm check`.

## Which Tools Drive The App

- **Codex / agents with native computer use:** drive the Conductor window
  yourself and skip the MCP-specific steps below.
- **Claude:** use the `tauri` MCP server from `.mcp.json`. It talks to
  `tauri-plugin-mcp-bridge`, which only the `mcp-bridge` Cargo feature
  compiles in (`pnpm dev:agent`), listening on `127.0.0.1` only. Prefer it
  over `computer-use`, which is only for native UI the bridge cannot reach
  (menu bar, file dialogs).

If neither is available, say so clearly and stop. Do not fall back to anything else.

## 1. Start

```bash
pnpm agent:start                    # empty database in .agent-app/data
pnpm agent:start --copy-real-data   # snapshot of Yara's data; only if she asks
```

It returns once the bridge is listening. On failure it prints the tail of
`.agent-app/dev.log`. Point any request you send at a local server you
started (for example `python3 -m http.server 8765 --bind 127.0.0.1`) and stop
it by its PID.

## 2. Get Data In

An empty database hides most bugs. Seed through IPC, not native file dialogs:
`ipc_execute_command` with `import_postman_collection` and
`{"postmanJson": "<collection JSON string>"}`, then `location.reload()` via
`webview_execute_js`. Any command in `src/bindings.ts` works this way by its
snake_case name.

## 3. Drive And Observe

Start with `driver_session`, then use an accessibility `webview_dom_snapshot`
to get `[ref=eN]` handles. After each action, `webview_wait_for` the text that
proves it worked instead of sleeping. Shortcuts are in `src/app/hotkeys.ts`.
Check `read_logs` (console) for frontend errors and `.agent-app/dev.log` for
Rust panics.

## 4. Stop

Leave the app running if Yara wants to look at it. Otherwise:

```bash
pnpm agent:stop           # keeps .agent-app/data
pnpm agent:stop --clean   # deletes it; always use this after --copy-real-data
```

## Reporting

Say what you did and what proved it ("sent with ⌘↵, waited for the body text;
the screenshot shows 200 and the JSON"), and what you did not verify. A clean
launch is not proof that a feature works.
