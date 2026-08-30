"""The repair transaction layer: propose_patch / apply_patch / verify_patch.

Three tools, three jobs (see cie.patch's module docstring):

    propose = "what should change?"             (never mutates files)
    apply   = "can this change safely land?"    (the only file mutation)
    verify  = "did it fix the thing, unbroken?" (never mutates files)

Invariants under test here — what makes this a protocol rather than three
names over raw edits:

1. propose changes NOTHING on disk,
2. apply is the only mutation, and it is gated (context match, path jail,
   scope, syntax, atomic write + integrity check),
3. patches are immutable — only lifecycle status moves,
4. the proposer never certifies its own fix (verify is a separate tool),
5. gate failures record REJECTED with the reason — no silent retries
   against stale context.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from cie import patch as _patch
from cie.embedded_repository import NullTaskRepository
from cie.in_memory_repository import InMemoryRepository
from cie.query import QueryEngine
from cie.tools import ToolService

OLD_MS_LINE = "    effective = timeout\n"
NEW_SECONDS_LINE = "    effective = timeout / 1000\n"

PAYMENT_BEFORE = (
    "def charge_payment(amount: float, timeout: int = 5000) -> str:\n"
    "    \"\"\"timeout arrives in milliseconds (config), used as seconds.\"\"\"\n"
    "    effective = timeout\n"
    "    return f\"charged {amount} after {effective}\"\n"
    "\n"
    "\n"
    "def refund_payment(payment_id: str) -> str:\n"
    "    return f\"refunded {payment_id}\"\n"
)

TEST_ID = "test_charge_payment_converts_to_seconds"


@pytest.fixture()
def project_root(tmp_path: Path) -> Path:
    (tmp_path / "payment.py").write_text(PAYMENT_BEFORE)
    (tmp_path / "test_payment.py").write_text(
        "from payment import charge_payment\n"
        "\n"
        "\n"
        f"def {TEST_ID}():\n"
        "    assert \"after 5\" in charge_payment(10.0, timeout=5000)\n"
    )
    return tmp_path


@pytest.fixture()
def service(project_root: Path) -> ToolService:
    return ToolService(
        QueryEngine(InMemoryRepository([], [])),
        NullTaskRepository(),
        root=project_root,
    )


def propose_ms_to_seconds(service: ToolService, **overrides) -> dict:
    changes = [{
        "file": "payment.py",
        "symbol": "charge_payment",
        "old_text": OLD_MS_LINE,
        "new_text": NEW_SECONDS_LINE,
    }]
    kwargs = {"test_id": TEST_ID, **overrides}
    return service.propose_patch(changes=changes, **kwargs)


def patch_id_of(env: dict) -> str:
    assert env["ok"], env
    return env["results"][0]["patch_id"]


# ---------------------------------------------------------------------------
# pure gate helpers (cie.patch)
# ---------------------------------------------------------------------------


def test_patch_id_is_content_addressed_and_salted():
    changes = [{"file": "a.py", "operation": "edit",
                "old_text": "x=1", "new_text": "x=2"}]
    first = _patch.patch_id_for(changes, "2026-08-30T00:00:00Z")
    assert first == _patch.patch_id_for(changes, "2026-08-30T00:00:00Z")
    assert first != _patch.patch_id_for(changes, "2026-08-30T09:30:00Z")
    assert first.startswith("patch::")


def test_validate_change_rejects_malformed_entries():
    assert _patch.validate_change(
        {"file": "a.py", "old_text": "a", "new_text": "b"}) is None
    assert _patch.validate_change(
        {"file": "n.py", "operation": "create", "new_text": "x"}) is None
    assert _patch.validate_change("not a dict")
    assert _patch.validate_change({"file": ""})
    assert _patch.validate_change(
        {"file": "a.py", "old_text": "x", "new_text": "x"})


def test_context_gate_rejects_missing_and_ambiguous_matches():
    assert _patch.check_change_context("a = 1\n", "a = 1") is None
    missing = _patch.check_change_context("a = 1\n", "a = 2")
    assert missing and "PATCH_CONTEXT_MISMATCH" in missing
    ambiguous = _patch.check_change_context("a = 1\na = 1\n", "a = 1")
    assert ambiguous and "2 locations" in ambiguous


def test_scope_gate_enforces_the_proposal_allowlist():
    assert _patch.scope_of(None, [{"file": "x.py"}]) is None
    assert _patch.scope_of(["a.py"], [{"file": "a.py"}]) is None
    bad = _patch.scope_of(["a.py"], [{"file": "a.py"}, {"file": "evil.py"}])
    assert bad and "PATCH_SCOPE_VIOLATION" in bad


def test_syntax_check_python_exact_others_honestly_skipped():
    assert _patch.syntax_check("a.py", "def f():\n    return 1\n")["valid"]
    broken = _patch.syntax_check("a.py", "def f(:\n")
    assert broken["valid"] is False and "SyntaxError" in broken["detail"]
    assert _patch.syntax_check("main.go", "package main")["valid"] is None


def test_net_new_imports_reports_only_added_lines():
    assert _patch.net_new_imports("a = 1", "import os\na = 1") == ["import os"]
    assert _patch.net_new_imports("import os", "import os\n") == []


# ---------------------------------------------------------------------------
# propose_patch — the reasoning artifact
# ---------------------------------------------------------------------------


def test_propose_builds_a_full_plan_and_never_touches_the_file(
        service, project_root):
    before = (project_root / "payment.py").read_text()
    env = service.propose_patch(
        changes=[{
            "file": "payment.py",
            "symbol": "charge_payment",
            "old_text": OLD_MS_LINE,
            "new_text": NEW_SECONDS_LINE,
        }],
        test_id=TEST_ID,
        root_cause="config stores milliseconds, code uses seconds",
        confidence=0.94,
        after_patch="charge_payment(10.0, timeout=5000) reports 5",
        agent="test-agent",
        model="test-model",
    )
    assert env["ok"], env
    plan = env["results"][0]

    assert plan["patch_id"].startswith("patch::")
    assert plan["status"] == "PROPOSED"
    diff = plan["changes"][0]["diff"]
    assert "-    effective = timeout" in diff
    assert "+    effective = timeout / 1000" in diff

    # enrichment the model didn't have to supply by hand
    assert plan["impact"]["affected_symbols"] == ["charge_payment"]
    assert plan["risk"]["level"] in ("LOW", "MEDIUM", "HIGH")
    assert plan["provenance"]["model"] == "test-model"
    assert plan["intent"]["counterfactual"]["after_patch"]

    # propose is a reasoning artifact: the file did NOT change
    assert (project_root / "payment.py").read_text() == before

    # persisted and read back intact — immutability baseline
    got = service.get_patch(plan["patch_id"])
    assert got["ok"]
    got_plan = got["results"][0]
    assert got_plan["changes"] == plan["changes"]
    assert got_plan["status"] == "PROPOSED"


def test_propose_rejects_empty_and_malformed_lists(service):
    empty = service.propose_patch(changes=[])
    assert not empty["ok"] and empty["error"]["kind"] == "validation"
    bad = service.propose_patch(
        changes=[{"file": "", "old_text": "a", "new_text": "b"}],
    )
    assert not bad["ok"]
    assert "non-empty 'file'" in bad["error"]["message"]


def test_propose_missing_edit_target_is_not_found(service):
    env = service.propose_patch(
        changes=[{"file": "ghost.py", "old_text": "x = 1",
                  "new_text": "x = 2"}],
    )
    assert not env["ok"]
    assert env["error"]["kind"] == "not_found"
    assert "operation='create'" in env["error"]["message"]


def test_propose_jails_paths(service):
    env = service.propose_patch(
        changes=[{"file": "../escape.py", "old_text": "a", "new_text": "b"}],
    )
    assert not env["ok"]
    assert env["error"]["kind"] == "validation"


def test_propose_scope_violation(service):
    env = service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_MS_LINE,
                  "new_text": NEW_SECONDS_LINE}],
        allowed_files=["other.py"],
    )
    assert not env["ok"]
    assert "PATCH_SCOPE_VIOLATION" in env["error"]["message"]


def test_propose_create_on_existing_file(service):
    env = service.propose_patch(
        changes=[{"file": "payment.py", "operation": "create",
                  "new_text": "x = 1\n"}],
    )
    assert not env["ok"]
    assert "existing file" in env["error"]["message"]


def test_propose_ambiguous_old_text_fails_at_propose_time(service, project_root):
    (project_root / "dup.py").write_text("x = 1\nx = 1\n")
    env = service.propose_patch(
        changes=[{"file": "dup.py", "old_text": "x = 1", "new_text": "x = 2"}],
    )
    assert not env["ok"]
    assert "PATCH_CONTEXT_MISMATCH" in env["error"]["message"]
    # and the file is untouched
    assert (project_root / "dup.py").read_text() == "x = 1\nx = 1\n"


def test_newer_proposal_for_same_test_supersedes_the_open_one(service):
    first = patch_id_of(service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_MS_LINE,
                  "new_text": NEW_SECONDS_LINE}],
        test_id=TEST_ID,
    ))
    second = patch_id_of(service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_MS_LINE,
                  "new_text": NEW_SECONDS_LINE}],
        test_id=TEST_ID,
    ))
    by_id = {p["patch_id"]: p
             for p in service.list_patches().get("results", [])}
    assert by_id[first]["status"] == "SUPERSEDED"
    assert by_id[second]["status"] == "PROPOSED"


# ---------------------------------------------------------------------------
# apply_patch — gates, then ONE atomic mutation
# ---------------------------------------------------------------------------


def test_apply_happy_path_and_immutability(service, project_root):
    proposal = service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_MS_LINE,
                  "new_text": NEW_SECONDS_LINE}],
        test_id=TEST_ID,
    )
    pid = patch_id_of(proposal)
    proposed_changes = proposal["results"][0]["changes"]

    applied = service.apply_patch(pid)
    assert applied["ok"], applied
    entry = applied["results"][0]
    assert entry["status"] == "APPLIED"
    assert set(entry["files"]) == {"payment.py"}
    assert entry["files"]["payment.py"]["created"] is False

    content = (project_root / "payment.py").read_text()
    assert NEW_SECONDS_LINE in content
    assert OLD_MS_LINE not in content

    got = service.get_patch(pid).get("results", [{}])[0]
    assert got["status"] == "APPLIED"
    # immutable: the plan's change list is byte-identical after apply
    assert got["changes"] == proposed_changes
    assert "applied_at" in got


def test_apply_is_not_re_runnable(service, project_root):
    proposal = service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_MS_LINE,
                  "new_text": NEW_SECONDS_LINE}],
    )
    pid = patch_id_of(proposal)
    assert service.apply_patch(pid)["ok"]
    again = service.apply_patch(pid)
    assert not again["ok"]
    assert "already APPLIED" in again["error"]["message"]


def test_apply_unknown_patch(service):
    env = service.apply_patch("patch::missing")
    assert not env["ok"] and env["error"]["kind"] == "not_found"
    assert "list_patches" in env["hint"]


def test_apply_context_mismatch_terminates_the_patch(service, project_root):
    pid = patch_id_of(service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_MS_LINE,
                  "new_text": NEW_SECONDS_LINE}],
    ))
    # an out-of-band edit AFTER the proposal invalidates its context
    service.edit_file(
        "payment.py",
        "    \"\"\"timeout arrives in milliseconds (config), used as seconds.\"\"\"\n",
        "    \"\"\"timeout arrives in milliseconds (config), used as seconds now.\"\"\"\n",
    )
    applied = service.apply_patch(pid)
    assert not applied["ok"]
    assert "PATCH_CONTEXT_MISMATCH" in applied["error"]["message"]
    # out-of-band edit survived; the patch never landed; the plan is terminal
    assert "seconds now" in (project_root / "payment.py").read_text()
    assert OLD_MS_LINE in (project_root / "payment.py").read_text()
    rejected = [p for p in service.list_patches().get("results", [])
                if p["patch_id"] == pid]
    assert rejected and rejected[0]["status"] == "REJECTED"


def test_apply_multi_change_plan_is_atomic(service, project_root):
    proposal = service.propose_patch(
        changes=[
            {"file": "payment.py", "old_text": OLD_MS_LINE,
             "new_text": NEW_SECONDS_LINE},
            {"file": "registry.py", "operation": "create",
             "new_text": "REGISTRY: list[str] = []\n"},
        ],
    )
    pid = patch_id_of(proposal)
    applied = service.apply_patch(pid)
    assert applied["ok"], applied
    assert set(applied["results"][0]["files"]) == {"payment.py", "registry.py"}
    assert (project_root / "payment.py").read_text() != PAYMENT_BEFORE
    assert (project_root / "registry.py").read_text() == "REGISTRY: list[str] = []\n"


def test_apply_syntax_gate_keeps_the_repo_untouched(service, project_root):
    pid = patch_id_of(service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_MS_LINE,
                  "new_text": "    effective = ((timeout / \n"}],
    ))
    applied = service.apply_patch(pid)
    assert not applied["ok"]
    assert "would not parse" in applied["error"]["message"]
    # file bytes are exactly pre-patch
    assert (project_root / "payment.py").read_text() == PAYMENT_BEFORE
    listed = {p["patch_id"]: p
              for p in service.list_patches().get("results", [])}
    assert listed[pid]["status"] == "REJECTED"

# ---------------------------------------------------------------------------
# verify_patch — the evidence-based verdict (repo untouched)
# ---------------------------------------------------------------------------


def test_verify_passes_a_correctly_applied_patch(service, project_root):
    pid = patch_id_of(service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_MS_LINE,
                  "new_text": NEW_SECONDS_LINE}],
        test_id=TEST_ID,
        after_patch="charge_payment(10.0, timeout=5000) reports 5 units",
    ))
    assert service.apply_patch(pid)["ok"]
    env = service.verify_patch(pid)
    assert env["ok"], env
    report = env["results"][0]
    assert report["status"] == "VERIFIED"
    check_names = {c["name"] for c in report["checks"]}
    assert {"patch_content_present", "syntax_valid", "counterfactual_holds",
            "regression_surface"} <= check_names
    assert not [c for c in report["checks"] if c["status"] == "fail"]
    # the verdict is durable history, appended (never overwritten content)
    got = service.get_patch(pid).get("results", [{}])[0]
    assert got["status"] == "VERIFIED"
    assert got["verification_history"][0]["status"] == "VERIFIED"


def test_verify_with_run_tests_executes_the_discovered_tests(service, monkeypatch):
    pid = patch_id_of(service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_MS_LINE,
                  "new_text": NEW_SECONDS_LINE}],
        test_id=TEST_ID,
    ))
    assert service.apply_patch(pid)["ok"]

    # run_tests itself needs a host-project module (`core.llm`) that isn't
    # importable in a standalone checkout (see pyproject.toml's testpaths
    # comment) — verify's wiring is what this test pins: the DISCOVERED
    # test files (the failing test's file, from the heuristic index) are
    # what gets executed, and the executed status decides the verdict.
    executed: list[list] = []

    def fake_run_tests(test_type, target_files):
        executed.append(target_files)
        return {"ok": True, "results": [{"test_type": test_type,
                                         "status": "passed"}]}

    service.run_tests = fake_run_tests  # type: ignore[method-assign]
    env = service.verify_patch(pid, run_tests=True, test_type="unit")
    assert env["ok"], env
    report = env["results"][0]
    checks = {c["name"]: c["status"] for c in report["checks"]}
    assert checks["executed_tests"] == "pass"
    assert report["status"] == "VERIFIED"

    # and a failing suite must fail the verification, not the call
    service.run_tests = (  # type: ignore[method-assign]
        lambda *a, **k: {"ok": True, "results": [{"status": "failed"}]}
    )
    second = service.verify_patch(pid, run_tests=True, test_type="unit")
    checks2 = {c["name"]: c["status"] for c in second["results"][0]["checks"]}
    assert checks["executed_tests"] == "pass"  # the FIRST report
    assert checks2["executed_tests"] == "fail"
    assert second["results"][0]["status"] == "FAILED"
    assert [c["name"] for c in second["results"][0]["checks"]
            if c["status"] == "fail"] == ["executed_tests"]


def test_verify_fails_when_the_patch_is_reverted_after_apply(
        service, project_root):
    pid = patch_id_of(service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_MS_LINE,
                  "new_text": NEW_SECONDS_LINE}],
    ))
    assert service.apply_patch(pid)["ok"]
    # someone reverts the file out-of-band: the "fix" is no longer there
    (project_root / "payment.py").write_text(PAYMENT_BEFORE)
    env = service.verify_patch(pid)
    assert env["ok"]
    report = env["results"][0]
    assert report["status"] == "FAILED"
    failed = {c["name"] for c in report["checks"] if c["status"] == "fail"}
    assert "patch_content_present" in failed
    assert "counterfactual_holds" in failed


def test_verify_refuses_unapplied_and_unknown_patches(service):
    proposed = service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_MS_LINE,
                  "new_text": NEW_SECONDS_LINE}],
    )
    env = service.verify_patch(patch_id_of(proposed))
    assert not env["ok"]
    assert "apply_patch" in env["error"]["message"]
    unknown = service.verify_patch("patch::missing")
    assert not unknown["ok"] and unknown["error"]["kind"] == "not_found"


def test_verify_appends_to_history_not_replaces(service):
    pid = patch_id_of(service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_MS_LINE,
                  "new_text": NEW_SECONDS_LINE}],
    ))
    assert service.apply_patch(pid)["ok"]
    first = service.verify_patch(pid)
    second = service.verify_patch(pid)
    assert first["ok"] and second["ok"]
    got = service.get_patch(pid).get("results", [{}])[0]
    assert len(got["verification_history"]) == 2
    assert all(h["status"] == "VERIFIED" for h in got["verification_history"])


def test_verify_is_read_only_over_the_repo(service, project_root):
    pid = patch_id_of(service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_MS_LINE,
                  "new_text": NEW_SECONDS_LINE}],
    ))
    assert service.apply_patch(pid)["ok"]
    after_apply = (project_root / "payment.py").read_text()
    env = service.verify_patch(pid)
    assert env["ok"]
    assert (project_root / "payment.py").read_text() == after_apply


# ---------------------------------------------------------------------------
# the audit trail (get_patch / list_patches) and the policy boundary
# ---------------------------------------------------------------------------


def test_get_patch_includes_rejection_reason(service):
    pid = patch_id_of(service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_MS_LINE,
                  "new_text": "    effective = ((timeout / \n"}],
    ))
    service.apply_patch(pid)
    got = service.get_patch(pid).get("results", [{}])[0]
    assert got["status"] == "REJECTED"
    assert "would not parse" in got.get("rejected_reason", "")


def test_list_patches_summarizes_history(service):
    assert service.list_patches().get("results", []) == []
    pid = patch_id_of(service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_MS_LINE,
                  "new_text": NEW_SECONDS_LINE}],
        test_id=TEST_ID,
    ))
    assert service.apply_patch(pid)["ok"]
    assert service.verify_patch(pid)["ok"]
    listed = service.list_patches().get("results", [])
    assert len(listed) == 1
    entry = listed[0]
    assert entry["status"] == "VERIFIED"
    assert entry["files"] == ["payment.py"]
    assert entry["test_id"] == TEST_ID
    filtered = service.list_patches(status="PROPOSED")
    assert filtered.get("results", []) == []


def test_unknown_get_patch_and_list_hint(service):
    env = service.get_patch("patch::ghost")
    assert not env["ok"] and env["error"]["kind"] == "not_found"


# ---------------------------------------------------------------------------
# policy boundary: apply/verify/propose are write tools, reads stay open
# ---------------------------------------------------------------------------


def test_patch_tools_policy_classification():
    from cie.tool_policy import (
        FORGE_POLICY,
        INSPECTOR_POLICY,
        WRITE_TOOLS,
    )

    assert {"propose_patch", "apply_patch", "verify_patch"} <= WRITE_TOOLS
    assert "get_patch" not in WRITE_TOOLS
    assert "list_patches" not in WRITE_TOOLS
    assert FORGE_POLICY.permits("apply_patch")
    assert not INSPECTOR_POLICY.permits("apply_patch")
    assert INSPECTOR_POLICY.permits("get_patch")
    assert INSPECTOR_POLICY.permits("list_patches")
