"""Section 1: Core Data Model — DM-01/03/04/09/11/13.

DM-02 (schema flexibility), DM-08 (inheritance/interface edges), and
DM-14 (TESTS edges) were already done before this slice (DM-02 needed no
code — Neo4j is schema-optional by construction; DM-08/DM-14 shipped in
the grounding slice, see `cie-grounding-slice-implementation.md`).

Scope narrowed relative to the spec text, matching every other section's
own precedent:
- DM-01 (dual model support): only the RDF-export half is built here
  (`export_rdf`, Turtle only). A SPARQL endpoint and OWL reasoning are
  explicitly separate external systems per the spec's own split — not
  attempted.
- DM-03 (ontology layer): "is-a" semantics for code already exist via
  DM-08's `extends`/`implements` edges and `class_hierarchy` query — no
  new extraction needed for that half. What's new here is
  `inverse_relation` (a pure lookup: `calls` -> `called_by`, etc.) plus
  `related_edges`, a tool that annotates a symbol's neighbors with their
  inverse-labeled direction. Property constraints and full OWL inference
  (transitive closure, class subsumption) are not attempted — the spec's
  own P2 designation for that half.
- DM-05 (temporal versioning) is NOT in this module — `cie.graph_diff`
  (RQ-12) already documents why building real bi-temporal storage is a
  separate, much larger undertaking and routes around it with a pure
  before/after diff instead; that decision stands here too.
- DM-06 (multi-modal content), DM-07 (poly-hierarchical taxonomies beyond
  what DM-08 already gives), DM-10 (variable/constant tracking), and
  DM-12 (database/schema layer) are NOT attempted in this slice — see
  `cie-data-model-implementation.md` for the per-item reasoning.
- DM-09 (type edges): resolves parameter/return types from each
  FUNC/METHOD node's already-stored `signature` STRING (no new type
  inference — this reads what's already extracted). A type name that
  matches an existing CLASS node's label links there; anything else
  (builtins, unresolved names) gets a synthesized `type::<name>` stub
  node, mirroring DM-08's `external::<name>` convention exactly.
- DM-11 (external dependency graph): parses `pyproject.toml`/
  `package.json` into `PACKAGE` nodes. Does NOT link them to the specific
  files/call-sites that import them (`CALLS_EXTERNAL` edges) — that needs
  import data that isn't persisted anywhere post-extraction; a documented
  follow-up, not attempted here.
- DM-13 (documentation graph): one `DOCUMENT` node per markdown file
  (not per-chunk — chunking is IN-07's job, a separate item), `DESCRIBES`
  edges to code nodes whose label appears in the doc text as a whole
  word. Only a bounded excerpt (first 4000 chars) is stored on the node,
  not the full file — DM-06's `content_blobs` mechanism (not attempted)
  would be the real fix for large-content nodes; storing the whole file
  as a single Neo4j property is not that.
"""

from __future__ import annotations

import json
import re
import tomllib
from pathlib import Path
from typing import Optional, Sequence

from cie.models import Node, NodeKind

# -- DM-04: canonicalization / URI scheme ------------------------------------


def to_urn(node: Node) -> str:
    """A globally-unique, stable URI/URN for a node: `urn:protobox:
    {project}:{kind}:{id}`. `project` defaults to `"default"` when the
    node carries no project namespace, so the URN is still well-formed
    (an empty path segment `urn:protobox::function:x` is a malformed
    URN, not just an ugly one)."""
    project = node.project or "default"
    return f"urn:protobox:{project}:{node.kind}:{node.id}"


# -- DM-01: RDF export (Turtle) ----------------------------------------------

_TURTLE_PREFIX = "@prefix prb: <urn:protobox:predicate:> .\n\n"


