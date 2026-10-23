#!/usr/bin/env bash
# BRIDGE bake-off: own-parse vs polyglot-fed ingest on real fixtures.
set -e
export PATH="$HOME/.cargo/bin:$PATH"
CIE=/home/z/my-project/target/debug/cie
ROOT=/tmp/cie-bake
rm -rf "$ROOT" && mkdir -p "$ROOT"

TIMEFORMAT='wall %R s'

echo "================ FIXTURE 1: tests/fixtures/polyglot (4 files, all polyglot-covered) ================"
cp -r /home/z/my-project/tests/fixtures/polyglot "$ROOT/fp-own"
cp -r /home/z/my-project/tests/fixtures/polyglot "$ROOT/fp-bridge"

echo "--- [own] cie ingest --facts-from own (first ingest, cold .cie) ---"
time "$CIE" ingest "$ROOT/fp-own" --facts-from own
echo
echo "--- [seed] cie polyglot fp-bridge (build + promote the store) ---"
time "$CIE" polyglot "$ROOT/fp-bridge" --json | head -3
echo
echo "--- [bridged] cie ingest fp-bridge (DEFAULT facts-from=polyglot, first ingest) ---"
time "$CIE" ingest "$ROOT/fp-bridge"
echo
echo "--- [bridged #2] re-ingest (fast path expected) ---"
"$CIE" ingest "$ROOT/fp-bridge"

echo
echo "================ FIXTURE 2: samples/polyglot (harbor, 5 services incl Kotlin) ================"
cp -r /home/z/my-project/samples/polyglot "$ROOT/hb-own"
cp -r /home/z/my-project/samples/polyglot "$ROOT/hb-bridge"

echo "--- [own] cie ingest --facts-from own ---"
time "$CIE" ingest "$ROOT/hb-own" --facts-from own
echo
echo "--- [seed] cie polyglot hb-bridge ---"
time "$CIE" polyglot "$ROOT/hb-bridge" --json | head -3
echo
echo "--- [bridged] cie ingest hb-bridge (DEFAULT) ---"
time "$CIE" ingest "$ROOT/hb-bridge"
echo
echo "--- [bridged #2] re-ingest ---"
"$CIE" ingest "$ROOT/hb-bridge"
