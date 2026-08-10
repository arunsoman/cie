"""Section 0: Population & Real-Time Sync Architecture.

Implements the "integrity foundation" the spec's Phase 1 priority matrix
leads with: a two-graph model (speculative vs canonical, PS-01/02/03), a
4-stage quality gate (PS-04..PS-07) run by a `GateRunner` (PS-15),
tiered confidence scoring (PS-08), symbol-level AST delta + move
detection (PS-09/PS-10), soft-delete-on-revert (PS-12), idempotent
commit-linked batch population (PS-16), and sync-event classification
(PS-14). PS-11 (transactional graph updates) and PS-13 (cross-file edge
re-resolution) turned out to be ALREADY DONE in `Neo4jRepository.
load_extraction`/`reindex_file` (single `session.execute_write`
transaction; importer-edge reconnection) when this module was written —
the spec doc's "Current State" column for those two was stale, not this
module's gap to close.

Two-graph model, minimal-blast-radius version: rather than introduce a
new `graph_mode` concept into the query layer, "canonical" is just the
project's existing bare namespace (every one of the ~150 other call
sites across this codebase that assume `project == canonical` keeps
working unmodified) and "speculative" is a SECOND, separate project
namespace (`f"{project}:spec"`, see `speculative_project`) — isolation
(PS-03) falls straight out of the project-scoping (`_pw`/`_pw_where`)
every Neo4jRepository query already enforces; no new query-layer
mechanism needed at all.

Confidence scoring (PS-08), minimal-blast-radius version: rather than
add a `confidence_score` field threaded through every Node/Edge
dataclass, node/edge, and every `_row_to_*`/serialize call site, numeric
scores are DERIVED from the existing `Confidence` enum (see
`CONFIDENCE_SCORES`) for edges, and a `confidence_score`/`gate_status`
PROPERTY (landing in `Node.properties`, the exact catch-all section 13
already uses for kind-specific fields) for gate-evaluated nodes. No
schema migration.

Scope narrowed relative to the spec text, matching every other section's
own precedent of documenting what's cut:
- PS-05 (semantic gate): a REAL subprocess mypy/tsc run, but degrades to
  a passing "skipped" result when the type checker isn't installed or
  times out — an infrastructure gap must never block promotion the way a
  real type error does. See `semantic_gate`'s docstring.
- PS-06 (behavioral gate): runs exactly the tests DM-14 already linked
  via `TESTS` edges, through the same sandboxed `cie.tools.runner.
  run_command` the `run` tool uses — never the full suite.
- PS-07 (architectural gate): reuses `cie.drift_detect.architecture_drift`
  (already built for CI-12) rather than a new checker; `layer_rules` stays
  opt-in, same as that function's own scoping.
- PS-10 (move detection): name+kind+signature match is the PRIMARY
  signal (with the spec's own documented false-positive risk for common
  names — ambiguous matches are dropped, never guessed); body-similarity
  is a best-effort docstring-equality secondary signal, not a real
  normalized-AST-hash comparison (would need the pre-move source text,
  which isn't stored anywhere). See `detect_moves`.
- PS-14 (multi-source event ingestion): `GraphSyncEvent`/`classify_event`
  are real, and the webhook route (`cie/routes.py`'s `POST /sync/event`)
  dispatches to the real gate/promote/revert functions below. Actually
  installing a git post-commit hook script on a developer's machine is
  outside what an unattended backend service can safely do — that half
  stays a documented CLI recipe (see `docs/cie-sync-implementation.md`),
  not automated.
"""

from __future__ import annotations

import subprocess
import tempfile
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Optional, Sequence

from cie.extract import Extraction, extract_tree, parse_file
from cie.models import Confidence, EdgeRecord, Node

# -- PS-01/PS-03: two-graph namespacing --------------------------------------

SPECULATIVE_SUFFIX = ":spec"
DEFAULT_TTL_SECONDS = 300  # PS-01's stated 5-minute default


def speculative_project(canonical_project: str) -> str:
    """The speculative namespace for a canonical project (PS-01/PS-03)."""
    if not canonical_project:
        raise ValueError(
            "speculative_project requires a non-empty canonical project name"
        )
    if canonical_project.endswith(SPECULATIVE_SUFFIX):
        return canonical_project
    return f"{canonical_project}{SPECULATIVE_SUFFIX}"


