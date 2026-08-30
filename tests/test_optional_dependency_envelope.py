"""Optional-dependency degradation: zombie `core.*` imports (protobox era)
must produce SPEC §0 `kind="unavailable"` envelopes, not 500-style
"internal" crashes — and after R5 (2026-08-30): the four FIXABLE modules
no longer import `core.llm` at module level at all, so their pure tools
actually RUN standalone; only genuinely LLM-only entry points degrade,
with machine-readable `reason` slugs.

Found by dogfooding `cie-mcp --embedded` against this repo (2026-08-30):
`coverage_gaps()` crashed with `ModuleNotFoundError: No module named 'core'`
and shipped a bare "unexpected tool failure; report this" envelope — even
though `coverage_gaps` itself is pure Python; only a module-level import in
`cie/test_orchestration.py` (one of 5 protobox leftovers: community_detect,
contracts, graphrag, state_machine, test_orchestration) drags `core.llm` in.
The lazy-import stop-gap (this file's original tests) routed the crash to a
graceful `unavailable` envelope — roadmap R5 then did the real fix: module
imports deferred into the LLM call sites (community_detect, contracts,
state_machine, test_orchestration), leaving graphrag's `qa` and the other
call-time-only LLM users as intentional `unavailable[OPTIONAL_BACKEND_MISSING]`.

Contract now: pure tools RUN standalone; genuinely LLM-only tools return
`kind="unavailable"` (503 on HTTP) WITH `error.reason` — an expected
property of THIS installation, not a reportable crash.
"""

from __future__ import annotations

import importlib

import pytest

pytest.importorskip("mcp", reason="mcp is an optional dependency (pip install cie[mcp])")

from cie.config import CieConfig, Neo4jConfig
from cie.envelope import ERROR_KINDS, err_envelope
from cie.factory import build_tool_service_from_config

#: the 5 modules still importing the protobox `core` package (lazy-imported
#: per tool call, which is why the crash surfaced at call time)
ZOMBIE_LLM_MODULES = (
    "cie.community_detect",
    "cie.contracts",
    "cie.graphrag",
    "cie.state_machine",
    "cie.test_orchestration",
)


@pytest.fixture(scope="module")
def service(tmp_path_factory):
    config = CieConfig(
        project_root=tmp_path_factory.mktemp("proj"),
        neo4j=Neo4jConfig(uri="bolt://localhost:7687", user="neo4j", password="password"),
    )
    return build_tool_service_from_config(config)


def test_core_is_genuinely_absent_in_standalone():
    """Proves what the other tests here assert against — if `core` IS
    importable this is a protobox-monorepo checkout, and the standalone
    degradation guarantee simply doesn't apply."""
    with pytest.raises(ModuleNotFoundError):
        importlib.import_module("core")


@pytest.mark.parametrize("module", ZOMBIE_LLM_MODULES)
def test_fixed_modules_now_import_standalone(module):
    """The R5 fix, pinned: the former zombie modules import standalone
    with NO `core.llm` on the path (their LLM use is deferred to the one
    call site per module that actually asks). R10 extended this to
    `cie.graphrag` itself: the pure pipeline layers (retrieval →
    expansion → `_assemble_context`) now import standalone so the
    semantic benchmark can drive them; the LLM bound moved to call time
    — `qa()`'s availability gate at call START (preserving R5's
    `unavailable[OPTIONAL_BACKEND_MISSING:core]` envelope exactly) and
    `rerank()`'s degrade-to-original-order path (pinned separately
    below)."""
    import sys
    sys.modules.pop(module, None)
    importlib.import_module(module)


def test_deferred_llm_entry_points_still_fail_only_at_call_time():
    """The remaining LLM-only functions raise the honest ModuleNotFoundError
    AT CALL TIME (never at import) — `_guard` converts that to
    `unavailable[OPTIONAL_BACKEND_MISSING]` (see
    tests/test_unavailable_reasons.py for the reason-slug registry)."""
    import asyncio

    import cie.contracts as contracts

    with pytest.raises(ModuleNotFoundError, match="core"):
        __import__("asyncio").run(contracts.extract_contracts("retries <= 3"))


def test_graphrag_llm_bound_is_call_time_not_import_time():
    """R10's refinement of the graphrag contract, both halves pinned:

    - import: `cie.graphrag` imports standalone (its pure retrieval +
      context-assembly layers are benchmarkable without the host — the
      R10 semantic benchmark depends on that).
    - call: `ToolService.qa` still resolves to the exact R5 envelope
      (`unavailable[OPTIONAL_BACKEND_MISSING:core]` — pinned in
      test_unavailable_reasons.py) because its availability gate runs at
      call start; `rerank` degrades to the hybrid order instead of
      raising (its own degraded-by-design path)."""
    import asyncio

    import cie.graphrag as graphrag
    from cie.models import HybridMatch, Node

    node = Node(id="x", label="x", kind="FUNC", source_file="x.py")
    match = HybridMatch(node=node, score=1.0, lexical_score=1.0,
                        dense_score=0.0, graph_score=0.0)

    # rerank degrades (never raises) without core.llm:
    ranked = asyncio.run(graphrag.rerank("q", [match, match]))
    assert [m.node.id for m in ranked] == ["x", "x"]  # order preserved

    # qa raises at call time (NOT import time) — the pipeline-level half
    # of the envelope contract:
    with pytest.raises(ModuleNotFoundError, match="core"):
        asyncio.run(graphrag.qa("anything"))


def test_error_kind_unavailable_is_registered_in_the_spec():
    assert "unavailable" in ERROR_KINDS


def test_err_envelope_preserves_unavailable_kind():
    env = err_envelope("coverage_gaps", "unavailable", "No module named 'core'")
    assert env["error"]["kind"] == "unavailable"  # pre-fix this coerced to "internal"


def test_guard_maps_module_not_found_error_to_unavailable(service):
    started = 0.0  # elapsed_ms is informational for this assertion
    env = service._guard(
        "some_llm_tool", started, ModuleNotFoundError("No module named 'core'")
    )
    assert env["ok"] is False
    assert env["error"]["kind"] == "unavailable"
    assert "ModuleNotFoundError" in env["error"]["message"]
    assert "core" in env["hint"]


@pytest.fixture(scope="module")
def embedded_service(tmp_path_factory):
    """A zero-config embedded service for the now-pure tools (no Neo4j, no
    `core.llm` — the exact environment the original crash happened in)."""
    from cie.factory import build_tool_service_embedded

    root = tmp_path_factory.mktemp("puretools")
    (root / "app.py").write_text("def alpha():\n    return 1\n")
    return build_tool_service_embedded(root)


def test_coverage_gaps_refactored_to_actually_run(embedded_service):
    """The tool that crashed live through the MCP surface in the original
    dogfood. After R5 (lazy `core.llm` in cie.test_orchestration now only
    wraps the LLM path), `coverage_gaps` IS pure and RUNS: empty graph ->
    ok envelope with the plan-first hint, no unavailable, no crash."""
    env = embedded_service.coverage_gaps()
    assert env["ok"] is True
    assert env["tool"] == "coverage_gaps"
    assert "test_plan first" in (env.get("hint") or "")