#!/usr/bin/env python3
"""Take S3S4 — THE QUESTION (claude TUI, real session).

claude launched in the indexed cie clone, --allowedTools scoped to
cie's four tools (the -p rehearsal conditions that produced the
verified answer; in the TUI this also pre-approves them - no
permission prompts to choreograph). The question types at t=7;
the take ends on screen idle (answer complete) or max_wait.
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, "/tmp/prod")
from recorder import Recorder

REPO = "/tmp/demo-work/cie"
QUESTION = (
    "I want to change how resolve_backend picks a storage backend. "
    "What calls it, what breaks downstream, and which tests would "
    "catch a regression? Be specific with files and test names."
)

r = Recorder(Path("/tmp/prod/takes/S3S4"), cmd=[
    "claude",
    "--allowedTools",
    "mcp__cie__search_symbol,mcp__cie__callers,mcp__cie__affected_by,mcp__cie__test_map",
    "--disallowedTools",
    "Bash,Read,Edit,Write,Glob,Grep,WebFetch,WebSearch,NotebookEdit,Task,TodoWrite",
], cwd=REPO)
r.run(
    script=[
        {"at": 7.0, "type": QUESTION, "delay": 0.028, "force": True},
    ],
    idle_done=25.0,
    max_wait=420.0,
    watchers=[
        (r"Quick safety check|one you trust", "\x1b[B\r", "trust-dialog: Down+Enter (Yes)"),
        (r"don't ask again", "2\r", "tool-permission: allow-for-session"),
        (r"Do you want to proceed\?", "1\r", "proceed-prompt: 1. Yes"),
    ],
)
m = json.loads(Path("/tmp/prod/takes/S3S4/take.json").read_text())
print("duration:", m["duration_s"], "| frames:", m["frames"])
for e in m["events"][:12]:
    print(" ", e)