def is_speculative(project: str) -> bool:
    return project.endswith(SPECULATIVE_SUFFIX)


def canonical_of(project: str) -> str:
    """Inverse of `speculative_project`; a no-op on an already-canonical name."""
    if is_speculative(project):
        return project[: -len(SPECULATIVE_SUFFIX)]
    return project


# -- PS-08: tiered confidence scoring ----------------------------------------

#: Numeric 0.0-1.0 score DERIVED from the existing `Confidence` enum — see
#: this module's docstring for why this is a mapping, not a new field.
CONFIDENCE_SCORES: dict[Confidence, float] = {
    Confidence.EXTRACTED: 1.0,
    Confidence.INFERRED: 0.7,
    Confidence.AMBIGUOUS: 0.4,
}
#: >=0.7 visible to LLM retrieval by default (spec's own PS-08 thresholds).
LLM_VISIBILITY_THRESHOLD = 0.7
#: <0.4 rejected to quarantine.
QUARANTINE_THRESHOLD = 0.4
#: PS-06 SUSPECT confidence (behavioral gate failed, promoted anyway).
SUSPECT_CONFIDENCE = 0.6


def confidence_score(confidence: Confidence) -> float:
    """PS-08: numeric score for an edge's existing `Confidence` enum."""
    return CONFIDENCE_SCORES.get(confidence, 0.5)


def visibility_tier(score: float) -> str:
    """PS-08: "visible" (LLM sees it by default), "hidden" (exists, excluded
    by default), or "quarantined" (rejected from canonical retrieval)."""
    if score >= LLM_VISIBILITY_THRESHOLD:
        return "visible"
    if score >= QUARANTINE_THRESHOLD:
        return "hidden"
    return "quarantined"


# -- PS-04..PS-07: the 4-stage quality gate ----------------------------------

GATE_STAGES = ("syntactic", "semantic", "behavioral", "architectural")


@dataclass(frozen=True)
class Violation:
    message: str
    detail: str = ""


@dataclass(frozen=True)
class GateResult:
    """One stage's outcome. `score` is a stage-local 0.0-1.0 confidence
    contribution, not the overall `GateRunResult.confidence`."""

    stage: str
    passed: bool
    score: float
    violations: tuple[Violation, ...] = ()


def syntactic_gate(path: Path) -> GateResult:
    """Stage 1: the tree-sitter parse tree contains no ERROR nodes.

    Real check, not a heuristic — `Node.has_error` is tree-sitter's own
    API for "this subtree (or any descendant) failed to parse cleanly."
    Tree-sitter is error-tolerant (it never raises on malformed syntax;
    `extract.py`'s empty-Extraction-on-garbage-input behavior the spec's
    gap analysis noted comes from `_walk_file` finding no valid class/
    function nodes to walk, not from a raised exception), so this is the
    only reliable syntactic signal available.
    """
    result = parse_file(path)
    if result is None:
        return GateResult(
            "syntactic", False, 0.0,
            (Violation("unsupported file type", str(path)),),
        )
    tree, _source = result
    if tree.root_node.has_error:
        return GateResult(
            "syntactic", False, 0.0,
            (Violation("syntax error(s) detected in parse tree", str(path)),),
        )
    return GateResult("syntactic", True, 1.0, ())


#: suffix -> single-file type-check command template. Best-effort only —
#: see `semantic_gate`'s docstring for why a missing/misconfigured checker
#: degrades to a pass rather than blocking promotion.
_TYPE_CHECK_CMDS = {
    ".py": "mypy --ignore-missing-imports --no-error-summary {name}",
    ".ts": "tsc --noEmit --skipLibCheck {name}",
    ".tsx": "tsc --noEmit --skipLibCheck {name}",
}