def _turtle_literal(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")
    return f'"{escaped}"'


def export_rdf(nodes: Sequence[Node], edges, fmt: str = "turtle") -> str:
    """DM-01: serialize a (nodes, edges) slice as RDF Turtle for interop.
    `edges` is any iterable of objects with a `.edge` attribute exposing
    `source`/`target`/`relation` (an `EdgeRecord`, matching
    `Repository.project_graph`'s own return shape) OR a plain object with
    those three attributes directly — both are accepted so this can be
    called with either `project_graph()`'s edge list or a raw `Edge` list.
    Only `fmt="turtle"` is implemented; anything else raises `ValueError`
    rather than silently returning something else labeled as it.
    """
    if fmt != "turtle":
        raise ValueError(f"unsupported RDF export format: {fmt!r} (only 'turtle')")
    urn_by_id = {n.id: to_urn(n) for n in nodes}
    lines = [_TURTLE_PREFIX]
    for n in nodes:
        subject = f"<{to_urn(n)}>"
        lines.append(
            f"{subject} prb:label {_turtle_literal(n.label)} ;\n"
            f"    prb:kind {_turtle_literal(n.kind)} ;\n"
            f"    prb:sourceFile {_turtle_literal(n.source_file)} ."
        )
    for er in edges:
        edge = getattr(er, "edge", er)
        src_urn = urn_by_id.get(edge.source)
        tgt_urn = urn_by_id.get(edge.target)
        if not src_urn or not tgt_urn:
            continue
        predicate = re.sub(r"[^a-zA-Z0-9_]", "_", edge.relation or "relates")
        lines.append(f"<{src_urn}> prb:{predicate} <{tgt_urn}> .")
    return "\n".join(lines) + "\n"


# -- DM-03: ontology — inverse relations --------------------------------------

#: Known relation -> its inverse label. Anything not listed here has no
#: known inverse (`inverse_relation` returns None, not a guess).
INVERSE_RELATIONS: dict[str, str] = {
    "calls": "called_by",
    "called_by": "calls",
    "extends": "extended_by",
    "extended_by": "extends",
    "implements": "implemented_by",
    "implemented_by": "implements",
    "contains": "contained_by",
    "contained_by": "contains",
    "TESTS": "tested_by",
    "tested_by": "TESTS",
    "DESCRIBES": "described_by",
    "described_by": "DESCRIBES",
    "TYPE_OF": "type_used_by",
    "type_used_by": "TYPE_OF",
    "DEPENDS_ON": "dependency_of",
    "dependency_of": "DEPENDS_ON",
}


def inverse_relation(relation: str) -> Optional[str]:
    """DM-03: the inverse label of a relation, or None if unknown."""
    return INVERSE_RELATIONS.get(relation)


def annotate_inverse(edge_records: Sequence) -> list[dict]:
    """DM-03: `[{edge_record, inverse}]` — each of `edge_records`
    (`EdgeRecord`s, e.g. from `get_neighbors`) paired with its relation's
    inverse label (or None). Used by the `related_edges` tool so a caller
    sees "calls (inverse: called_by)" instead of a bare relation string."""
    return [
        {"edge_record": er, "inverse": inverse_relation(er.edge.relation)}
        for er in edge_records
    ]


# -- DM-09: type / data-flow edges --------------------------------------------

#: Built-in/generic type names that never get a real CLASS lookup or a
#: stub node — they're too common to be a useful graph edge target and
#: would otherwise create a giant fan-in hub for "str"/"int"/etc.
_BUILTIN_TYPE_NAMES = frozenset({
    "str", "int", "float", "bool", "bytes", "list", "dict", "tuple", "set",
    "frozenset", "None", "Any", "object", "self", "cls",
    "string", "number", "boolean", "void", "unknown", "never", "any",
})


def _parse_signature_types(signature: str) -> tuple[str, list[tuple[str, str]]]:
    """Return `(return_type, [(param_name, param_type), ...])` parsed
    from a signature string like `foo(a: int, b: str = "x") -> User`.
    Best-effort regex/string parsing over what `extract.py` already
    stored — no re-parsing of source, no type inference beyond what the
    signature text itself states."""
    return_type = ""
    arrow = signature.rfind("->")
    if arrow != -1:
        return_type = signature[arrow + 2:].strip()
    start = signature.find("(")
    end = signature.rfind(")")
    params: list[tuple[str, str]] = []
    if start != -1 and end != -1 and end > start:
        inner = signature[start + 1:end]
        for raw in inner.split(","):
            raw = raw.strip()
            if not raw or ":" not in raw:
                continue
            name_part, _, type_part = raw.partition(":")
            name = name_part.strip()
            type_part = type_part.split("=")[0].strip()
            if name and type_part:
                params.append((name, type_part))
    return return_type, params


def _clean_type_name(raw: str) -> str:
    """Strip a generic wrapper (`Optional[User]` -> `User`, `List[Foo]`
    -> `Foo`) down to the innermost bare type name. Deliberately shallow
    — a `Dict[str, User]` returns `User` (the last/innermost segment,
    not a full multi-arg breakdown); good enough to link the interesting
    domain type without a real generic-type parser."""
    name = raw.strip()
    while "[" in name and name.endswith("]"):
        name = name[name.find("[") + 1: -1].strip()
        if "," in name:
            name = name.split(",")[-1].strip()
    return name.strip("'\" ")


def resolve_type_edges(nodes: Sequence[Node]) -> tuple[list[dict], list[dict]]:
    """DM-09: for every FUNC/METHOD node's `signature`, resolve its
    return type and parameter types into `TYPE_OF` edges. A type name
    matching an existing CLASS node's label links there (EXTRACTED
    confidence — resolved against a real node in this project); anything
    else gets a synthesized `type::<name>` stub node (AMBIGUOUS
    confidence), same "never drop, stub instead" contract as DM-08's
    `external::` nodes. Builtins (`str`/`int`/...) are skipped entirely —
    see `_BUILTIN_TYPE_NAMES`. Returns `(type_stub_nodes, type_edges)`.
    """
    class_ids_by_label: dict[str, str] = {
        n.label: n.id for n in nodes if n.kind == NodeKind.CLASS.value
    }
    known_ids = {n.id for n in nodes}
    stub_nodes: dict[str, dict] = {}
    edges: list[dict] = []

    for n in nodes:
        if n.kind not in (NodeKind.FUNC.value, NodeKind.METHOD.value):
            continue
        if not n.signature:
            continue
        return_type, params = _parse_signature_types(n.signature)
        candidates: list[tuple[str, str, str]] = []  # (raw_type, role, param_name)
        if return_type:
            candidates.append((return_type, "return", ""))
        for pname, ptype in params:
            candidates.append((ptype, "param", pname))

        for raw_type, role, pname in candidates:
            type_name = _clean_type_name(raw_type)
            if not type_name or type_name in _BUILTIN_TYPE_NAMES:
                continue
            target_id = class_ids_by_label.get(type_name)
            confidence = "EXTRACTED"
            if target_id is None:
                target_id = f"type::{type_name}"
                confidence = "AMBIGUOUS"
                if target_id not in known_ids and target_id not in stub_nodes:
                    stub_nodes[target_id] = {
                        "id": target_id, "label": type_name,
                        "kind": NodeKind.TYPE.value, "source_file": "",
                        "signature": "", "docstring": "",
                    }
            edge = {
                "source": n.id, "target": target_id, "relation": "TYPE_OF",
                "confidence": confidence, "role": role,
            }
            if pname:
                edge["param_name"] = pname
            edges.append(edge)

    return list(stub_nodes.values()), edges


# -- DM-11: external dependency graph -----------------------------------------

def _parse_pep508_name(spec: str) -> str:
    """`"fastapi>=0.100,<1.0"` -> `"fastapi"`; `"fastapi[standard]"` ->
    `"fastapi"` — the package name is everything before the first
    version-specifier or extras-bracket character."""
    match = re.match(r"^[A-Za-z0-9_.\-]+", spec.strip())
    return match.group(0) if match else spec.strip()


def resolve_package_nodes(repo_root: Path) -> tuple[list[dict], list[dict]]:
    """DM-11: parse `pyproject.toml` and `package.json` (whichever exist
    under `repo_root`) into `PACKAGE` nodes, one per declared dependency,
    tagged with its ecosystem (`pypi`/`npm`) and manifest section
    (`dependency`/`dev`). No edges — linking a package to the specific
    files that import it needs import data this pass doesn't have access
    to post-extraction (see this module's docstring). Returns
    `(package_nodes, [])`; the empty edge list is returned for shape
    consistency with `resolve_type_edges`/`resolve_document_nodes`, not
    because edges are coming later in this same call.
    """
    packages: dict[str, dict] = {}

    pyproject = repo_root / "pyproject.toml"
    if pyproject.is_file():
        try:
            data = tomllib.loads(pyproject.read_text())
        except Exception:  # noqa: BLE001 - malformed manifest, skip it
            data = {}
        deps = data.get("project", {}).get("dependencies", [])
        for spec in deps:
            name = _parse_pep508_name(spec)
            if name:
                packages[f"package::pypi::{name}"] = {
                    "id": f"package::pypi::{name}", "label": name,
                    "kind": NodeKind.PACKAGE.value, "source_file": str(pyproject),
                    "ecosystem": "pypi", "section": "dependency", "spec": spec,
                }
        poetry_deps = (
            data.get("tool", {}).get("poetry", {}).get("dependencies", {})
        )
        for name in poetry_deps:
            if name.lower() == "python":
                continue
            packages[f"package::pypi::{name}"] = {
                "id": f"package::pypi::{name}", "label": name,
                "kind": NodeKind.PACKAGE.value, "source_file": str(pyproject),
                "ecosystem": "pypi", "section": "dependency", "spec": str(poetry_deps[name]),
            }

    package_json = repo_root / "package.json"
    if package_json.is_file():
        try:
            data = json.loads(package_json.read_text())
        except Exception:  # noqa: BLE001 - malformed manifest, skip it
            data = {}
        for section, key in (("dependency", "dependencies"), ("dev", "devDependencies")):
            for name, version in (data.get(key) or {}).items():
                packages[f"package::npm::{name}"] = {
                    "id": f"package::npm::{name}", "label": name,
                    "kind": NodeKind.PACKAGE.value, "source_file": str(package_json),
                    "ecosystem": "npm", "section": section, "spec": str(version),
                }

    return list(packages.values()), []


# -- DM-13: documentation / concept graph -------------------------------------

_MD_GLOB = "*.md"
_EXCLUDED_DIRS = {"__pycache__", "node_modules", ".git", ".venv", "venv"}
_MAX_EXCERPT_CHARS = 4000
_MAX_MENTIONS_PER_DOC = 50


def resolve_document_nodes(
    repo_root: Path, nodes: Sequence[Node],
) -> tuple[list[dict], list[dict]]:
    """DM-13: one `DOCUMENT` node per markdown file under `repo_root`
    (skipping the usual noise directories), `DESCRIBES` edges to any
    code node whose label appears in the doc text as a whole word (regex
    word-boundary match, case-sensitive — code identifiers are
    case-sensitive by nature; a case-insensitive match would false-hit
    on common English words that happen to share a short identifier's
    spelling). Only labels of 3+ characters are matched (shorter labels
    are far too common in prose to be a meaningful signal), and mentions
    are capped at `_MAX_MENTIONS_PER_DOC` per document to avoid an
    unbounded fan-out from a doc that happens to reference nearly every
    symbol in the project (e.g. an API reference page).
    """
    label_to_ids: dict[str, list[str]] = {}
    for n in nodes:
        if n.kind in (NodeKind.FUNC.value, NodeKind.METHOD.value, NodeKind.CLASS.value):
            if len(n.label) >= 3:
                label_to_ids.setdefault(n.label, []).append(n.id)

    doc_nodes: list[dict] = []
    edges: list[dict] = []
    for path in sorted(repo_root.rglob(_MD_GLOB)):
        if any(part in _EXCLUDED_DIRS or part.startswith(".") for part in path.parts):
            continue
        try:
            text = path.read_text(errors="replace")
        except OSError:
            continue
        rel = str(path.relative_to(repo_root))
        doc_id = f"document::{rel}"
        headings = re.findall(r"^#{1,6}\s+(.+)$", text, flags=re.MULTILINE)
        doc_nodes.append({
            "id": doc_id, "label": path.name, "kind": NodeKind.DOCUMENT.value,
            "source_file": str(path), "docstring": text[:_MAX_EXCERPT_CHARS],
            "heading_count": len(headings),
        })
        mentioned = 0
        for label, target_ids in label_to_ids.items():
            if mentioned >= _MAX_MENTIONS_PER_DOC:
                break
            if re.search(rf"\b{re.escape(label)}\b", text):
                for target_id in target_ids[:1]:  # one edge per distinct label
                    edges.append({
                        "source": doc_id, "target": target_id, "relation": "DESCRIBES",
                        "confidence": "INFERRED",
                    })
                mentioned += 1

    return doc_nodes, edges
