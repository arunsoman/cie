#!/usr/bin/env python3
"""R15 companion: the CLIENT VIEW of a registered cie entry — read
`.mcp.json`, spawn the registered command/args over real stdio MCP, list
tools. Used by scripts/record_init.sh (and handy for any client config
debug: `python dogfood_mcp_stdio_list.py <project-with-.mcp.json>`)."""

from __future__ import annotations

import asyncio
import json
import os
import sys
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


async def main() -> None:
    project = Path(sys.argv[1]).resolve()
    entry = json.loads((project / ".mcp.json").read_text())["mcpServers"]["cie"]
    import shutil

    if shutil.which(entry["command"]) is None:
        # PATH-less environments (like CI): same server via the module
        entry = {
            "command": sys.executable,
            "args": ["-m", "cie.mcp_server", *entry["args"]],
        }
    env = {**os.environ, "PYTHONPATH": os.environ.get("PYTHONPATH", "")}
    params = StdioServerParameters(
        command=entry["command"], args=entry["args"], env=env,
    )
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            tools = await session.list_tools()
            names = sorted(t.name for t in tools.tools)
            print(f"client sees {len(names)} tools: {', '.join(names[:8])} ...")
            writes = [n for n in names if n in {
                "write_file", "edit_file", "delete_file", "run", "push_tasks",
            }]
            assert not writes, f"write tools leaked into a readonly registration: {writes}"
            print("readonly policy honored server-side (no write tools listed)")


if __name__ == "__main__":
    asyncio.run(main())