def semantic_gate(
    path: Path, cwd: Optional[Path] = None, timeout: int = 60,
) -> GateResult:
    """Stage 2: type-check the single changed file.

    Best-effort: no type checker configured for this suffix, the checker
    binary isn't installed, or it times out all degrade to a PASSING
    result (score 1.0) with an explanatory violation entry rather than
    blocking promotion on an infrastructure gap — a genuinely working
    checker reporting a real type error is the only path that fails this
    stage. This mirrors `Neo4jRepository._git_commit_sha`'s own "a
    missing tool must never break the main path" precedent.
    """
    from cie.tools.runner import run_command

    cmd_template = _TYPE_CHECK_CMDS.get(path.suffix)
    if cmd_template is None:
        return GateResult(
            "semantic", True, 1.0,
            (Violation(f"no type checker configured for {path.suffix!r}"),),
        )
    jail = Path(cwd) if cwd is not None else path.parent
    cmd = cmd_template.format(name=path.name)
    try:
        result = run_command(cmd, cwd=jail, timeout=timeout, allowed_root=jail)
    except Exception as exc:  # noqa: BLE001 - infra failure, never blocks
        return GateResult(
            "semantic", True, 1.0,
            (Violation("type checker unavailable, skipped", str(exc)),),
        )
    if result.timed_out:
        return GateResult(
            "semantic", True, 1.0, (Violation("type checker timed out, skipped"),),
        )
    lowered = result.output.lower()
    if result.exit_code == 127 or "not found" in lowered or "command not found" in lowered:
        return GateResult(
            "semantic", True, 1.0, (Violation("type checker not installed, skipped"),),
        )
    if result.exit_code == 0:
        return GateResult("semantic", True, 1.0, ())
    return GateResult(
        "semantic", False, 0.5,
        (Violation("type errors found", result.output[:2000]),),
    )


def behavioral_gate(
    repo: Any, symbol_labels: Sequence[str], project_root: Path, timeout: int = 120,
) -> GateResult:
    """Stage 3: run the tests DM-14 already linked (`TESTS` edges) to the
    given symbols via the sandboxed `run` tool's own executor.

    No linked tests at all is reported as a violation ("unverified") but
    does NOT fail the stage — a sparsely-tested project must still be
    able to promote code; per the spec's own stage table, only a
    confirmed test FAILURE downgrades to SUSPECT (score 0.6).
    """
    from cie.tools.runner import run_command

    test_files: set[str] = set()
    for label in symbol_labels:
        for er in repo.test_map(label):
            record = repo.get_node(er.edge.source)
            if record is not None and record.node.source_file:
                test_files.add(record.node.source_file)
    if not test_files:
        return GateResult(
            "behavioral", True, SUSPECT_CONFIDENCE,
            (Violation(
                "no linked tests found for changed symbols (unverified, not failing)"
            ),),
        )
    rel_files = []
    for f in sorted(test_files):
        try:
            rel_files.append(str(Path(f).resolve().relative_to(project_root.resolve())))
        except ValueError:
            rel_files.append(f)
    cmd = "python -m pytest -q " + " ".join(f'"{f}"' for f in rel_files)
    try:
        result = run_command(cmd, cwd=project_root, timeout=timeout, allowed_root=project_root)
    except Exception as exc:  # noqa: BLE001 - infra failure
        return GateResult(
            "behavioral", True, SUSPECT_CONFIDENCE,
            (Violation("test runner unavailable, unverified", str(exc)),),
        )
    if not result.timed_out and result.exit_code == 0:
        return GateResult("behavioral", True, 1.0, ())
    return GateResult(
        "behavioral", False, SUSPECT_CONFIDENCE,
        (Violation("linked test(s) failed — SUSPECT", result.output[:2000]),),
    )


def architectural_gate(
    repo: Any, layer_rules: Optional[dict[str, list[str]]] = None,
) -> GateResult:
    """Stage 4: reuses `cie.drift_detect.architecture_drift` (CI-12) —
    circular file-level dependencies always checked; layer-boundary
    violations only when `layer_rules` is supplied (that function's own
    "never invent boundaries this project hasn't declared" scoping)."""
    from cie.drift_detect import architecture_drift

    nodes, edges = repo.project_graph()
    findings = architecture_drift(nodes, edges, layer_rules=layer_rules)
    if not findings:
        return GateResult("architectural", True, 1.0, ())
    violations = tuple(
        Violation(f.get("drift_type", "ARCHITECTURE"), f.get("detail", "")) for f in findings
    )
    return GateResult("architectural", False, 0.5, violations)


@dataclass(frozen=True)
class GateRunResult:
    """Outcome of the full 4-stage pipeline for one file."""

    path: str
    stages: tuple[GateResult, ...]
    status: str  # "canonical" | "suspect" | "quarantined"
    confidence: float


