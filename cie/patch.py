"""Repair transaction layer — the propose/apply/verify patch protocol.

This module is the pure core of three ToolService tools that deliberately
separate *reasoning* from *mutation*:

    propose_patch = "what should change?"            (never mutates files)
    apply_patch   = "can this change safely land?"   (the ONLY file mutation)
    verify_patch  = "did it fix the thing, unbroken?"(never mutates files)

The separation has one crucial consequence for an autonomous repair loop:
**the component that proposes a fix does not get to declare its own fix
correct.** The proposer can only construct a durable, auditable, IMMUTABLE
`PatchPlan`; a separate tool decides whether it may be applied; a third
decides — from evidence, not from the proposer's optimism — whether the
applied result actually fixed the failure.

A patch is therefore a first-class object, not a side effect of edits. Its
`changes` list carries exact `old_text`->`new_text` string semantics (the
same mutation primitive `edit_file` uses downstream) PLUS a computed
unified diff, the file-content hash the proposal was made against, the
diagnosis, evidence, intended behavior, a counterfactual, blast-radius
impact, and provenance. `apply_patch` re-validates every one of those
assumptions against CURRENT disk state before writing a byte, and
application is all-or-nothing (`cie.tools.edit.write_files_atomic`
snapshot + rollback underneath — a half-patched repository is never a
possible end state).

Immutability: a plan's content (changes/diagnosis/impact/risk) is fixed at
proposal. Only lifecycle properties move — status
PROPOSED -> APPLIED -> VERIFIED|FAILED, REJECTED (terminal, apply-time
gate failure), SUPERSEDED (a newer proposal for the same failing test).
The FAILED -> FAILED -> VERIFIED chain that accumulates across proposals
IS the repair history (first-patch success rate, patches-per-bug) —
queryable read-only via `list_patches`/`get_patch`.

Every function here is pure (no filesystem, no graph I/O); the
`ToolService` methods in `cie/tools` own all side effects and the
standard SPEC §0 envelopes.
"""

from __future__ import annotations

import ast
import hashlib
from typing import Any, Optional

#: Patch lifecycle statuses (the only mutable part of a persisted plan).
STATUS_PROPOSED = "PROPOSED"
STATUS_APPLIED = "APPLIED"
STATUS_VERIFIED = "VERIFIED"
STATUS_FAILED = "FAILED"
STATUS_REJECTED = "REJECTED"
STATUS_SUPERSEDED = "SUPERSEDED"

#: Statuses `apply_patch` will still act on. A REJECTED patch is terminal:
#: its context was proven violated — the fix is to propose a NEW patch
#: against current content, not to smuggle new changes into the old id.
APPLIABLE_STATUSES = frozenset({STATUS_PROPOSED})

#: Change operations. `edit` replaces the (unique) occurrence of `old_text`
#: with `new_text`; `create` writes `new_text` as a NEW file (old_text must
#: be empty and the file must not exist at apply time).
OP_EDIT = "edit"
OP_CREATE = "create"
_OPERATIONS = (OP_EDIT, OP_CREATE)

#: Patch node id prefix, mirroring `verdict::` / `clone::` ids elsewhere
#: (analysis nodes carry type-prefixed ids, not UUIDs, so ids are
#: self-describing in any query surface).
PATCH_ID_PREFIX = "patch::"


def patch_id_for(changes: list[dict], created_at: str) -> str:
    """Stable `patch::<sha16>` id over the change set + proposal timestamp.

    Content-addressed over the semantic bytes (file/operation/old/new), not
    the diff formatting — and salted with `created_at` so two genuinely
    identical proposals in one session still get distinct ids (a re-proposal
    is a new lifecycle event, superseding the first, not a silent no-op).
    """
    canon = "\x1f".join(
        f"{c.get('file', '')}\x1e{c.get('operation', '')}"
        f"\x1e{c.get('old_text', '')}\x1e{c.get('new_text', '')}"
        for c in changes
    )
    return PATCH_ID_PREFIX + hashlib.sha256(
        f"{canon}\x1d{created_at}".encode()
    ).hexdigest()[:16]


def validate_change(change: Any) -> Optional[str]:
    """Structural validation of one `propose_patch(changes=...)` entry.

    Returns an error message, or None when the change is well-formed.
    Strictness here is the point: a malformed proposal must fail at
    PROPOSE time, never turn into a mid-apply surprise.
    """
    if not isinstance(change, dict):
        return "each change must be an object with a 'file' field"
    file = change.get("file", "")
    if not isinstance(file, str) or not file.strip():
        return "each change needs a non-empty 'file'"
    old_text = change.get("old_text", "")
    new_text = change.get("new_text", "")
    if not isinstance(old_text, str) or not isinstance(new_text, str):
        return f"change for {file!r}: old_text/new_text must be strings"
    op = change.get("operation") or (OP_EDIT if old_text else OP_CREATE)
    if op not in _OPERATIONS:
        return f"change for {file!r}: operation must be one of {_OPERATIONS!r}"
    if op == OP_CREATE and old_text:
        return f"change for {file!r}: a 'create' change carries new_text only"
    if old_text == new_text:
        return f"change for {file!r}: old_text and new_text are identical"
    if op == OP_EDIT and not old_text:
        return f"change for {file!r}: an 'edit' change needs old_text"
    return None


