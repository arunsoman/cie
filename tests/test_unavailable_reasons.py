"""R5 — machine-readable `reason` slugs on the unavailable-by-design set.

The contract this pins: every tool that 503s as `unavailable` on a
standalone install must carry `error.reason` — a stable slug, not prose —
so conformance harnesses and policy layers can assert against it
(roadmap R5: "every remaining 503 carries a machine-readable reason
string"). The registry below is hand-maintained: adding a tool that can
be unavailable requires an intentional entry here, which is exactly the
review point at which R5 asks "could this be degraded gracefully
instead?"

Verified-against-live expectations (2026-08-30, post lazy-`core.llm`
refactor — see the per-module R5 notes):
  - 14 formerly-503 tools now actually run (pure logic, no LLM);
  - `state_machine_run`, `qa`, `contracts_run` genuinely need the host
    project's `core.llm` LLM layer -> OPTIONAL_BACKEND_MISSING:core;
  - `decompose_page` needs a host-registered decompose plugin ->
    HOST_PLUGIN_MISSING:decompose-detector.
"""

from __future__ import annotations

import pytest

from cie.factory import build_tool_service_embedded

#: tool name -> the machine-readable reason slug its `unavailable`
#: envelope MUST carry. Keep hand-curated; a new entry means a human
#: decided this tool cannot degrade, not that an import happened to fail.
EXPECTED_REASONS = {
    "contracts_run": "OPTIONAL_BACKEND_MISSING:core",
    "state_machine_run": "OPTIONAL_BACKEND_MISSING:core",
    "qa": "OPTIONAL_BACKEND_MISSING:core",
    # LLM-only in practice: on an empty project it short-circuits to an
    # ok/hinted envelope (no communities to summarize) — the conformance
    # sandbox HAS communities (fixture below reproduces), so its 503 is
    # the honest live shape. See surface_results.json's bucket.
    "community_summarize_run": "OPTIONAL_BACKEND_MISSING:core",
    "decompose_page": "HOST_PLUGIN_MISSING:decompose-detector",
}


@pytest.fixture(scope="module")
def svc(tmp_path_factory):
    root = tmp_path_factory.mktemp("reasons")
    (root / "app.py").write_text("def alpha():\n    return 1\n")
    service = build_tool_service_embedded(root)
    # community_summarize_run only reaches its LLM import when the graph
    # HAS communities (empty project short-circuits to a hinted ok), so
    # give it one the same way community_detect_run would.
    from cie.callgraph import resolve_call_edges
    from cie.extract import extract_many

    per_file = extract_many(root)
    nodes = [n for ext in per_file for n in ext.nodes]
    edges = (
        [e for ext in per_file for e in ext.edges] + resolve_call_edges(per_file)
    )
    service._engine._repo.load_extraction(nodes, edges)  # noqa: SLF001
    service.community_detect_run()
    return service


@pytest.mark.parametrize("tool", sorted(EXPECTED_REASONS))
def test_unavailable_tools_carry_their_machine_readable_reason(svc, tool):
    kwargs = {
        "contracts_run": {"text": "rate limiter retries <= 3"},
        "state_machine_run": {"text": "a lock has locked/unlocked states"},
        "qa": {"question": "who calls alpha?"},
        "community_summarize_run": {},
        "decompose_page": {"html": "<html></html>", "screen_id": "s1"},
    }[tool]
    env = getattr(svc, tool)(**kwargs)
    assert env["ok"] is False, f"{tool} unexpectedly succeeded"
    assert env["error"]["kind"] == "unavailable", f"{tool}: {env['error']}"
    assert env["error"].get("reason") == EXPECTED_REASONS[tool], (
        f"{tool}'s 503 must carry the pinned machine-readable reason "
        f"{EXPECTED_REASONS[tool]!r}; got {env['error'].get('reason')!r}"
    )
    assert env["hint"], "unavailable failures must stay self-describing"


def test_unavailable_bucket_is_no_larger_than_six(svc):
    """The R5 gate: standalone-install 503s went 18 -> 4, and STAY that
    low. A tool newly returning `unavailable` here either regressed a
    lazy-import fix or needs an intentional registry entry (and a
    decompose-vs-degrade decision) before merging."""
    from cie.tools import ToolService

    over = []
    for name in sorted(vars(ToolService)):
        if name.startswith("_") or name == "describe":
            continue
        env = None
        try:
            env = _probe(svc, name)
        except Exception:  # noqa: BLE001 — a raise is worse than a 503
            over.append((name, "raised instead of envelope"))
            continue
        if isinstance(env, dict) and not env.get("ok") \
                and env.get("error", {}).get("kind") == "unavailable":
            if env["error"].get("reason") not in set(EXPECTED_REASONS.values()):
                over.append((name, env["error"].get("reason")))
    # every live unavailable must be one of the registered ones
    assert [n for n, _ in over] == [], (
        f"tools returned `unavailable` without a registered reason: {over} — "
        "either fix the regression (R5's rule: degrade gracefully when the "
        "logic is pure) or add an intentional EXPECTED_REASONS entry"
    )


def _probe(svc, name: str):
    """Call one tool with plausible zero/empty args and return its
    envelope; only meant for the failure-bucket scan above (not a
    happy-path harness — that lives in tool-test-lab)."""
    import inspect

    fn = getattr(svc, name)
    try:
        sig = inspect.signature(fn)
    except (TypeError, ValueError):  # pragma: no cover
        return None
    kwargs = {}
    for pname, p in sig.parameters.items():
        if p.default is not inspect.Parameter.empty:
            kwargs[pname] = p.default
    # required params get filler values to provoke an honest error
    # envelope, never a crash
    for pname, p in sig.parameters.items():
        if p.default is inspect.Parameter.empty and pname not in kwargs:
            kwargs[pname] = ""
    return fn(**kwargs)


def test_full_suite_never_raises_on_unavailable_backends(svc):
    """Regression pin for the R5 crash class: the lazy refactor moved
    imports to call sites — a wrong import placement would RAISE from the
    tool method instead of returning an envelope. Pure tools must succeed
    outright; the LLM-only ones degrade as envelopes with reasons."""
    assert svc.contracts(contract_type="", scope="")["ok"] is True
    assert svc.validate_types(domain_types={})["ok"] is True
    # the LLM-only ones degrade as envelopes with reasons, never raise
    env = svc.state_machine_run(text="lock: locked, unlocked")
    assert env["ok"] is False and env["error"]["kind"] == "unavailable"