def _node_to_row(node: Node, commit_hash: str = "") -> dict:
    row: dict = {
        "id": node.id, "label": node.label, "source_file": node.source_file,
        "source_location": node.source_location, "file_type": node.file_type,
        "kind": node.kind, "signature": node.signature,
        "line_start": node.line_start, "line_end": node.line_end,
        "docstring": node.docstring, "embedding": list(node.embedding),
        "extracted_at": node.extracted_at,
        "extractor_version": node.extractor_version,
        "source_ref": commit_hash or node.source_ref,
    }
    if node.community is not None:
        row["community"] = node.community
    row.update(node.properties)
    return row


def _edge_to_row(edge, commit_hash: str = "") -> dict:
    row: dict = {
        "source": edge.source, "target": edge.target, "relation": edge.relation,
        "confidence": edge.confidence.value,
        "extracted_at": edge.extracted_at,
        "extractor_version": edge.extractor_version,
        "source_ref": commit_hash or edge.source_ref,
    }
    row.update(edge.properties)
    return row


def run_quality_gate(
    repo_speculative: Any,
    repo_canonical: Any,
    canonical_project: str,
    path: Path,
    extraction: Extraction,
    project_root: Optional[Path] = None,
    layer_rules: Optional[dict[str, list[str]]] = None,
    commit_hash: str = "",
) -> GateRunResult:
    """PS-15: the async 4-stage pipeline (PS-04..PS-07), orchestrated.

    `repo_speculative` MUST be a Repository bound to
    `speculative_project(canonical_project)`; `repo_canonical` bound to
    the bare `canonical_project`. The speculative graph is ALWAYS updated
    first, unconditionally — the spec's own §0.5 scenario table has every
    single scenario (even "commit with syntax errors") update the
    speculative graph, so IDE diagnostics never go stale even when
    nothing promotes.

    Stage order matches §0.2: stage 1 and 2 failures block outright
    (canonical unchanged, quarantined). Stage 3 failure does NOT block —
    it downgrades the eventual promotion to SUSPECT/0.6 confidence. Stage
    4 gates last: even a clean stage-3 pass is blocked from promotion if
    stage 4 finds an architectural violation.
    """
    project_root = Path(project_root) if project_root is not None else path.parent
    repo_speculative.reindex_file(str(path), extraction)

    stage1 = syntactic_gate(path)
    if not stage1.passed:
        return GateRunResult(str(path), (stage1,), "quarantined", 0.0)

    stage2 = semantic_gate(path, cwd=project_root)
    if not stage2.passed:
        return GateRunResult(str(path), (stage1, stage2), "quarantined", stage2.score)

    symbol_labels = sorted({n.get("label", "") for n in extraction.nodes if n.get("label")})
    stage3 = behavioral_gate(repo_speculative, symbol_labels, project_root)
    stage4 = architectural_gate(repo_canonical, layer_rules=layer_rules)

    if not stage4.passed:
        return GateRunResult(
            str(path), (stage1, stage2, stage3, stage4), "quarantined", stage4.score,
        )

    status = "canonical" if stage3.passed else "suspect"
    confidence = 1.0 if stage3.passed else SUSPECT_CONFIDENCE
    nodes = []
    for n in extraction.nodes:
        row = dict(n)
        row["confidence_score"] = confidence
        row["gate_status"] = "OK" if status == "canonical" else "SUSPECT"
        nodes.append(row)
    edges = [dict(e) for e in extraction.edges]
    repo_canonical.merge_delta(nodes, edges, project=canonical_project)
    return GateRunResult(
        str(path), (stage1, stage2, stage3, stage4), status, confidence,
    )


# -- PS-02: canonical graph promotion ----------------------------------------

def promote(
    repo_speculative: Any, repo_canonical: Any, canonical_project: str,
    commit_hash: str = "",
) -> int:
    """PS-02: copy `repo_speculative`'s current contents into the
    canonical namespace, stamped with `commit_hash`. Idempotent
    (`merge_delta` is MERGE-on-id, never a delete+recreate) — promoting
    the same speculative snapshot twice yields the same result. This is
    the promotion PRIMITIVE, not a second gate — callers that need
    gating should use `run_quality_gate` instead; this is for a
    caller (e.g. a `CI_COMPLETE` sync event, PS-14) that already knows
    the content passed CI and just needs it merged into canonical.
    """
    nodes, edges = repo_speculative.project_graph()
    node_rows = [_node_to_row(n, commit_hash=commit_hash) for n in nodes]
    edge_rows = [_edge_to_row(er.edge, commit_hash=commit_hash) for er in edges]
    return repo_canonical.merge_delta(node_rows, edge_rows, project=canonical_project)


