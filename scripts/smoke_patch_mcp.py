"""One REAL MCP round-trip of the repair loop: propose → apply → verify.

Proves the repair transaction layer is reachable the exact way an MCP host
(Claude Code / Cursor / forge's MCPBridge) reaches it — over stdio, through
`cie-mcp --embedded`, with the JSON envelopes parsed from the protocol's
content blocks. Run: `python scripts/smoke_patch_mcp.py`.
"""
from __future__ import annotations

import asyncio
import json
import sys
import tempfile
from pathlib import Path

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


async def main() -> int:
    project = Path(tempfile.mkdtemp())
    (project / "a.py").write_text("def f():\n    x = 1\n    return x\n")

    params = StdioServerParameters(
        command=sys.executable,
        args=["-m", "cie.mcp_server", str(project), "--embedded"],
    )
    async with stdio_client(params) as (reader, writer):
        async with ClientSession(reader, writer) as session:
            await session.initialize()

            propose = await session.call_tool("propose_patch", {
                "changes": [{
                    "file": "a.py",
                    "old_text": "    x = 1\n",
                    "new_text": "    x = 2\n",
                }],
                "test_id": "test_f",
            })
            plan = json.loads(propose.content[0].text)
            assert plan["ok"], plan
            patch_id = plan["results"][0]["patch_id"]
            print("propose →  ok, patch:", patch_id)

            applied = await session.call_tool("apply_patch",
                                              {"patch_id": patch_id})
            applied_env = json.loads(applied.content[0].text)
            assert applied_env["ok"], applied_env
            assert "    x = 2\n" in (project / "a.py").read_text()
            assert "    x = 1\n" not in (project / "a.py").read_text()
            print("apply   →  ok, file mutated through the gate pipeline")

            verified = await session.call_tool("verify_patch",
                                               {"patch_id": patch_id})
            verified_env = json.loads(verified.content[0].text)
            assert verified_env["ok"], verified_env
            verdict = verified_env["results"][0]["status"]
            assert verdict == "VERIFIED", verified_env
            print("verify   →  verdict:", verdict)

            listed = await session.call_tool("list_patches", {})
            statuses = {p["patch_id"]: p["status"]
                        for p in json.loads(listed.content[0].text)["results"]}
            assert statuses[patch_id] == "VERIFIED"
            print("list_patches →  VERIFIED: the whole protocol over real MCP")
    return 0


if __name__ == "__main__":
    raise SystemExit(asyncio.run(main()))