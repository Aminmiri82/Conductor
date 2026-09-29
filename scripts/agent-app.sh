#!/usr/bin/env bash
# Kept as the entry point for existing agent instructions.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
exec node "$ROOT/scripts/agent-app.mjs" "$@"
