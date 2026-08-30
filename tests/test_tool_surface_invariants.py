"""Tool-surface invariants — codifies the boundaries between cie's three
front-ends so they can't silently drift.

The history this exists to prevent: cie's HTTP layer (`cie/routes.py`)
once carried 17 task/hierarchy/coverage "tools" implemented as bespoke
`_tool_*` route handlers that were NOT `ToolService` methods — invisible
to `ToolService.describe()`, to `cie.tool_policy`, and to the MCP server
(`cie.mcp_server.build_mcp_server` only introspects `ToolService`). That
made the HTTP, MCP, and CLI surfaces diverge with no test catching it
(see `docs/language-agnostic-design.md` §6/§7). These invariants pin the
relationships instead of any one count, so they hold as tools are added
or (as just happened) the be-v2-hardwired `resolve_api_route`/
`api_call_sites` user tools are cut down to adapters.

No Neo4j, no MCP SDK, no live `ToolService` instance required — these
introspect the class and the route table directly.
"""

from __future__ import annotations

import inspect

import pytest

from cie.tool_policy import (
    FORGE_POLICY,
    INSPECTOR_POLICY,
    WRITE_TOOLS,
    ToolPolicy,
)


# ---------------------------------------------------------------------------
# Source-of-truth sets, computed from the code (never hardcoded counts)
# ---------------------------------------------------------------------------


def _public_tool_service_methods() -> set[str]:
    """Every public ToolService method name, exactly as
    `ToolService.describe()` and `build_mcp_server` enumerate them:
    callables on the class, excluding `_`-prefixed and `describe`."""
    from cie.tools import ToolService

    return {
        name
        for name, attr in vars(ToolService).items()
        if not name.startswith("_")
        and name != "describe"
        and callable(attr)
    }


def _route_tool_keys() -> set[str]:
    """Every key in `cie.routes.TOOLS` (the `POST /tools/{tool}` surface)."""
    from cie.routes import TOOLS

    return set(TOOLS.keys())


#: HTTP-only tools that are NOT ToolService methods — the explicit,
#: hand-maintained task/hierarchy/coverage read-side + validation +
#: health/schema helpers in `cie/routes.py`'s `_tool_*` functions. These
#: are the ONLY tools allowed to exist on the HTTP surface without a
#: matching ToolService method. Keep this set in sync with `cie/routes.py`;
#: if you add a new `_tool_*` helper, add its key here or (preferred) make
#: it a real ToolService method so MCP/CLI get it for free.
#: Roadmap R1 shrank this set by six: the task/QA write-back tools
#: (push_tasks, set_task_status, link_artifact, append_repair_events,
#: record_coverage, record_coverage_snapshot) ARE ToolService methods now,
#: and MUST stay out of this set (they're WRITE_TOOLS members; keeping a
#: stale helper entry would silently exempt them from the write gate).
HTTP_ONLY_HELPERS: frozenset[str] = frozenset({
    "coverage_report",
    "coverage_trend",
    "get_coverage",
    "health",
    "schema_version",
    "validate_api_contracts",
    "validate_coverage",
    "validate_cycles",
})


# ---------------------------------------------------------------------------
# ToolService <-> HTTP parity
# ---------------------------------------------------------------------------


def test_every_tool_service_method_is_exposed_over_http():
    """A method on ToolService that no HTTP route mounts is a tool an agent
    can call over MCP/CLI but not over `POST /tools/{tool}` — a silent
    surface split. Every public ToolService method must have a route key."""
    missing = _public_tool_service_methods() - _route_tool_keys()
    assert not missing, (
        f"ToolService methods with no HTTP /tools route (add them to "
        f"cie.routes.TOOLS): {sorted(missing)}"
    )


def test_http_only_tools_are_exactly_the_documented_helpers():
    """The ONLY HTTP tools that aren't ToolService methods are the explicit
    helper set above. A surprise entry here means someone added a `_tool_*`
    route handler without (a) making it a ToolService method or (b) listing
    it in HTTP_ONLY_HELPERS — i.e. a tool the MCP server and CLI can't see."""
    http_only = _route_tool_keys() - _public_tool_service_methods()
    extra = http_only - HTTP_ONLY_HELPERS
    missing = HTTP_ONLY_HELPERS - http_only
    assert not extra, (
        f"HTTP tools that are neither ToolService methods nor documented "
        f"helpers (make them ToolService methods, or add to "
        f"HTTP_ONLY_HELPERS): {sorted(extra)}"
    )
    assert not missing, (
        f"HTTP_ONLY_HELPERS lists tools not actually in cie.routes.TOOLS "
        f"(stale helper entry): {sorted(missing)}"
    )


def test_http_write_aliases_and_write_tools_never_overlap():
    """The R1 promotion trap, pinned permanently.

    When a task/QA alias handler is promoted into a ToolService method,
    routes' `/tools/{tool}` dispatcher reclassifies it: authorization for
    that name moves from the HTTP_WRITE_ALIASES branch (allow_write flag)
    to WRITE_TOOLS. If a promoted name were left in the alias set, the
    alias branch would shadow the write gate; if it were dropped from
    WRITE_TOOLS at promotion, the read-only-by-default HTTP policy would
    silently PERMIT a write. Both directions are pinned here: the alias
    set must stay free of ToolService names, and the six tools promoted
    in R1 must remain in WRITE_TOOLS."""
    from cie.routes import HTTP_WRITE_ALIASES

    surface = _public_tool_service_methods()
    overlap = HTTP_WRITE_ALIASES & surface & WRITE_TOOLS
    assert not overlap, (
        f"{sorted(overlap)} are both ToolService methods and HTTP write "
        "aliases — dispatched with two different authorization semantics; "
        "remove them from HTTP_WRITE_ALIASES (routes.py) once they are "
        "WRITE_TOOLS members"
    )
    promoted_r1 = {
        "push_tasks", "set_task_status", "link_artifact",
        "append_repair_events", "record_coverage", "record_coverage_snapshot",
    }
    lost = promoted_r1 - WRITE_TOOLS - HTTP_WRITE_ALIASES
    assert not lost, (
        f"{sorted(lost)} left the alias set but are missing from WRITE_TOOLS "
        "— on the read-only-by-default HTTP surface they would now be "
        "callable as WRITES; add them back to WRITE_TOOLS"
    )
    for name in promoted_r1:
        assert name in surface, f"{name} must remain a public ToolService method"


