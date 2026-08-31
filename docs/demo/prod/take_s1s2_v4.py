#!/usr/bin/env python3
"""Take S1S2 v4 — DOGFOOD corpus (owner redirect 2026-08-31): cie clones
ITSELF at v0.1.4 (the release that ships the direct-calls TESTS fix),
indexes itself, registers, connects. Beats: clone (real GitHub, public
tag) -> checkout v0.1.4 -> cie index . (1,896 nodes / 6,553 edges, ~1.9s
real) -> claude mcp add one-liner -> claude mcp list (✔ Connected).
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, "/tmp/prod")
from recorder import Recorder

WORK = Path("/tmp/demo-work")
import shutil
shutil.rmtree(WORK, ignore_errors=True)
WORK.mkdir(parents=True)

r = Recorder(Path("/tmp/prod/takes/S1S2v4"), cmd=["/bin/bash"], cwd="/tmp/demo-work")
r.run(
    script=[
        {"at": 0.8, "type": "git clone https://github.com/kannamma-labs/cie",
         "delay": 0.025},
        {"at": 10.0, "type": "cd cie && git checkout -q v0.1.4", "delay": 0.025},
        {"at": 12.5, "type": "cie index .", "delay": 0.06},
        {"at": 20.0,
         "type": 'claude mcp add cie -- $(command -v cie-mcp) "$PWD" '
                 "--backend embedded --policy readonly",
         "delay": 0.02},
        {"at": 28.0, "type": "claude mcp list", "delay": 0.04},
    ],
    idle_done=8.0,
    max_wait=140.0,
    watchers=[
        (r"Quick safety check|one you trust", "\x1b[B\r", "trust-dialog: Down+Enter (Yes)"),
        (r"don't ask again", "2\r", "tool-permission: allow-for-session"),
    ],
)
m = json.loads(Path("/tmp/prod/takes/S1S2v4/take.json").read_text())
print("duration:", m["duration_s"], "| frames:", m["frames"])
for e in m["events"]:
    print(" ", e)