"""Dogfood client: drive a REAL cie-mcp --embedded server over real MCP
stdio with the actual mcp SDK, against cie's own indexed repo. Counts the
live tool surface (README claims ~126, the doc says 81 — verify the real
number), then exercises a spread of tools and quantifies the .venv
contamination found by hand. Every value printed is live server output.

Usage: python3 dogfood_mcp.py <project_root>
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
import sys

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

logging.disable(logging.CRITICAL)

PROJ = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else ".")
POLICY = sys.argv[2] if len(sys.argv) > 2 else "inspector"


def text(result) -> str:
    for block in result.content:
        if hasattr(block, "text"):
            return block.text
    return ""


async def main() -> None:
    cmd = (
        f"cd {PROJ} && {sys.executable} -m cie.mcp_server {PROJ} "
        f"--embedded{" --policy " + POLICY if POLICY != "full" else ""} 2>/dev/null"
    )
    params = StdioServerParameters(command="bash", args=["-c", cmd], env=dict(os.environ))
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            tools = await session.list_tools()
            names = sorted(t.name for t in tools.tools)
            print(f"LIVE TOOL SURFACE: {len(names)} tools served (policy={POLICY})")
            for key in ("search_symbol", "callers", "callees", "file_skeleton",
                        "path_between", "traceability_orphans", "tasks",
                        "plan_push", "record_qa"):
                hit = next((n for n in names if key in n), None)
                print(f"  {key:<22} -> {hit or 'ABSENT'}")

            async def call(name: str, args: dict) -> dict | None:
                r = await session.call_tool(name, args)
                raw = text(r)
                try:
                    return json.loads(raw)
                except Exception:
                    print(f"\n[{name}] non-JSON -> {raw[:220]}")
                    return None

            # 1) noise metric: how much of the graph is .venv junk?
            r1 = await call("search_symbol", {"name": "load"})
            if r1 is not None:
                hits = r1.get("results", r1 if isinstance(r1, list) else [])
                if isinstance(hits, dict):
                    hits = hits.get("symbols", hits.get("matches", []))
                total = len(hits) if isinstance(hits, list) else -1
                venv = sum(1 for h in hits
                           if isinstance(h, dict) and ".venv/" in str(h.get("file", h.get("path", ""))))
                print(f"\nsearch_symbol('load'): {total} hits, {venv} from .venv "
                      f"({100 * venv / max(total, 1):.0f}% noise)")

            # 2) self-referential blast radius
            r2 = await call("callers", {"symbol": "extract_many"})
            if r2 is not None:
                res = r2.get("results", [])
                print(f"\ncallers('extract_many'): {r2.get('total', len(res))} call sites")
                for c in res[:6]:
                    print(f"  {c.get('caller_signature','?'):<48} {c.get('caller_file','?')}")

            # 3) skeleton on the file about to be fixed
            r3 = await call("file_skeleton", {"path": f"{PROJ}/cie/extract.py"})
            if r3 is not None:
                if r3.get("ok") and r3.get("data"):
                    s = json.dumps(r3["data"])
                    print(f"\nfile_skeleton(cie/extract.py): envelope ok, "
                          f"{len(s)} bytes of data")
                else:
                    print(f"\nfile_skeleton: {json.dumps(r3)[:200]}")

            # 4) a read-only task/traceability view (headline feature)
            for tname in names:
                if "orphan" in tname or "coverage" in tname:
                    r4 = await call(tname, {})
                    if r4 is not None:
                        s = json.dumps(r4)
                        print(f"\n{tname}(): {len(s)} bytes -> {s[:180]}")
                    break

            print(f"\nDONE (server policy={POLICY}, {len(names)} tools)")


if __name__ == "__main__":
    asyncio.run(main())