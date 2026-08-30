#!/usr/bin/env bash
# benchmark.sh — R9: the reproducible runner for docs/benchmarks-*.md.
#
# Reproduces a benchmark dataset end to end: clone the repo at the
# pinned commit, `cie index`, run scripts/benchmark_tasks.py (the three
# canonical task shapes, naive AND cie sides, with the token-per-query
# chars metric), and emit the JSON the doc's tables are pasted from.
#
# Usage: scripts/benchmark.sh <git-url> <pin-commit> <db-subdir|''> \
#          <src-glob> <class-name> <ambiguous-name> <big-file> [out.json]
#
# Example (the urllib3 dataset, docs/benchmarks-urllib3.md):
#   scripts/benchmark.sh https://github.com/urllib3/urllib3 85a8a9cf src \
#     '/clone/src/urllib3/*.py' PoolManager close \
#     src/urllib3/connectionpool.py
set -euo pipefail

URL="$1"; COMMIT="$2"; SUBDIR="${3:-}"; GLOB="$4"; CLASS="$5"; AMB="$6"; BIG="$7"
OUT="${8:-bench.json}"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [ -x "$REPO_ROOT/.venv/bin/python" ]; then PY="$REPO_ROOT/.venv/bin/python"; else PY="${PY:-python3}"; fi
export PYTHONPATH="$REPO_ROOT${PYTHONPATH:+:$PYTHONPATH}"

CLONE="$(mktemp -d)/repo"
git clone --quiet "$URL" "$CLONE"
cd "$CLONE"
git checkout --quiet "$COMMIT"

DB="$PWD/.cie/graph.db"
[ -n "$SUBDIR" ] && DB="$PWD/$SUBDIR/.cie/graph.db"

echo "== cie index $SUBDIR"
if [ -n "$SUBDIR" ]; then "$PY" -m cie.cli index "$SUBDIR"; else "$PY" -m cie.cli index .; fi

echo "== benchmark_tasks.py"
"$PY" "$REPO_ROOT/scripts/benchmark_tasks.py" "$PWD" \
  --db "$DB" --src-glob "${GLOB/'<clone>'/$PWD}" --class-name "$CLASS" \
  --ambiguous-name "$AMB" --big-file "$BIG" --out "$OUT"

echo "JSON -> $OUT; paste into the dataset doc with the run date"
echo "+ commit hash as methodology (the doc is regenerated, never tuned)."