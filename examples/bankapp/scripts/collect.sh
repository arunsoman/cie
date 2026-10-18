#!/usr/bin/env bash
# collect all runtime evidence for the bankapp sample project.
# Writes runtime-data/*.json + the merged runtime.json the dashboard loads.
set -euo pipefail
source "$(dirname "$0")/lib/common.sh"
require_cmd python3
log "collecting runtime evidence (cpu, memory, concurrency, deadlock, race, coverage)"
python3 "$(dirname "$0")/collect_runtime.py"
log "done — see runtime-data/runtime.json"
