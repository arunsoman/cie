#!/usr/bin/env bash
# record_init.sh — R15's record-then-commit pattern: run `cie init`
# against a scratch project, show what it registered, then prove the
# registered entry IS the server (real stdio tools/list handshake).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [ -x "$REPO_ROOT/.venv/bin/python" ]; then PY="$REPO_ROOT/.venv/bin/python"; else PY="${PY:-python3}"; fi
export PYTHONPATH="$REPO_ROOT${PYTHONPATH:+:$PYTHONPATH}"

WORK="$(mktemp -d)"
mkdir -p "$WORK/home"
export HOME="$WORK/home"

mkdir -p "$WORK/proj"
cat > "$WORK/proj/app.py" <<'PY'
def helper():
    return 0


def alpha():
    return helper()
PY
cat > "$WORK/home/.claude.json" <<'JSON'
{"mcpServers": {}}
JSON

echo "$ cie init $WORK/proj"
"$PY" -m cie.cli init "$WORK/proj"

echo
echo "$ cat $WORK/proj/.mcp.json"
cat "$WORK/proj/.mcp.json"

echo
echo "== real stdio handshake on the registered entry (client view)"
"$PY" tool-test-lab/dogfood_mcp_stdio_list.py "$WORK/proj"

echo "done — artifacts: the registered .mcp.json + the client-side listing."