# ---------------------------------------------------------------------------
# ToolService <-> MCP parity (policy-gated; no MCP SDK needed)
# ---------------------------------------------------------------------------


def _mcp_exposed(policy: ToolPolicy) -> set[str]:
    """What `build_mcp_server` would register under `policy`, computed via
    the same `policy.permits` gate the MCP server uses — without needing
    the optional `mcp` SDK installed."""
    return {m for m in _public_tool_service_methods() if policy.permits(m)}


def test_full_policy_exposes_every_tool_service_method():
    """The default MCP policy (full / forge) exposes every public
    ToolService method — no tool is silently unreachable over MCP."""
    assert _mcp_exposed(FORGE_POLICY) == _public_tool_service_methods()


def test_readonly_policy_exposes_every_non_write_tool():
    """A read-only policy (inspector) exposes every non-write tool and
    denies exactly WRITE_TOOLS — the security boundary the policy layer
    exists to enforce."""
    exposed = _mcp_exposed(INSPECTOR_POLICY)
    public = _public_tool_service_methods()
    assert exposed == public - WRITE_TOOLS
    assert exposed & WRITE_TOOLS == set()


def test_write_tools_are_all_real_tool_service_methods():
    """Every name in WRITE_TOOLS must be a real ToolService method — a
    stale/typo'd entry would silently fail to deny anything (the policy
    would just never match it). Mirrors the existing
    test_write_tools_are_real_tool_names but asserts against the live
    class, not a snapshot."""
    stale = WRITE_TOOLS - _public_tool_service_methods()
    assert not stale, (
        f"WRITE_TOOLS names that are not ToolService methods (stale/typo): "
        f"{sorted(stale)}"
    )


# ---------------------------------------------------------------------------
# The cut that was just made: be-v2-hardwired API-boundary tools are gone
# from the generic surface (kept as an internal module for drift_detect /
# test_orchestration). Regression guard so they don't come back.
# ---------------------------------------------------------------------------


def test_api_route_tools_are_not_on_the_generic_surface():
    """`resolve_api_route` / `api_call_sites` were hardwired to one repo's
    layout (be-v2/src, backend/src, frontend/vite.config.js). They were cut
    from ToolService / HTTP / CLI; the `cie.api_routes` module stays only
    as an internal helper for drift_detect + test_orchestration. These
    must not reappear as user-facing tools."""
    public = _public_tool_service_methods()
    routes = _route_tool_keys()
    assert "resolve_api_route" not in public
    assert "api_call_sites" not in public
    assert "resolve_api_route" not in routes
    assert "api_call_sites" not in routes


# ---------------------------------------------------------------------------
# Policy back-compat aliases (Phase 0 scrub): canonical full/readonly exist
# and the historical forge/miner/inspector/orchestrator names still resolve
# to the same permission level.
# ---------------------------------------------------------------------------


def test_canonical_and_deprecated_policy_aliases():
    from cie.mcp_server import POLICIES_BY_NAME

    assert "full" in POLICIES_BY_NAME
    assert "readonly" in POLICIES_BY_NAME
    # historical aliases kept for back-compat — same permission level
    assert POLICIES_BY_NAME["forge"] is POLICIES_BY_NAME["full"]
    assert POLICIES_BY_NAME["orchestrator"] is POLICIES_BY_NAME["full"]
    assert POLICIES_BY_NAME["miner"] is POLICIES_BY_NAME["readonly"]
    assert POLICIES_BY_NAME["inspector"] is POLICIES_BY_NAME["readonly"]


def test_resolve_policy_accepts_canonical_and_alias_names():
    from cie.mcp_server import POLICIES_BY_NAME, resolve_policy

    for name in ("full", "readonly", "forge", "inspector"):
        assert resolve_policy(name) is POLICIES_BY_NAME[name]
    with pytest.raises(ValueError, match="unknown policy"):
        resolve_policy("does-not-exist")


# ---------------------------------------------------------------------------
# URN identity (Phase 0.5a): nodes identify as urn:cie:, not urn:protobox:
# ---------------------------------------------------------------------------


def test_node_urn_is_cie_not_protobox():
    from cie.models import Node
    from cie.data_model import to_urn

    n = Node(id="x", label="x", kind="function", project="demo")
    urn = to_urn(n)
    assert urn.startswith("urn:cie:"), urn
    assert "protobox" not in urn
    assert urn == "urn:cie:demo:function:x"


def test_rdf_export_uses_cie_prefix_not_prb():
    from cie.data_model import export_rdf
    from cie.models import Edge, Node

    n = Node(id="x", label="foo", kind="function", project="demo", source_file="a.py")
    rdf = export_rdf([n], [Edge(source="x", target="x", relation="calls")])
    assert "@prefix cie: <urn:cie:predicate:>" in rdf
    assert "prb:" not in rdf
    assert "urn:protobox" not in rdf
    assert "cie:label" in rdf and "cie:sourceFile" in rdf