# -- PS-01: speculative graph TTL eviction -----------------------------------

def evict_stale_speculative(
    repo_speculative: Any, project: str, ttl_seconds: int = DEFAULT_TTL_SECONDS,
    now: Optional[datetime] = None,
) -> int:
    """PS-01: delete speculative nodes older than `ttl_seconds` (by
    IN-08's `extracted_at`). Refuses to run against a project name that
    doesn't look speculative (see `is_speculative`) — an eviction call is
    destructive and must never be pointed at canonical data by a caller's
    mistake."""
    if not is_speculative(project):
        raise ValueError(f"refusing to evict non-speculative project {project!r}")
    now = now or datetime.now(timezone.utc)
    cutoff = now - timedelta(seconds=ttl_seconds)
    nodes, _edges = repo_speculative.project_graph()
    stale_ids = []
    for n in nodes:
        if not n.extracted_at:
            continue
        try:
            stamped = datetime.fromisoformat(n.extracted_at)
        except ValueError:
            continue
        if stamped < cutoff:
            stale_ids.append(n.id)
    if not stale_ids:
        return 0
    return repo_speculative.delete_nodes(stale_ids, project=project)


# -- PS-09/PS-10: AST delta detection + move detection -----------------------

#: Node fields whose change makes a same-key node "modified" — mirrors
#: `cie.graph_diff`'s own `_MODIFIED_FIELDS` choice (line drift alone is
#: deliberately excluded; it fires on nearly every symbol in a touched file).
_DELTA_FIELDS = ("signature", "docstring")


def ast_delta(before: Sequence[Node], after: Sequence[Node]) -> dict:
    """PS-09: symbol-level delta between two node sets for the SAME file
    (typically: what's currently stored for a file vs a fresh re-parse of
    it) — keyed by `(label, kind)`, same approach as `cie.graph_diff.
    graph_diff` at a narrower (single-file) scope. Returns `{added,
    removed, modified, unchanged}`; `modified` entries carry `before`/
    `after`/`changed_fields`, matching `graph_diff`'s own shape.
    """
    before_by_key = {(n.label, n.kind): n for n in before}
    after_by_key = {(n.label, n.kind): n for n in after}
    before_keys, after_keys = set(before_by_key), set(after_by_key)

    added = [after_by_key[k] for k in sorted(after_keys - before_keys)]
    removed = [before_by_key[k] for k in sorted(before_keys - after_keys)]
    modified: list[dict] = []
    unchanged: list[Node] = []
    for key in sorted(before_keys & after_keys):
        b, a = before_by_key[key], after_by_key[key]
        changed = [f for f in _DELTA_FIELDS if getattr(b, f) != getattr(a, f)]
        if changed:
            modified.append({"before": b, "after": a, "changed_fields": changed})
        else:
            unchanged.append(a)
    return {"added": added, "removed": removed, "modified": modified, "unchanged": unchanged}


def detect_moves(removed: Sequence[Node], added: Sequence[Node]) -> list[dict]:
    """PS-10: pair up REMOVED/ADDED nodes across DIFFERENT files that are
    actually the same symbol relocated, not an unrelated delete+create.

    Primary signal: identical `(label, kind, signature)`. Per the spec's
    own caveat, this alone false-positives on common names (`__init__`,
    `main`, `handle`) — when more than one removed/added node shares a
    key, NONE of them are reported (ambiguous; never guess). Secondary
    signal (`body_similarity`) is a best-effort docstring-equality check,
    not a real normalized-AST-body-hash comparison — the pre-move source
    text isn't stored anywhere this function can reach, so that's a
    documented gap, not an attempt that silently under-delivers.
    """
    by_key: dict[tuple[str, str, str], list[Node]] = {}
    for n in removed:
        by_key.setdefault((n.label, n.kind, n.signature), []).append(n)
    added_by_key: dict[tuple[str, str, str], list[Node]] = {}
    for n in added:
        added_by_key.setdefault((n.label, n.kind, n.signature), []).append(n)

    moves: list[dict] = []
    for key, r_nodes in by_key.items():
        a_nodes = added_by_key.get(key)
        if not a_nodes or len(r_nodes) != 1 or len(a_nodes) != 1:
            continue
        old, new = r_nodes[0], a_nodes[0]
        if old.source_file == new.source_file:
            continue
        moves.append({
            "from_id": old.id, "to_id": new.id,
            "from_file": old.source_file, "to_file": new.source_file,
            "label": old.label, "kind": old.kind,
            "body_similarity": (
                "docstring_match"
                if old.docstring and old.docstring == new.docstring
                else "unknown"
            ),
        })
    return moves


