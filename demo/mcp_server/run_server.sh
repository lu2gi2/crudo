#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CRUDO_DEMO_ROOT="$ROOT"
TOOLS_ROOT="${CRUDO_TOOLS_ROOT:-${HOME}/crudo-tools}"
export CRUDO_DOCLING_PYTHONPATH="${CRUDO_DOCLING_PYTHONPATH:-$TOOLS_ROOT/venv/docling/lib/python3.12/site-packages}"
PYTHON_BIN="${CRUDO_DEMO_PYTHON:-$TOOLS_ROOT/venv/demo/bin/python}"
if [[ ! -x "$PYTHON_BIN" ]]; then
  PYTHON_BIN="$(command -v python3 || true)"
fi
if [[ -z "$PYTHON_BIN" || ! -x "$PYTHON_BIN" ]]; then
  printf 'Error: demo Python not found; set CRUDO_DEMO_PYTHON.\n' >&2
  exit 1
fi
exec "$PYTHON_BIN" "$ROOT/mcp_server/server.py"
