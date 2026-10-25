#!/usr/bin/env bash
# Rebuild the engine (release) and restart the Next.js dev server.
#
# "Backend" here is the `cie` release binary / `cie mcp` stdio server.
# `cie mcp` has no port or daemon mode -- real MCP clients spawn a fresh
# instance per session off stdin, so there is nothing long-running to
# restart. This script just makes sure the binary builds and runs.
#
# "Frontend" is the Next.js dev server (`npm run dev`, port 3000). Any
# previous instance is killed and a fresh one is started in the background.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WEB_DIR="$ROOT/web"
PORT=3000
PID_FILE="$WEB_DIR/.dev-server.pid"
LOG_FILE="$WEB_DIR/server.log"

echo "== 1/3: cargo build --release =="
cd "$ROOT"
cargo build --release

echo
echo "== 2/3: verify the cie binary =="
BIN="$ROOT/target/release/cie"
"$BIN" --version

echo
echo "== 3/3: restart frontend dev server (port $PORT) =="

# Kill whatever previously held the port (previous run of this script,
# or a manually started `next dev`), plus whatever PID file we left behind.
if [ -f "$PID_FILE" ]; then
  OLD_PID="$(cat "$PID_FILE" 2>/dev/null || true)"
  if [ -n "${OLD_PID:-}" ] && kill -0 "$OLD_PID" 2>/dev/null; then
    echo "Stopping previous dev server (pid $OLD_PID)..."
    kill "$OLD_PID" 2>/dev/null || true
    for _ in 1 2 3 4 5; do
      kill -0 "$OLD_PID" 2>/dev/null || break
      sleep 1
    done
    kill -9 "$OLD_PID" 2>/dev/null || true
  fi
  rm -f "$PID_FILE"
fi

PORT_PIDS="$(lsof -ti tcp:"$PORT" -sTCP:LISTEN 2>/dev/null || true)"
if [ -n "$PORT_PIDS" ]; then
  echo "Killing process(es) still on port $PORT: $PORT_PIDS"
  kill $PORT_PIDS 2>/dev/null || true
  sleep 1
  PORT_PIDS="$(lsof -ti tcp:"$PORT" -sTCP:LISTEN 2>/dev/null || true)"
  [ -n "$PORT_PIDS" ] && kill -9 $PORT_PIDS 2>/dev/null || true
fi

cd "$WEB_DIR"
nohup npm run dev > "$LOG_FILE" 2>&1 &
NEW_PID=$!
echo "$NEW_PID" > "$PID_FILE"
echo "Started dev server, pid $NEW_PID, logging to $LOG_FILE"

# Wait for it to come up.
for _ in $(seq 1 30); do
  if curl -sf "http://localhost:$PORT" >/dev/null 2>&1; then
    echo "Dev server is up at http://localhost:$PORT"
    exit 0
  fi
  sleep 1
done

echo "WARNING: dev server did not respond on port $PORT within 30s; check $LOG_FILE" >&2
exit 1
