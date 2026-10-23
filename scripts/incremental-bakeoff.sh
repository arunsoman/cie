#!/usr/bin/env bash
# INCREMENTAL-ON (backlog item 6) bake-off — cold vs warm vs --full rebuilds
# of the polyglot store, plus the parity proofs:
#   * the web lane's exact artifact (`--viewspec all`) must be byte-identical
#     between a cold build and a warm (reuse-tier) build;
#   * `--json` may differ ONLY in the additive files_reused/files_parsed/
#     files_dropped keys.
# Usage: scripts/incremental-bakeoff.sh <repo-path> [<repo-path> ...]
set -u
cd "$(dirname "$0")/.."
CIE=target/release/cie
if [ ! -x "$CIE" ]; then echo "build first: cargo build --release -p cie-cli" >&2; exit 1; fi

for repo in "$@"; do
  repo=$(cd "$repo" && pwd)
  echo "=============================================================="
  echo "target: $repo"
  # cold = no previous store; warm = reuse tier; full = --full (honest recompute)
  dbkey=$(python3 - <<PY
import ctypes
def fnv(s):
    h = ctypes.c_uint64(0xcbf29ce484222325)
    for b in s.encode():
        h = ctypes.c_uint64(h.value ^ b)
        h = ctypes.c_uint64(h.value * 0x100000001b3)
    return format(h.value, 'x')
print(fnv("$repo"))
PY
)
  db="${TMPDIR:-/tmp}/cie-polyglot-$dbkey.db"
  rm -f "$db" "$db-wal" "$db-shm"

  t() { local s=$(date +%s%N); "$@" >/tmp/bakeoff.out 2>/tmp/bakeoff.err; local rc=$?; echo "rc=$rc $(( ($(date +%s%N)-s)/1000000 ))ms"; [ $rc -ne 0 ] && sed -n '1,3p' /tmp/bakeoff.err; return $rc; }
  echo "--- cold build (no previous store) ---"
  tc=$(t "$CIE" polyglot "$repo"); echo "cold wall: $tc"; sed -n '1,2p' /tmp/bakeoff.out
  "$CIE" polyglot "$repo" --json >/tmp/bakeoff-cold.json 2>/dev/null

  echo "--- warm rebuild (reuse tier, unchanged tree) ---"
  tw=$(t "$CIE" polyglot "$repo"); echo "warm wall: $tw"; sed -n '1,2p' /tmp/bakeoff.out
  "$CIE" polyglot "$repo" --json >/tmp/bakeoff-warm.json 2>/dev/null

  echo "--- forced full rebuild (--full) ---"
  tf=$(t "$CIE" polyglot "$repo" --full); echo "full wall: $tf"; sed -n '1,2p' /tmp/bakeoff.out

  echo "--- warm again (steady state) ---"
  tw2=$(t "$CIE" polyglot "$repo"); echo "warm2 wall: $tw2"; sed -n '1,2p' /tmp/bakeoff.out

  # parity proof 1: the web lane's artifact — cold build vs warm build
  rm -f "$db" "$db-wal" "$db-shm"
  "$CIE" polyglot "$repo" --viewspec all >/tmp/bakeoff-viewspec-cold.json 2>/dev/null
  "$CIE" polyglot "$repo" --viewspec all >/tmp/bakeoff-viewspec-warm.json 2>/dev/null
  if cmp -s /tmp/bakeoff-viewspec-cold.json /tmp/bakeoff-viewspec-warm.json; then
    echo "VIEWSPEC PARITY: byte-identical (cold build == warm build)"
  else
    echo "VIEWSPEC PARITY: DIFFERS — investigate"
  fi

  # parity proof 2: --json differs only in the additive reuse keys
  python3 - <<PY
import json
c = json.load(open('/tmp/bakeoff-cold.json'))
w = json.load(open('/tmp/bakeoff-warm.json'))
ci, wi = c.pop('ingest'), w.pop('ingest')
same = c == w and all(ci[k] == wi[k] for k in ci if k not in ('files_reused','files_parsed','files_dropped','files'))
print(f"JSON PARITY: {'all non-reuse fields identical' if same and ci['files']==wi['files'] else 'DIFFERS'} "
      f"(cold files={ci['files']}, warm files={wi['files']}, reused={wi.get('files_reused')}, parsed={wi.get('files_parsed')}, dropped={wi.get('files_dropped')})")
PY
done
