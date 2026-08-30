#!/usr/bin/env python3
"""CI conformance gate (roadmap R17): run the full-surface harness and
enforce the approved contract — 0 crashes, the introspected tool count
matches `tool-test-lab/approved_surface.json`, and every unavailable
tool's machine-readable reason is an approved slug. Run on every push so
"135 tools, 0 crashes" stays a machine-checked invariant, not a doc
claim.

Usage: python tool-test-lab/conformance_gate.py <indexed-sandbox-root>

EXIT 0 = contract holds; EXIT 1 = a violation (the message names it, so
the red CI run is actionable rather than a mystery).
"""

from __future__ import annotations

import asyncio
import json
import subprocess
import sys
import tempfile
from pathlib import Path

LAB = Path(__file__).resolve().parent
REPO = LAB.parent
sys.path.insert(0, str(REPO))


def _current_tool_count() -> int:
    from cie.tools import ToolService

    return len([
        m for m in vars(ToolService)
        if not m.startswith("_") and m != "describe"
        and callable(vars(ToolService)[m])
    ])


async def _run_conformance(sandbox: Path) -> dict:
    """The tool-test-lab/surface_conformance.py suite, run in-process
    against `sandbox` (same code path CI exercises)."""
    from mcp import ClientSession, StdioServerParameters
    from mcp.client.stdio import stdio_client

    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "surface_conformance", LAB / "surface_conformance.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    out_path = Path(tempfile.mkstemp(suffix=".json")[1])
    sys.argv = ["surface_conformance.py", str(sandbox), str(out_path)]
    # the harness reads sys.argv in main; replicate its invocation
    params = StdioServerParameters(
        command=sys.executable,
        args=[
            str(LAB / "surface_conformance.py"), str(sandbox), str(out_path),
        ],
        env=None,
    )
    results: dict = {}
    proc = subprocess.run(
        [sys.executable, str(LAB / "surface_conformance.py"),
         str(sandbox), str(out_path)],
        cwd=sandbox,
        capture_output=True,
        text=True,
    )
    if not out_path.exists():
        raise RuntimeError(f"harness wrote no results: {proc.stdout[-400:]}")
    results = json.loads(out_path.read_text())
    out_path.unlink()
    return results


def main() -> int:
    approved = json.loads((LAB / "approved_surface.json").read_text())
    sandbox = REPO / "tests" / "fixtures" / "sandbox"

    live_count = _current_tool_count()
    violations: list[str] = []

    if live_count != approved["tool_count"]:
        violations.append(
            f"surface count changed: introspects {live_count}, approved "
            f"{approved['tool_count']} — update tool-test-lab/"
            "approved_surface.json ONLY if this PR's point was a surface "
            "change (see that file's _comment)"
        )

    harness = Path(tempfile.mkdtemp()) / "out.json"
    proc = subprocess.run(
        [
            sys.executable, str(LAB / "surface_conformance.py"),
            str(sandbox), str(harness),
        ],
        env={"PYTHONPATH": str(REPO), "PATH": "/usr/bin:/bin"},
        capture_output=True, text=True,
    )
    if not harness.exists():
        violations.append(f"harness produced no output: {proc.stdout[-400:]} {proc.stderr[-200:]}")
        print("\n".join(violations))
        return 1
    results = json.loads(harness.read_text())
    harness.unlink()

    summary = results.get("summary", {})
    crashes = summary.get("CRASH", 0)
    if approved.get("zero_crashes_required") and crashes:
        violations.append(f"{crashes} CRASH(es) on the live surface — fix before merging")

    if live_count != results.get("tool_count"):
        violations.append(
            f"served tools ({results.get('tool_count')}) != introspected "
            f"count ({live_count}) — the MCP surface and ToolService diverged"
        )

    unavailable = sorted(
        name for name, r in results.get("results", {}).items()
        if r["status"] == "unavailable-by-design"
    )
    if len(unavailable) > approved.get("max_unavailable_by_design", 6):
        violations.append(
            f"unavailable-by-design grew to {len(unavailable)} "
            f"(approved max {approved['max_unavailable_by_design']}): "
            f"{unavailable} — R5's gate"
        )

    for name in unavailable:
        reason = (results["results"][name].get("detail") or "").split(";")[0]
        slug = reason.replace("reason=", "")
        if slug not in approved.get("approved_unavailable_reasons", []) and unavailable:
            violations.append(
                f"{name} 503s with UNapproved reason slug {slug!r} — add it "
                "to approved_surface.json only after a human decided this "
                "tool cannot degrade gracefully (R5's registry rule)"
            )

    if violations:
        print("CONFORMANCE GATE FAILED:")
        for v in violations:
            print(f"  - {v}")
        return 1
    print(
        f"CONFORMANCE GATE: OK — {results['tool_count']} tools, "
        f"0 crashes, {len(unavailable)} unavailable (all reasons approved)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())