def check_change_context(content: str, old_text: str) -> Optional[str]:
    """Apply-time context gate (Gate 1): does `old_text` still match the
    CURRENT on-disk content, exactly once?

    Returns None when the change may proceed, else a `PATCH_CONTEXT_MISMATCH`
    rejection reason quoting expected vs found. This is the single most
    important apply-time protection: a patch proposed against revision A is
    rejected — loudly, not silently — when landed on revision B.
    """
    count = content.count(old_text)
    if count == 0:
        expected = [ln.strip() for ln in old_text.strip().splitlines() if ln.strip()]
        found = [ln.strip() for ln in content.splitlines() if ln.strip()]
        return (
            "PATCH_CONTEXT_MISMATCH: old_text no longer in the file. "
            f"expected: {expected[:4]}; file head: {found[:4]}. "
            "The file changed since propose_patch — re-propose against "
            "current content; the stale proposal was rejected."
        )
    if count > 1:
        return (
            f"PATCH_CONTEXT_MISMATCH: old_text matches {count} locations — "
            "add surrounding context to make it unique. Rejected rather than "
            "guessing which occurrence you meant."
        )
    return None


def compute_new_content(content: str, old_text: str, new_text: str) -> str:
    """One structured edit in-memory (no I/O; the write happens only after
    EVERY gate passes). `check_change_context` guarantees uniqueness."""
    return content.replace(old_text, new_text, 1)


def net_new_imports(old_text: str, new_text: str) -> list[str]:
    """Import lines in `new_text` absent from `old_text` — input to
    `verify_patch`'s imports check. Unresolved imports are reported as
    WARNINGS there, never failures: third-party imports legitimately don't
    resolve against a project-local symbol index."""
    old_lines = set(old_text.splitlines())
    out: list[str] = []
    for line in new_text.splitlines():
        stripped = line.strip()
        if not ((stripped.startswith("import ") or stripped.startswith("from "))):
            continue
        if stripped in old_lines or stripped in out:
            continue
        out.append(stripped)
    return out


def syntax_check(path: str, content: str) -> dict:
    """Does `content` still parse? Python: real check via `ast.parse`.
    Every other language: honestly `None` (skipped) — cie's tree-sitter
    parsers are error-TOLERANT, so a clean parse there is not evidence of
    validity, and a false "valid" would be worse than a known hole."""
    suffix = "." + path.rsplit(".", 1)[-1].lower() if "." in path else ""
    if suffix == ".py":
        try:
            ast.parse(content)
        except SyntaxError as exc:
            return {"language": "python", "valid": False,
                    "detail": f"SyntaxError: {exc.msg} (line {exc.lineno})"}
        return {"language": "python", "valid": True, "detail": "ast.parse ok"}
    return {
        "language": suffix.lstrip(".") or "unknown",
        "valid": None,
        "detail": "no exact syntax validator for this extension; "
                  "run_tests is the authoritative check",
    }


def auto_risk(changes: list[dict], caller_count: int) -> dict:
    """Heuristic blast-radius risk when the proposer supplies no risk_level.

    Clearly labelled heuristic, always overridable: HIGH once the call
    graph shows >5 known callers of changed symbols or 3+ files move;
    MEDIUM for any known callers / 2 files; otherwise LOW.
    """
    files = len({c.get("file", "") for c in changes}) if changes else 0
    if files >= 3 or caller_count > 5:
        level = "HIGH"
    elif files >= 2 or caller_count > 0:
        level = "MEDIUM"
    else:
        level = "LOW"
    return {
        "level": level,
        "reason": (
            f"heuristic: {len(changes)} change(s) across {files} file(s), "
            f"{caller_count} known caller(s) of changed symbols"
        ),
        "source": "heuristic",
    }


def scope_of(allowed_files: Optional[list[str]], changes: list[dict]) -> Optional[str]:
    """Scope gate (Gate 4): every change's file must be inside the
    proposal's `allowed_files` allowlist it declared for itself. Returns a
    rejection reason or None. This is the enforcement behind
    `PATCH_SCOPE_VIOLATION`: a patch that reaches beyond the files its own
    diagnosis declared is a different patch than the one reviewed, and it
    gets rejected rather than applied."""
    if not allowed_files:
        return None
    declared = set(allowed_files)
    stray = sorted({c.get("file", "") for c in changes} - declared)
    if stray:
        return (
            f"PATCH_SCOPE_VIOLATION: patch reaches files outside its own "
            f"declared scope {sorted(declared)}: {stray}. Propose a new patch "
            "whose scope covers everything it actually changes."
        )
    return None


