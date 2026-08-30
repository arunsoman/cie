"""Standard response envelope for every cie tool surface.

This module is the ONLY place the envelope shape is defined. The CLI
(``--json`` mode) and the FastAPI layer both import these helpers so the
JSON contract from the integration spec (§0) can never drift between
surfaces:

    {"ok": true, "tool": "search_symbol", "results": [], "truncated": false,
     "total": 0, "hint": null, "elapsed_ms": 12}

Errors:

    {"ok": false, "tool": ..., "error": {"kind": "not_found|validation|internal",
     "message": ...}, "hint": "what to try", "elapsed_ms": N}

Hints are MANDATORY on empty results and errors (integration spec §0.3):
an agent must always be told what to try next. The canonical hint strings
live here as constants so every surface words them identically.
"""

from __future__ import annotations

import time
from typing import Any, Optional, Union

# -- canonical hints (integration spec §0.3) --------------------------------

#: Empty callers/callees result — the call graph is best-effort, so an empty
#: answer does NOT prove a symbol is unused.
HINT_EMPTY_CALLERS = (
    "call-graph coverage is partial until pass-2 inference runs; empty != unused"
)

#: Empty symbol search — template; format with the searched name.
HINT_EMPTY_SEARCH = "no symbol named '{name}'; try a substring or drop the kind filter"

#: Empty failing_context result — the test node itself was not found.
HINT_EMPTY_FAILING_CONTEXT = "test node not indexed; load/reindex the test file first"

#: Error kinds allowed in the error envelope.
#:
#: "unavailable" = the tool itself is fine but an optional backend module
#: (e.g. the protobox `core.llm` layer absent from the standalone package)
#: is not importable. Distinct from "internal": it is an expected, permanent
#: property of this installation, not a crash — the fix is installing the
#: optional dependency, not reporting a bug.
ERROR_KINDS = ("not_found", "validation", "unavailable", "forbidden", "internal")


def envelope(
    tool: str,
    results: Union[list, dict],
    *,
    truncated: bool = False,
    total: Optional[int] = None,
    hint: Optional[str] = None,
    elapsed_ms: int = 0,
) -> dict:
    """Build the standard success envelope for a tool response.

    Args:
        tool: Tool name as the agent invoked it (e.g. ``"search_symbol"``).
        results: Payload — a list of result dicts, or a single dict for
            tools that return one object.
        truncated: True when the payload was cut by a response bound.
        total: Total number of available results before truncation. Defaults
            to ``len(results)`` for list payloads; left None for dict payloads.
        hint: What to try next. MANDATORY when results is empty.
        elapsed_ms: Wall time of the tool call in milliseconds.

    Returns:
        The envelope dict, JSON-serializable as-is.
    """
    if total is None and isinstance(results, (list, tuple)):
        total = len(results)
    return {
        "ok": True,
        "tool": tool,
        "results": results,
        "truncated": truncated,
        "total": total,
        "hint": hint,
        "elapsed_ms": elapsed_ms,
    }


def err_envelope(
    tool: str,
    kind: str,
    message: str,
    *,
    hint: Optional[str] = None,
    elapsed_ms: int = 0,
) -> dict:
    """Build the standard error envelope for a failed tool call.

    Args:
        tool: Tool name as the agent invoked it.
        kind: One of ``"not_found"``, ``"validation"``, ``"forbidden"``,
        ``"unavailable"``, ``"internal"``.
        message: Human/agent-readable description of what went wrong.
        hint: What to try next. MANDATORY — failures must be self-describing.
        elapsed_ms: Wall time until the failure in milliseconds.

    Returns:
        The error envelope dict, JSON-serializable as-is.
    """
    if kind not in ERROR_KINDS:
        kind = "internal"
    return {
        "ok": False,
        "tool": tool,
        "error": {"kind": kind, "message": message},
        "hint": hint,
        "elapsed_ms": elapsed_ms,
    }


class ToolTimer:
    """Context manager measuring tool wall time in whole milliseconds.

    Example:
        >>> with ToolTimer() as timer:
        ...     results = engine.search_symbols("parse_csv")
        >>> payload = envelope("search_symbol", results,
        ...                    elapsed_ms=timer.elapsed_ms)

    Attributes:
        elapsed_ms: Milliseconds since ``__enter__``; frozen at ``__exit__``.
    """

    def __init__(self) -> None:
        self._start: float = 0.0
        self.elapsed_ms: int = 0

    def __enter__(self) -> "ToolTimer":
        self._start = time.perf_counter()
        self.elapsed_ms = 0
        return self

    def __exit__(self, *exc: Any) -> None:
        self.elapsed_ms = int(round((time.perf_counter() - self._start) * 1000))
