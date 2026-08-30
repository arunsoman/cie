"""Full-surface conformance harness for cie-mcp.

Enumerates EVERY tool the live server exposes under the `forge` policy
(the full surface), attempts a real call against the sandbox repo, and
retries validation failures by parsing the server's own pydantic
"Field required" hints (which param names are missing). Every outcome is
classified:

  verified            ok=true — real happy-path result from the tool
  graceful            ok=false with kind=validation/not_found — correct
                      error contract (arg guesses were wrong, tool behaved)
  unavailable         kind=unavailable — optional backend absent BY DESIGN
  backend-down        kind=internal but message is connection-refused /
                      backend connectivity (environment, not a tool bug)
  CRASH               anything else (kind=internal w/ unexpected error,
                      non-JSON, MCP error, timeout) — must be fixed

Usage: python3 surface_conformance.py <sandbox_root> [out.json]
"""

from __future__ import annotations

import asyncio
import json
import os
import re
import sys

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

SANDBOX = os.path.abspath(sys.argv[1])
OUT = sys.argv[2] if len(sys.argv) > 2 else None
BACKEND_DOWN = re.compile(
    r"connection refused|failed to establish|Neo4j|neo4j|bolt://|"
    r"ServiceUnavailable|SessionExpired|couldn't connect|Couldn't connect",
    re.I,
)


def arg_value(name: str, schema: dict):
    t = (schema or {}).get("type")
    if "enum" in (schema or {}):
        return (schema or {})["enum"][0]
    if t in ("integer", "number"):
        return 1
    if t == "boolean":
        return False
    if t == "array":
        return []
    if t == "object":
        return {}
    if t == "null":
        return None
    n = name.lower()
    if any(k in n for k in ("path", "file", "target", "source", "root")):
        return "app.py"
    if n in ("cmd", "command"):
        return "echo ok"
    if n in ("old_text", "new_text", "text", "content", "body", "prd"):
        return "def alpha():\n    return 1\n"
    if n in ("project",):
        return "test-project"
    if n in ("status",):
        return "passed"
    return "alpha"


def build_args(schema: dict, extra_fields: list[str] | None = None) -> dict:
    args: dict = {}
    props = schema.get("properties", {}) or {}
    required = schema.get("required", []) or []
    if extra_fields:
        required = list(dict.fromkeys(list(required) + [
            f for f in extra_fields if f in props]))
    for name in required:
        args[name] = arg_value(name, props.get(name, {}))
    return args


def classify(raw: str, is_error: bool) -> tuple[str, str]:
    if is_error:
        return "CRASH", raw[:160]
    try:
        env = json.loads(raw)
    except Exception:
        return "CRASH", f"non-JSON: {raw[:90]}"
    if env.get("ok") is True:
        return "verified", ""
    err = env.get("error") or {}
    kind = env.get("error", {}).get("kind", "?" if not env.get("ok") else "?")
    reason = err.get("reason")  # machine-readable slug (R5), when present
    msg = str(err.get("message", ""))[:110]
    hint = str(env.get("hint", ""))[:60]
    if kind == "unavailable":
        return "unavailable-by-design", (f"reason={reason}; " if reason else "") + msg
    if kind in ("validation", "not_found"):
        return "graceful", f"{kind}: {msg}"
    if kind == "internal":
        if BACKEND_DOWN.search(msg):
            return "backend-down", msg
        return "CRASH", msg
    return "CRASH", f"kind={kind}: {msg or hint}"


def missing_fields(raw: str) -> list[str]:
    return re.findall(r"(\w+)\n\s+Field required", raw)


async def main() -> None:
    cmd = (
        f"cd {SANDBOX} && {sys.executable} -m cie.mcp_server {SANDBOX} "
        "--embedded 2>/dev/null"
    )
    # IMPORTANT: pin the REPO as the code source. From a sandbox cwd, `import cie`
    # resolves to the stale site-packages copy (0.1.0a2), not the repo being
    # tested — found the hard way when fixed tools still crashed.
    env = dict(os.environ, PYTHONPATH=os.environ.get("CIE_REPO_ROOT", "/home/arun/Downloads/cie"))
    params = StdioServerParameters(command="bash", args=["-c", cmd], env=env)
    results: dict[str, dict] = {}
    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            tools = await session.list_tools()
            names = sorted(t.name for t in tools.tools)
            schemas = {t.name: (t.input_schema if hasattr(t, "input_schema") else t.inputSchema) for t in tools.tools}
            print(f"enumerated {len(names)} tools on the live surface — exercising each\n")

            for name in names:
                args = build_args(schemas.get(name, {}))
                raw = ""
                status, detail, attempts = "CRASH", "", 0
                for attempt in range(4):
                    attempts = attempt + 1
                    try:
                        r = await asyncio.wait_for(
                            session.call_tool(name, args), timeout=25
                        )
                        raw = ""
                        is_err = getattr(r, "is_error", getattr(r, "isError", False))
                        for block in r.content:
                            if hasattr(block, "text"):
                                raw += block.text
                        status, detail = classify(raw, is_err)
                    except Exception as exc:  # noqa: BLE001
                        status, detail = "CRASH", f"{type(exc).__name__}: {exc}"[:110]
                    if status in ("verified", "unavailable-by-design"):
                        break
                    if "Field required" in raw or "Field required" in detail:
                        fields = missing_fields(raw) or missing_fields(detail)
                        if fields and attempt < 3:
                            props = schemas.get(name, {}).get("properties", {})
                            args.update({f: arg_value(f, props.get(f)) for f in fields})
                            continue
                    break
                results[name] = {
                    "status": status, "detail": detail, "attempts": attempts, "args": args,
                }

    from collections import Counter
    counts = Counter(r["status"] for r in results.values())
    print(json.dumps({k: v for k, v in counts.items()}, indent=2))
    print()
    for bucket in ("CRASH", "backend-down", "unavailable-by-design"):
        group = [(n, r["detail"]) for n, r in sorted(results.items()) if r["status"] == bucket]
        if group:
            print(f"--- {bucket} ({len(group)}) ---")
            for n, d in group:
                print(f"  {n:<32} {d}")
    if OUT:
        with open(OUT, "w") as fh:
            json.dump({"tool_count": len(names), "summary": dict(counts),
                       "results": results}, fh, indent=2)
        print(f"\nfull JSON -> {OUT}")


if __name__ == "__main__":
    asyncio.run(main())