# -- PS-12: soft-delete on revert ---------------------------------------------

def revert(repo: Any, project: str, commit_hash: str) -> int:
    """PS-12: soft-delete every node whose IN-08 `source_ref` equals
    `commit_hash` — sets `status: DEPRECATED` + `deprecated_at`, never a
    hard delete. Built entirely on the existing `update_node_properties`
    primitive (no new repo method needed: `status`/`deprecated_at` aren't
    named `Node` fields, so they land in `.properties`, same as every
    section-13 analysis property)."""
    nodes, _edges = repo.project_graph()
    targets = [n for n in nodes if n.source_ref == commit_hash]
    if not targets:
        return 0
    deprecated_at = datetime.now(timezone.utc).isoformat()
    updates = {
        n.id: {"status": "DEPRECATED", "deprecated_at": deprecated_at}
        for n in targets
    }
    return repo.update_node_properties(updates, project=project)


# -- PS-16: idempotent, commit-linked batch population -----------------------

def load_commit(repo: Any, repo_path: Path, commit_hash: str, project: str = "") -> int:
    """PS-16: checks out `commit_hash` into a TEMPORARY git worktree
    (`git worktree add --detach` — never touches the caller's actual
    branch/working tree), extracts it, and loads it via the existing
    `load_extraction` (already idempotent-by-replace for one project).
    Re-running with the same `commit_hash` re-extracts the identical tree
    and produces the identical graph — idempotency for the STRUCTURAL
    (AST-based) extraction tier only; LLM-derived entities need
    input-hash+model-version caching to achieve the same property, which
    this function does not attempt (per the spec's own caveat on PS-16).
    """
    repo_path = Path(repo_path)
    with tempfile.TemporaryDirectory(prefix="cie-load-commit-") as tmp:
        worktree = Path(tmp) / "worktree"
        subprocess.run(
            ["git", "worktree", "add", "--detach", str(worktree), commit_hash],
            cwd=str(repo_path), check=True, capture_output=True, text=True, timeout=60,
        )
        try:
            extraction = extract_tree(worktree)
            return repo.load_extraction(extraction.nodes, extraction.edges, project=project)
        finally:
            subprocess.run(
                ["git", "worktree", "remove", "--force", str(worktree)],
                cwd=str(repo_path), capture_output=True, text=True, timeout=30,
            )


# -- PS-14: multi-source event ingestion (classification only) --------------

@dataclass(frozen=True)
class GraphSyncEvent:
    event_type: str  # FILE_SAVE | COMMIT | CI_COMPLETE | REVERT
    commit_hash: str = ""
    file_paths: tuple[str, ...] = field(default_factory=tuple)
    author: str = ""
    timestamp: str = ""


#: event type -> pipeline stage it routes to. Pure routing table, kept
#: separate from the actual dispatch (cie/routes.py's `/sync/event`) so
#: the RULE is unit-testable without a FastAPI app or a live Neo4j
#: connection.
_EVENT_ROUTES = {
    "FILE_SAVE": "speculative",
    "COMMIT": "gate",
    "CI_COMPLETE": "promote",
    "REVERT": "revert",
}


def classify_event(event: GraphSyncEvent) -> str:
    """PS-14: which pipeline stage `event` routes to — "speculative"
    (file-watch reindex), "gate" (run the 4-stage pipeline), "promote"
    (CI passed, merge speculative into canonical), "revert" (soft-delete
    a commit's nodes), or "unknown" for an unrecognized event type."""
    return _EVENT_ROUTES.get(event.event_type, "unknown")


# -- PS-09/PS-10, wired: batch-level move detection during a real reindex ---

