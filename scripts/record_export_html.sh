#!/usr/bin/env bash
# record_export_html.sh — R8's record-then-commit pattern for the
# `cie export-html` artifact (the twin of record_demo.sh for the demo cast).
#
# Reproduces end to end, from THIS repo: clone psf/requests at the pinned
# commit, index it, export the self-contained HTML snapshot, then capture
# screenshots with a headless Chrome directly against file:// — proving
# the page needs no server and no network. Artifacts land in docs/images/
# (screenshots committed; the export itself is regenerable, keep OR keep
# both — a shared snapshot is exactly what the feature is for).
#
# Usage: scripts/record_export_html.sh [clone-dir]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CLONE="${1:-$(mktemp -d)}"
PIN_COMMIT="5460f467"   # matches docs/benchmarks-requests.md
OUT_HTML="requests-export.html"

# the script pins the REPO as the code source (the stale-site-packages
# trap the conformance harness also documents): use the repo venv's
# interpreter when present, else whatever python is on PATH — either way
# with PYTHONPATH pointed at the repo.
if [ -x "$REPO_ROOT/.venv/bin/python" ]; then
  PY="$REPO_ROOT/.venv/bin/python"
else
  PY="${PY:-python3}"
fi
export PYTHONPATH="$REPO_ROOT${PYTHONPATH:+:$PYTHONPATH}"

mkdir -p "$REPO_ROOT/docs/images"

if [ ! -d "$CLONE/.git" ]; then
  git clone --quiet https://github.com/psf/requests.git "$CLONE"
fi
cd "$CLONE"
git fetch --quiet origin "$PIN_COMMIT" 2>/dev/null || true
git checkout --quiet "$PIN_COMMIT" 2>/dev/null || true

echo "== cie index ."
"$PY" -m cie.cli index . > /dev/null
echo "== cie export-html . --out $OUT_HTML"
"$PY" -m cie.cli export-html . --out "$OUT_HTML"

echo "== zero-external-reference check (the file:// contract)"
if grep -Eq 'https?://|<script[[:space:]]+src|<link[[:space:]]' "$OUT_HTML"; then
  echo "FAIL: export carries external references" >&2
  exit 1
fi
echo "  none found."

echo "== headless Chrome screenshots, straight from file://"
CHROME="$(command -v google-chrome-stable || command -v chromium || command -v chromium-browser)"
"$CHROME" --headless=new --disable-gpu --no-sandbox --hide-scrollbars \
  --virtual-time-budget=5000 --window-size=1280,900 \
  --screenshot="$REPO_ROOT/docs/images/export-snapshot.png" \
  "file://$CLONE/$OUT_HTML"
"$CHROME" --headless=new --disable-gpu --no-sandbox --hide-scrollbars \
  --virtual-time-budget=5000 --window-size=1280,900 \
  --screenshot="$REPO_ROOT/docs/images/export-chains.png" \
  "file://$CLONE/$OUT_HTML#sec-chains"

echo "done — docs/images/export-{snapshot,chains}.png are the committed"
echo "artifacts of record; regenerate any time with this script."