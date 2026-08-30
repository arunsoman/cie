"""R11 verify harness: drive a real `cie-mcp --embedded
--transport streamable-http` server over the REAL streamable-HTTP MCP
transport (the official SDK client), and pin the two contracts the
roadmap named:

1. `tools/list` over HTTP == EXACTLY the schema set the inspector policy
   predicts (server-side filtering — a read-only client never even sees
   a write tool, same contract as stdio);
2. a write attempt is REFUSED SERVER-SIDE (under readonly the write
   tools are not registered at all, so the call errors at the server —
   never silently honored, never trusted to the client).

CLI equivalent for a human: `npx @modelcontextprotocol/inspector` →
connect to http://127.0.0.1:<port>/mcp (browser-mode Inspector); this
script is the scriptable, CI-runnable version of that check.

Usage: python3 dogfood_mcp_http.py [project_root] [port]
"""

from __future__ import annotations

import asyncio
import json
import os
import subprocess
import sys
import time

import httpx

from mcp import ClientSession
from mcp.client.streamable_http import streamable_http_client

from cie.tool_policy import INSPECTOR_POLICY
from cie.tools import ToolService

PROJ = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else ".")
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 8731
URL = f"http://127.0.0.1:{PORT}/mcp"


def _predicted_tools() -> set[str]:
    """What INSPECTOR policy permits — the exact registration gate
    `build_mcp_server` applies on every transport."""
    return {
        name
        for name in vars(ToolService)
        if not name.startswith("_") and name != "describe"
        and callable(vars(ToolService)[name])
        and INSPECTOR_POLICY.permits(name)
    }


async def http_ready(timeout: float = 20.0) -> None:
    deadline = time.monotonic() + timeout
    async with httpx.AsyncClient() as client:
        while time.monotonic() < deadline:
            try:
                await client.post(
                    URL, json={"jsonrpc": "2.0", "id": 0, "method": "ping"},
                    timeout=2.0,
                )
                return
            except Exception:
                await asyncio.sleep(0.25)
    raise RuntimeError(f"server at {URL} never accepted a request")


async def main() -> None:
    expected = _predicted_tools()
    proc = subprocess.Popen(
        [
            sys.executable, "-m", "cie.mcp_server", PROJ, "--embedded",
            "--policy", "readonly", "--transport", "streamable-http",
            "--host", "127.0.0.1", "--port", str(PORT),
        ],
        cwd=PROJ,
        env={**os.environ, "PYTHONPATH": os.environ.get("PYTHONPATH", "")},
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        await http_ready()
        async with streamable_http_client(URL, terminate_on_close=False) as (read, write):
            async with ClientSession(read, write) as session:
                await session.initialize()
                tools = await session.list_tools()
                served = {t.name for t in tools.tools}
                print(f"HTTP tools/list: {len(served)} tools "
                      f"(inspector prediction: {len(expected)})")
                missing = expected - served
                extra = served - expected
                assert not extra, f"HTTP surface served tools the policy DENIES: {sorted(extra)}"
                assert not missing, f"HTTP surface missing predicted tools: {sorted(missing)}"
                print("  tools/list == inspector prediction: OK")
                print("          (no write tool is even registered, per row)")

                # write attempt: server-side refusal (tool never registered
                # under this policy -> MCP error, not a successful write)
                result = await session.call_tool(
                    "edit_file",
                    {"path": "x.py", "old_text": "a", "new_text": "b"},
                )
                raw = "".join(b.text for b in result.content if hasattr(b, "text"))
                is_err = getattr(result, "is_error", getattr(result, "isError", False))
                assert is_err, f"write call unexpectedly succeeded: {raw[:200]}"
                print(f"  write refused server-side: OK ({raw[:90]}...)")
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
    print("R11 verify: PASS (streamable-http, server-enforced policy)")


if __name__ == "__main__":
    asyncio.run(main())