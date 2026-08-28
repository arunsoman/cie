#!/usr/bin/env bash
# Regenerates demo.cast / demo.svg (the README's animated terminal demo).
# Every command below is real, run for real, against a real public repo
# — nothing here is faked or hand-edited after recording (see
# docs/benchmarks-requests.md for the same target repo's full benchmark
# methodology).
#
# Requires: asciinema (`pip install asciinema`), svg-term-cli (`npx
# svg-term-cli` — no install needed), and a local clone of the target
# repo (default: psf/requests, cloned as a sibling of this checkout).
#
# Usage:
#   ./scripts/record_demo.sh [path-to-a-cloned-repo]
set -euo pipefail

CIE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${1:-$(dirname "$CIE_ROOT")/requests_bench}"

if [ ! -d "$TARGET" ]; then
    echo "Cloning psf/requests to $TARGET ..."
    git clone --depth 1 https://github.com/psf/requests.git "$TARGET"
fi

export PYTHONPATH="$CIE_ROOT"
cd "$TARGET"

type_() { printf '\033[1;32m$\033[0m %s\n' "$1"; sleep 0.4; }

clear
type_ "# psf/requests — 52k-star public Python repo, not this project's own code"
sleep 1.2
type_ "cie index ."
python3 -m cie.cli index .
sleep 1.6

type_ "# 'close()' is really defined 4x in this codebase (2 Adapters, Response, Session)"
type_ 'grep -rn "\.close()" src/requests/*.py'
grep -rn "\.close()" src/requests/*.py
sleep 1
echo "  -> 6 matches, and grep can't tell you which class's close() any of them call."
sleep 2.5

type_ "# Now the same question over a REAL MCP server, real stdio JSON-RPC"
sleep 1
python3 "$CIE_ROOT/scripts/record_demo_client.py" "$TARGET"
sleep 1.5

# To actually record this run into demo.cast/demo.svg (kept out of the
# recorded output itself, above — see this file's own header comment):
#   cd <cie repo root>
#   asciinema rec --command "scripts/record_demo.sh <target-repo>" \
#       --cols 100 --rows 40 --overwrite -q demo.cast
#   cat demo.cast | npx --yes svg-term-cli --out demo.svg --window --no-cursor
