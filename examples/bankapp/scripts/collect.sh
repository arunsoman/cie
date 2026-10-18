#!/usr/bin/env bash
# collect all runtime evidence for the bankapp sample project.
# Writes runtime-data/*.json + the merged runtime.json the dashboard loads,
# then measures/records all 33 remaining stories into stories.json.
set -euo pipefail
source "$(dirname "$0")/lib/common.sh"
require_cmd python3
log "collecting runtime evidence (cpu, memory, concurrency, deadlock, race, coverage)"
python3 "$(dirname "$0")/collect_runtime.py"
log "collecting the 33 story demos (measured + recorded, nothing invented)"
python3 "$(dirname "$0")/collect_stories.py"
log "done — see runtime-data/runtime.json and runtime-data/stories.json"
