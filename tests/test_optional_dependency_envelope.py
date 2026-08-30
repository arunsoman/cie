"""Optional-dependency degradation: zombie `core.*` imports (protobox era)
must produce SPEC §0 `kind="unavailable"` envelopes, not 500-style
"internal" crashes.

Found by dogfooding `cie-mcp --embedded` against this repo (2026-08-30):
`coverage_gaps()` crashed with `ModuleNotFoundError: No module named 'core'`
and shipped a bare "unexpected tool failure; report this" envelope — even
though `coverage_gaps` itself is pure Python; only a module-level import in
`cie/test_orchestration.py` (one of 5 protobox leftovers: community_detect,
contracts, graphrag, state_machine, test_orchestration) drags `core.llm` in.
Five affected modules are imported lazily per tool call, so the crash fired
at call time on the live read-only surface.

Contract after the fix: `ToolService._guard` (the single funnel behind every
MCP tool result) and the HTTP tool dispatch both map `ModuleNotFoundError`
to a NEW error kind `unavailable` (503 on HTTP) — an expected property of
THIS installation, not a reportable crash.
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
def test_zombie_modules_still_not_importable_standalone(module):
    """Honest documentation of the current state: these modules fail to
    import standalone. The FIX being tested is that tools route through
    this failure as a graceful `unavailable` envelope — not that the
    modules import (full `core.llm` decoupling is a separate workstream)."""
    import sys
    sys.modules.pop(module, None)
    with pytest.raises(ModuleNotFoundError, match="core"):
        importlib.import_module(module)


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


def test_coverage_gaps_crash_becomes_graceful_envelope(service):
    """The exact tool that crashed live through the MCP surface. It imports
    `cie.test_orchestration` lazily inside its own try, so the whole
    failure path (lazy import → ModuleNotFoundError → _guard) runs for
    real here."""
    env = service.coverage_gaps()
    assert env["ok"] is False
    assert env["tool"] == "coverage_gaps"
    assert env["error"]["kind"] == "unavailable"
    assert "core" in env["error"]["message"] or "core" in (env.get("hint") or "")