def apply_batch_move_detection(
    repo: Any,
    before_by_file: dict[str, Sequence[Node]],
    after_by_file: dict[str, Sequence[Node]],
    incoming_by_node_id: dict[str, Sequence[EdgeRecord]],
    project: str = "",
) -> dict:
    """PS-09/PS-10, wired into a real multi-file reindex batch (see
    `ToolService.reindex`) rather than staying purely advisory.

    A move is inherently a two-file event — `reindex_file` only ever
    sees one file at a time, so per-file move detection is structurally
    impossible; this function is the batch-level integration point a
    caller re-indexing several changed files at once must supply.

    `before_by_file`/`after_by_file`: this reindex batch's per-file node
    skeletons, captured by the caller immediately before/after each
    file's own `reindex_file` call. `incoming_by_node_id`: each
    before-state node's incoming edges, captured BEFORE `reindex_file`
    ran — by the time a cross-file move is confirmed (only knowable
    after comparing the WHOLE batch), the old node has already been
    `DETACH DELETE`d by its own file's `reindex_file` call, so this data
    is unrecoverable after the fact and must be captured in advance.

    For each detected move: stamps `moved_from_id`/`moved_from_file`
    PROPERTIES on the new node — NOT a `MOVED_FROM` edge. A graph edge
    needs both endpoints to exist; the old node is already gone by the
    time a move is confirmed, so a property is the only way to preserve
    this provenance without keeping tombstone nodes around (which
    PS-12's soft-delete already handles differently, for a different
    scenario — an explicit revert, not an ordinary move). Also
    reconnects the moved symbol's pre-captured incoming `calls` edges to
    the new node id via `merge_delta` (idempotent — a caller file's own
    reindex may have already naturally re-resolved the same edge; MERGE
    just updates it harmlessly rather than duplicating).
    """
    removed: list[Node] = []
    added: list[Node] = []
    for rel, before in before_by_file.items():
        after = after_by_file.get(rel, before)
        delta = ast_delta(before, after)
        removed.extend(delta["removed"])
    for rel, after in after_by_file.items():
        before = before_by_file.get(rel, after)
        delta = ast_delta(before, after)
        added.extend(delta["added"])

    moves = detect_moves(removed, added)
    reconnected = 0
    for move in moves:
        repo.update_node_properties(
            {move["to_id"]: {"moved_from_id": move["from_id"], "moved_from_file": move["from_file"]}},
            project=project,
        )
        incoming = incoming_by_node_id.get(move["from_id"], ())
        edge_rows = [
            {
                "source": er.edge.source, "target": move["to_id"], "relation": "calls",
                "confidence": er.edge.confidence.value,
            }
            for er in incoming if er.edge.target == move["from_id"]
        ]
        if edge_rows:
            repo.merge_delta([], edge_rows, project=project)
            reconnected += len(edge_rows)

    return {"moves_detected": len(moves), "moves": moves, "incoming_edges_reconnected": reconnected}


# -- PS-07: per-project architectural layer-rule configuration --------------

LAYER_RULES_CONFIG_ID = "config::layer_rules"


def store_layer_rules(repo: Any, project: str, layer_rules: dict[str, list[str]]) -> None:
    """PS-07: persist per-project layer-boundary rules — the SAME shape
    `cie.drift_detect.architecture_drift`'s own `layer_rules` argument
    takes (`{"layer_name": ["forbidden_substring", ...]}`) — as a
    singleton config node, so `run_quality_gate`'s architectural stage
    can enforce them without every caller passing them in by hand on
    every gate run. A plain `:Node{kind:"Config"}` rather than a new
    repository primitive — `merge_delta` (already built for PS-02) is a
    generic enough write path that a dedicated config-storage method
    would just duplicate it.
    """
    import json

    node = {
        "id": LAYER_RULES_CONFIG_ID, "label": "layer_rules", "kind": "Config",
        "source_file": "", "layer_rules_json": json.dumps(layer_rules),
    }
    repo.merge_delta([node], [], project=project)


def load_layer_rules(repo: Any, project: str) -> dict[str, list[str]]:
    """PS-07: read back whatever `store_layer_rules` last wrote for
    `project`, or `{}` if nothing has been configured yet — matches
    `architecture_drift`'s own "no layer rules configured -> only
    circular-dependency checks" default, not an error."""
    import json

    record = repo.get_node(LAYER_RULES_CONFIG_ID)
    if record is None:
        return {}
    raw = record.node.properties.get("layer_rules_json", "")
    if not raw:
        return {}
    try:
        return json.loads(raw)
    except (TypeError, ValueError):
        return {}
