#!/usr/bin/env bash
# Push the cie repo's main branch to GitHub using a token supplied at runtime.
# The token is NEVER written into the repo — pass it as $1 or via GITHUB_TOKEN.
#
#   ./scripts/push-with-token.sh ghp_xxx
#   GITHUB_TOKEN=ghp_xxx ./scripts/push-with-token.sh
#
# Per the standing instruction: every stage/commit gets pushed the moment a
# valid token exists. This script pushes ALL local commits on main at once
# (currently: RFC-0024 stages 3–6 + the RFC-0023 merge commit).
set -euo pipefail
TOKEN="${1:-${GITHUB_TOKEN:-}}"
if [ -z "$TOKEN" ]; then
  echo "usage: $0 <github-token>   (or set GITHUB_TOKEN)" >&2
  exit 2
fi
cd "$(dirname "$0")/.."
GIT_TERMINAL_PROMPT=0 git push "https://arunsoman:${TOKEN}@github.com/arunsoman/cie.git" main