def content_sha(content: str) -> str:
    """sha256 of one file's text — the context/post-apply anchors stored on
    the plan (context_sha256 at propose time, post_sha256 at apply time)."""
    return hashlib.sha256(content.encode()).hexdigest()


def build_patch_plan(
    *,
    changes: list[dict],
    test_id: str = "",
    root_cause: str = "",
    confidence: Optional[float] = None,
    evidence: Optional[list[str]] = None,
    intended_behavior: str = "",
    expected_failure: str = "",
    after_patch: str = "",
    constraints: Optional[list[str]] = None,
    allowed_files: Optional[list[str]] = None,
    risk: Optional[dict] = None,
    impact: Optional[dict] = None,
    agent: str = "",
    model: str = "",
    repository_revision: str = "",
    created_at: str = "",
) -> dict:
    """Assemble the complete, serializable PatchPlan (one source of truth
    for the node that gets persisted and every tool response that reads it
    back — the plan is stored verbatim, so what was proposed is exactly
    what any later call audits).

    Pure: enrichment (diffs, context hashes, impact) is the calling
    ToolService's job — see `ToolService.propose_patch`.
    """
    structured = []
    for c in changes:
        op = c.get("operation") or (OP_EDIT if c.get("old_text") else OP_CREATE)
        structured.append({
            "file": c["file"],
            "symbol": c.get("symbol", ""),
            "operation": op,
            "old_text": c.get("old_text", ""),
            "new_text": c.get("new_text", ""),
            "diff": c.get("diff", ""),
            "context_sha256": c.get("context_sha256", ""),
        })
    return {
        "patch_id": patch_id_for(structured, created_at),
        "status": STATUS_PROPOSED,
        "created_at": created_at,
        "trigger": {"test_id": test_id, "failure_id": "", "reproduction": ""},
        "diagnosis": {
            "root_cause": root_cause,
            "confidence": confidence,
            "evidence": list(evidence or []),
        },
        "intent": {
            "intended_behavior": intended_behavior,
            "counterfactual": {
                "expected_failure": expected_failure,
                "after_patch": after_patch,
            },
            "constraints": list(constraints or []),
            "allowed_files": list(allowed_files) if allowed_files else [],
        },
        "changes": structured,
        "impact": dict(impact or {
            "affected_symbols": [],
            "affected_files": sorted({c["file"] for c in structured}),
            "callers": [],
            "affected_tests": [],
        }),
        "validation_plan": {
            "failing_test": test_id,
            "regression_tests": (impact or {}).get("affected_tests", []),
            "build": "project test suite",
            "static_checks": ["syntax", "imports_resolvable"],
        },
        "risk": risk or {"level": "", "reason": "", "source": "unset"},
        "provenance": {
            "agent": agent,
            "model": model,
            "repository_revision": repository_revision,
        },
    }


def build_patch_node(
    plan: dict,
    file_node_ids: Optional[dict[str, str]] = None,
    test_node_id: str = "",
) -> tuple[dict, list[dict]]:
    """One `PatchPlan` analysis node (the whole plan lives in `properties`,
    MERGE-on-id) + navigation edges from the patch to nodes that ACTUALLY
    exist in the graph: `PATCH_PROPOSED_FOR` → the failing test's node and
    `PATCHES` → each changed file's FILE node. The caller (ToolService)
    resolves real node ids first; patches against an un-indexed project
    simply get fewer edges, never a dangling one."""
    file_node_ids = file_node_ids or {}
    changes = plan.get("changes", [])
    files_preview = ", ".join(c["file"] for c in changes[:3])
    suffix = f" (+{len(changes) - 3})" if len(changes) > 3 else ""
    node = {
        "id": plan["patch_id"],
        "label": f"patch {plan['status'].lower()}: {files_preview}{suffix}",
        "kind": "PatchPlan",
        "source_file": (changes[0]["file"] if changes else ""),
        "status": plan["status"],
        "created_at": plan.get("created_at", ""),
        "test_id": (plan.get("trigger") or {}).get("test_id", ""),
        "risk_level": (plan.get("risk") or {}).get("level", ""),
        "plan": plan,  # full snapshot — properties["plan"] is THE plan
    }
    edges: list[dict] = []
    if test_node_id:
        edges.append({
            "source": plan["patch_id"], "target": test_node_id,
            "relation": "PATCH_PROPOSED_FOR", "confidence": "EXTRACTED",
        })
    for file_path in sorted({c["file"] for c in changes}):
        node_id = file_node_ids.get(file_path)
        if not node_id:
            continue  # FILE node not in the graph — skip, don't dangle
        edges.append({
            "source": plan["patch_id"], "target": node_id,
            "relation": "PATCHES", "confidence": "EXTRACTED",
        })
    return node, edges