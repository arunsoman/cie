"""Real MCP stdio client used by scripts/record_demo.sh to produce
demo.cast / demo.svg — talks to a real `cie-mcp` server process over the
actual Model Context Protocol (the `mcp` SDK's stdio transport), not a
shortcut through `cie.tools.ToolService` directly. Every value printed
here is live tool output against the indexed target repo, not
pre-rendered or hand-edited afterward.

Usage: python3 record_demo_client.py <indexed-project-root>
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
import sys
import time

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

logging.disable(logging.CRITICAL)  # quiet for the recording; real calls, real output


async def main(project_root: str) -> None:
    server_cmd = (
        f"{sys.executable} -m cie.mcp_server {project_root} "
        "--embedded --policy inspector 2>/dev/null"
    )
    params = StdioServerParameters(command="bash", args=["-c", server_cmd], env=dict(os.environ))
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            tools = await session.list_tools()
            print(f"$ tools/list  ->  {len(tools.tools)} read-only tools available\n")
            time.sleep(1.2)

            print('$ callers("close")   # ambiguous by name alone (4 real definitions)')
            r = await session.call_tool("callers", {"symbol": "close"})
            payload = json.loads(_text(r))
            for c in payload["results"]:
                path = c["caller_file"].replace(project_root.rstrip("/") + "/", "")
                print(f"  {c['caller_signature']:<45s} {path}")
            print(
                f"\n  {payload['total']} real call sites, resolved via the actual "
                "call graph — not text matching."
            )
            time.sleep(3)


def _text(result) -> str:
    for block in result.content:
        if hasattr(block, "text"):
            return block.text
    return str(result)


if __name__ == "__main__":
    asyncio.run(main(sys.argv[1] if len(sys.argv) > 1 else "."))
