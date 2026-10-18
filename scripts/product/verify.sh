#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.."
# Keep checks sequential: rebuilding shared dependencies while rustdoc is running
# can invalidate its metadata. Smaller debug artifacts also help laptop storage.
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
export CARGO_PROFILE_TEST_DEBUG="${CARGO_PROFILE_TEST_DEBUG:-0}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
cargo_bin="${CIE_CARGO:-cargo}"
mkdir -p target
if command -v flock >/dev/null 2>&1; then
  exec 9>target/product-verification.lock
  flock -n 9 || { echo 'Another product verification is running.' >&2; exit 2; }
  trap 'flock -u 9' EXIT
fi
"$cargo_bin" test --workspace --locked
"$cargo_bin" clippy -p cie-server --all-targets --no-deps --locked -- -D warnings
"$cargo_bin" build -p cie-server --bin cie-workspace --locked
node --test scripts/product/evidence.test.cjs
node scripts/product/browser.cjs
