"""The AST mirror (cie.ast_store) and its two tools: get_meta/get_function.

Contract under test — "cie is the single source where the actual file
and its AST stay":

1. get_meta/get_function serve ONLY from the parsed AST snapshot, never
   a disk read — proven here by mutating the file on disk BEHIND cie's
   back and showing the tools still serve cie's view until reindex_file
   explicitly refreshes the mirror.
2. every cie write path refreshes the mirror in the same call as the
   filesystem write (write_file/edit_file/delete_file/apply_patch/
   reindex_file/sync_ast_delta) — after apply_patch, get_function
   returns the patched body with no disk read in between.
3. apply_patch's atomicity extends to the mirror: a gate-REJECTED patch
   (stale old_text) leaves file AND mirror untouched; only a patch that
   survives the write + integrity gates moves both.
4. a 3,000-line function is windowed (nested-definition map + paging),
   never dumped.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from cie.ast_store import NESTED_CAP, WINDOW
from cie.embedded_repository import NullTaskRepository
from cie.in_memory_repository import InMemoryRepository
from cie.query import QueryEngine
from cie.tools import ToolService

SRC = (
    '"""A payment module."""\n'
    "\n"
    "\n"
    "def charge_payment(amount: float, timeout: int = 5000) -> str:\n"
    '    """Charge the customer."""\n'
    "    effective = timeout\n"
    "    return f\"charged {amount} after {effective}\"\n"
    "\n"
    "\n"
    "class Gateway:\n"
    '    """Card gateway."""\n'
    "\n"
    "    def charge(self, amount: float) -> str:\n"
    "        return charge_payment(amount)\n"
    "\n"
    "    def refund(self, payment_id: str) -> str:\n"
    "        return f\"refunded {payment_id}\"\n"
    "\n"
    "\n"
    "def refund_payment(payment_id: str) -> str:\n"
    "    return f\"refunded {payment_id}\"\n"
)

OLD_LINE = "    effective = timeout\n"
NEW_LINE = "    effective = timeout / 1000\n"


@pytest.fixture()
def project_root(tmp_path: Path) -> Path:
    (tmp_path / "payment.py").write_text(SRC)
    (tmp_path / "notes.md").write_text("# Notes\nline two\n")
    return tmp_path


@pytest.fixture()
def service(project_root: Path) -> ToolService:
    return ToolService(
        QueryEngine(InMemoryRepository([], [])),
        NullTaskRepository(),
        root=project_root,
    )


def _signatures(service: ToolService, path: str = "payment.py") -> list[str]:
    env = service.get_meta(path)
    assert env["ok"], env
    return [fn["signature"] for fn in env["results"][0]["functions"]]


# ---------------------------------------------------------------------------


def test_get_meta_serves_file_type_lines_and_signatures(service):
    env = service.get_meta("payment.py")
    assert env["ok"], env
    row = env["results"][0]
    assert row["file_type"] == "python"
    assert row["tier"] == "python-ast"
    assert row["total_lines"] == len(SRC.splitlines())
    assert row["function_count"] == 3  # top-level table: charge_payment, Gateway, refund_payment
    sigs = row["functions"]
    assert sigs[0]["signature"].startswith("def charge_payment(amount: float, timeout: int = 5000) -> str:")
    assert sigs[1]["kind"] == "class" and sigs[1]["signature"].startswith("class Gateway:")
    # top-level table carries the three file-level definitions
    assert {fn["qualname"] for fn in sigs} == {"charge_payment", "Gateway", "refund_payment"}
    # spans come off the AST, decorators included
    charge = sigs[0]
    assert charge["start_line"] == 4 and charge["end_line"] == 7
    assert charge["doc"] == "Charge the customer."


def test_get_function_returns_the_actual_ast_content(service):
    env = service.get_function("payment.py", "def charge_payment(amount: float, timeout: int = 5000) -> str:")
    assert env["ok"], env
    row = env["results"][0]
    assert row["qualname"] == "charge_payment"
    assert row["start_line"] == 4 and row["end_line"] == 7
    assert row["total_lines"] == 4
    assert not env["truncated"] and env["hint"] is None
    # content carries view_file's exact numbering format, absolute lines
    assert row["content"].splitlines()[0] == f"{4:>5}\tdef charge_payment(amount: float, timeout: int = 5000) -> str:"
    assert f"{7:>5}\t    return f\"charged {{amount}} after {{effective}}\"" in row["content"]


def test_signature_matching_ladder(service):
    # bare name (unique) + qualified name both resolve
    assert service.get_function("payment.py", "refund_payment")["ok"]
    row = service.get_function("payment.py", "Gateway.charge")["results"][0]
    assert row["start_line"] == 13 and row["end_line"] == 14
    # bare name that is unique resolves to the method
    row = service.get_function("payment.py", "charge")["results"][0]
    assert row["qualname"] == "Gateway.charge"
    # full signature whatever get_meta printed, minus whitespace/colon
    assert service.get_function("payment.py", "def Gateway():")["ok"] is False  # class sig differs
    # a genuinely ambiguous bare name -> error listing candidates
    service.write_file(
        "dupes.py",
        "class A:\n"
        "    def run(self) -> int:\n"
        "        return 1\n"
        "\n"
        "class B:\n"
        "    def run(self) -> int:\n"
        "        return 2\n",
    )
    dup = service.get_function("dupes.py", "run")
    assert dup["ok"] is False
    assert dup["error"]["kind"] == "ambiguous"
    assert "A.run" in dup["hint"] and "B.run" in dup["hint"]
    # nothing matching -> not_found with the candidate list hint
    miss = service.get_function("payment.py", "nope")
    assert miss["error"]["kind"] == "not_found"
    assert "get_meta" in miss["hint"]


def test_get_function_windows_a_3000_line_function(service, project_root):
    # one huge function with nested definitions to navigate by
    body = "\n".join(f"    x{i} = {i}" for i in range(2990))
    big = (
        "def giant(seed: int) -> int:\n"
        '    """A very long function."""\n'
        "\n"
        "    def helper(v: int) -> int:\n"
        "        return v + seed\n"
        "\n"
        "    class Inner:\n"
        "        def twice(self, v: int) -> int:\n"
        "            return helper(v) * 2\n"
        "\n"
        f"{body}\n"
        "    return helper(seed)\n"
    )
    (project_root / "giant.py").write_text(big)
    service.reindex_file("giant.py")

    env = service.get_function("giant.py", "giant")
    row = env["results"][0]
    total = row["total_lines"]
    assert total > 2990
    # default: a bounded window, never the whole dump
    assert row["window"] == {"start": 1, "end": WINDOW}
    assert len(row["content"].splitlines()) == WINDOW
    assert env["truncated"] is True
    assert f"call get_function with start={WINDOW + 1}" in env["hint"]
    # the nested map is what makes the huge function navigable
    assert {n["qualname"] for n in row["nested"]} == {"giant.helper", "giant.Inner", "giant.Inner.twice"}
    # arbitrary interior slice — function-relative window bounds, absolute line numbers
    mid = service.get_function("giant.py", "giant", start=1500, end=1600)
    assert mid["results"][0]["window"] == {"start": 1500, "end": 1600}
    first_abs = int(mid["results"][0]["content"].splitlines()[0].split("\t")[0])
    assert first_abs == mid["results"][0]["start_line"] + 1500 - 1
    # explicit full read is still possible when genuinely wanted
    full = service.get_function("giant.py", "giant", start=1, end=total)
    assert full["truncated"] is False and len(full["results"][0]["content"].splitlines()) == total
    # window bounds are validated, not silently clamped into a lie
    past = service.get_function("giant.py", "giant", start=total + 5)
    assert past["error"]["kind"] == "validation"


def test_reads_serve_the_mirror_not_the_disk(service, project_root):
    """THE single-source contract: after cie writes a file, a foreign
    change to the bytes on disk does NOT leak into get_meta/get_function
    until reindex_file explicitly refreshes the mirror."""
    service.write_file("mirror.py", "def before() -> int:\n    return 1\n")
    (project_root / "mirror.py").write_text("def before() -> int:\n    return 2\n")  # bypass cie

    row = service.get_meta("mirror.py")["results"][0]
    assert row["total_lines"] == 2
    body = service.get_function("mirror.py", "before")["results"][0]
    assert "return 1" in body["content"]

    refreshed = service.reindex_file("mirror.py")
    assert refreshed["ok"], refreshed
    assert "return 2" in service.get_function("mirror.py", "before")["results"][0]["content"]


def test_apply_patch_moves_file_and_ast_as_one_unit(service):
    proposal = service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_LINE, "new_text": NEW_LINE}],
        test_id="test_charge",
    )
    pid = proposal["results"][0]["patch_id"]
    applied = service.apply_patch(pid)
    assert applied["ok"], applied

    # the mirror already carries the patched body — no disk read in between
    body = service.get_function("payment.py", "charge_payment")["results"][0]
    assert "timeout / 1000" in body["content"]
    assert "effective = timeout\n" not in body["content"]


def test_rejected_patch_touches_neither_file_nor_ast(service, project_root):
    """The realistic rejection: a patch proposed against content that a
    FOREIGN writer then changed on disk (Gate 1, PATCH_CONTEXT_MISMATCH).
    apply must leave BOTH the bytes and the mirror exactly as they were —
    the mirror keeps serving cie's last-known view, because apply's
    atomic write (and the mirror refresh attached to it) never ran."""
    assert service.get_meta("payment.py")["ok"]        # mirror now holds cie's view of SRC
    proposal = service.propose_patch(
        changes=[{"file": "payment.py", "old_text": OLD_LINE, "new_text": NEW_LINE}],
        test_id="test_stale",
    )
    pid = proposal["results"][0]["patch_id"]
    foreign = SRC.replace(OLD_LINE, "    effective = timeout * 60\n")
    (project_root / "payment.py").write_text(foreign)  # bypass cie between propose and apply

    rejected = service.apply_patch(pid)
    assert rejected["ok"] is False
    assert "old_text" in rejected["error"]["message"] or "match" in rejected["error"]["message"]

    # the file still holds the foreign bytes apply refused to touch
    assert (project_root / "payment.py").read_text() == foreign
    # the mirror still holds cie's view — apply never refreshed it
    body = service.get_function("payment.py", "charge_payment")["results"][0]
    assert "    effective = timeout\n" in body["content"]
    assert "timeout * 60" not in body["content"]


def test_edit_file_updates_mirror_in_same_call(service):
    service.write_file("small.py", "def one() -> int:\n    return 1\n")
    edited = service.edit_file("small.py", "return 1", "return 41")
    assert edited["ok"], edited
    body = service.get_function("small.py", "one")["results"][0]
    assert "return 41" in body["content"]
    # meta re-served from the refreshed snapshot too
    assert service.get_meta("small.py")["results"][0]["total_lines"] == 2


def test_delete_file_drops_the_mirror_entry(service):
    service.write_file("doomed.py", "def gone() -> None:\n    pass\n")
    assert service.get_meta("doomed.py")["ok"]
    deleted = service.delete_file("doomed.py")
    assert deleted["ok"], deleted
    gone = service.get_meta("doomed.py")
    assert gone["ok"] is False
    assert gone["error"]["kind"] == "not_found"


def test_non_python_file_is_honest_about_tier(service):
    env = service.get_meta("notes.md")
    assert env["ok"], env
    row = env["results"][0]
    assert row["file_type"] == "markdown" and row["tier"] == "text"
    assert row["functions"] == [] and row["function_count"] == 0
    assert "file_skeleton" in env["hint"]
    denied = service.get_function("notes.md", "anything")
    assert denied["ok"] is False
    assert denied["error"]["kind"] == "not_found"


def test_unparseable_file_degrades_without_lying(service, project_root):
    (project_root / "broken.py").write_text("def oops(:\n    pass\n")
    env = service.get_meta("broken.py")
    assert env["ok"], env
    row = env["results"][0]
    assert row["file_type"] == "python" and row["total_lines"] == 2
    assert row["functions"] == []
    assert "does not parse" in env["hint"]


def test_missing_file_is_not_found_and_jail_still_holds(service):
    miss = service.get_meta("ghost.py")
    assert miss["ok"] is False and miss["error"]["kind"] == "not_found"
    escape = service.get_meta("../outside.py")
    assert escape["ok"] is False
    assert "escapes root" in escape["error"]["message"]


def test_nested_definitions_are_counted_and_capped(service, project_root):
    # build a file with many nested defs inside one outer function
    lines = ["def outer() -> None:"]
    for i in range(NESTED_CAP + 5):
        lines += [f"    def inner_{i}() -> int:", f"        return {i}"]
    lines.append("    return None")
    (project_root / "nested.py").write_text("\n".join(lines) + "\n")
    service.reindex_file("nested.py")

    meta = service.get_meta("nested.py")["results"][0]
    assert meta["functions"][0]["qualname"] == "outer"

    env = service.get_function("nested.py", "outer")
    row = env["results"][0]
    assert row["nested_count"] == NESTED_CAP + 5
    assert len(row["nested"]) == NESTED_CAP
    assert env["hint"] and "nested map truncated" in env["hint"]
    # every nested def is addressable by qualified name
    one = service.get_function("nested.py", "outer.inner_3")["results"][0]
    assert one["qualname"] == "outer.inner_3" and "return 3" in one["content"]