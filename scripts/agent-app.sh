#!/usr/bin/env bash
# Runs Conductor for agent verification: isolated data in .agent-app/data, the
# MCP bridge enabled, and every spawned PID tracked so `stop` never kills by
# name pattern.
#
#   scripts/agent-app.sh start [--copy-real-data]
#   scripts/agent-app.sh stop [--clean]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STATE="$ROOT/.agent-app"
LAUNCHER_PID="$STATE/launcher.pid"
REAL_DB="$HOME/Library/Application Support/com.yaramiri.conductor/conductor.sqlite3"

# Prints a PID and all of its descendants, parents first.
descendants() {
  echo "$1"
  local child
  for child in $(pgrep -P "$1" || true); do
    descendants "$child"
  done
}

bridge_port() {
  local pid
  for pid in "$@"; do
    lsof -nP -a -p "$pid" -iTCP:9223-9322 -sTCP:LISTEN 2>/dev/null | awk 'NR > 1 { split($9, a, ":"); print a[2]; exit }' || true
  done | head -n 1 || true
}

start() {
  if [[ -f "$LAUNCHER_PID" ]] && kill -0 "$(cat "$LAUNCHER_PID")" 2>/dev/null; then
    echo "Already running (log: $STATE/dev.log). Run: pnpm agent:stop" >&2
    exit 1
  fi
  if lsof -nP -iTCP:1420 -sTCP:LISTEN >/dev/null 2>&1; then
    echo "Port 1420 is in use, probably another 'tauri dev'. Not touching it." >&2
    exit 1
  fi

  mkdir -p "$STATE/data"
  if [[ "${1:-}" == "--copy-real-data" ]]; then
    rm -f "$STATE"/data/conductor.sqlite3*
    # VACUUM INTO reads the live database consistently and never writes to it.
    sqlite3 "file:$REAL_DB?mode=ro" "VACUUM INTO '$STATE/data/conductor.sqlite3'"
    echo "Copied real data into $STATE/data (contains secrets; stop with --clean when done)."
  fi

  cd "$ROOT"
  CONDUCTOR_DATA_DIR="$STATE/data" nohup pnpm dev:agent >"$STATE/dev.log" 2>&1 &
  echo $! >"$LAUNCHER_PID"

  echo "Building and launching (first build takes a few minutes)..."
  local port=""
  for _ in $(seq 1 600); do
    if ! kill -0 "$(cat "$LAUNCHER_PID")" 2>/dev/null; then
      echo "Launcher exited. Last log lines:" >&2
      tail -n 30 "$STATE/dev.log" >&2
      rm -f "$LAUNCHER_PID"
      exit 1
    fi
    # shellcheck disable=SC2046
    port="$(bridge_port $(descendants "$(cat "$LAUNCHER_PID")"))"
    [[ -n "$port" ]] && break
    sleep 1
  done
  if [[ -z "$port" ]]; then
    echo "Timed out waiting for the MCP bridge. See $STATE/dev.log" >&2
    exit 1
  fi
  echo "Ready: MCP bridge on 127.0.0.1:$port, data in $STATE/data, log in $STATE/dev.log"
}

stop() {
  if [[ -f "$LAUNCHER_PID" ]]; then
    local pids
    read -r -a pids <<<"$(descendants "$(cat "$LAUNCHER_PID")" | tr '\n' ' ')"
    kill "${pids[@]}" 2>/dev/null || true
    for _ in $(seq 1 20); do
      local alive=0 pid
      for pid in "${pids[@]}"; do kill -0 "$pid" 2>/dev/null && alive=1; done
      [[ $alive == 0 ]] && break
      sleep 0.5
    done
    kill -9 "${pids[@]}" 2>/dev/null || true
    rm -f "$LAUNCHER_PID"
    echo "Stopped."
  else
    echo "Not running."
  fi
  if [[ "${1:-}" == "--clean" ]]; then
    rm -rf "$STATE"
    echo "Removed $STATE."
  fi
}

case "${1:-}" in
  start) start "${2:-}" ;;
  stop) stop "${2:-}" ;;
  *) echo "usage: $0 start [--copy-real-data] | stop [--clean]" >&2; exit 2 ;;
esac
