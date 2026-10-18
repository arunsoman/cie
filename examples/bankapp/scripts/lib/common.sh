#!/usr/bin/env bash
# common helpers for bankapp scripts (sourced by setup.sh / collect.sh)

log() {
  echo "[bankapp] $*"
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || { log "missing command: $1"; exit 1; }
}
