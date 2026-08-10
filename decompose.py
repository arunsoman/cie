"""Section 15: Decomposition Engine (spec sections 15.1-15.2).

Reuses TWO existing extractors rather than rebuilding them (per CLAUDE.md's
"grep features/ for existing behavior under a different name" rule — both
were found already built, just not wired into the graph):

- HTML tree-walking: `features.design_studio.layout_fingerprint.
  MockupHTMLWalker` (stdlib `html.parser`, no bs4/lxml dependency) — the
  same walker `html_to_jsx.py` and `interactive_elements.py` already use,
  so this module doesn't write a fourth divergent HTML parser.
- Interactive element detection (DE-02) — ALREADY FULLY IMPLEMENTED at
  `features.design_studio.interactive_elements.detect_interactive_elements`
  (table/chart/form/list_grid/map_calendar kinds, `derived_task_hints`
  per kind — exactly what DE-02 itself asks for). `extract_elements`
  below is a thin GRAPH-WRITING wrapper around that existing function,
  not a second implementation. The spec doc's "Missing" for DE-02 was
  stale — this codebase already had it under `features/design_studio/`,
  not `cie/decompose/`.

DE-04's `DERIVES_TASK` edges point to a lightweight `DerivedTaskHint`
:Node stand-in, NOT a real `:AtomicTask` — `AtomicTask` lives in a
separate schema/lifecycle (`cie.task_repository`, pushed via
`tasks:push`, with its own status/dependency/artifact model) that this
on-demand, exploratory decomposition pass should not silently write
into. Wiring a decomposition-engine-generated hint into that real
lifecycle (turning a hint into an actual pushable AtomicTask, prose
filled in by the Requirement-Miner per the spec's own framing) is a
separate, later integration.
"""

from __future__ import annotations

import hashlib
import re
from typing import Sequence

from features.design_studio.interactive_elements import detect_interactive_elements
from features.design_studio.layout_fingerprint import MockupHTMLWalker

from cie.models import EdgeRecord, Node, NodeKind

#: A small vocabulary of icon-button class/aria-label substrings that
#: imply a destination page — settings gear, profile avatar, etc. Not
#: exhaustive by design (see DE-01's own "icon-button routes" example);
#: expanding this list is the natural way to widen coverage, not a
#: structural change.
_ICON_ROUTE_HINTS: dict[str, str] = {
    "settings": "Settings", "profile": "Profile", "account": "Account",
    "notifications": "Notifications", "help": "Help", "search": "Search",
    "cart": "Cart", "logout": "Logout",
}


def _stable_id(*parts: str) -> str:
    return hashlib.sha256("::".join(parts).encode()).hexdigest()[:16]


def _page_name_from_href(href: str) -> str:
    """`"/orders/list"` -> `"List"`; `"/"` -> `"Home"`; `"#"` or a
    JS no-op href -> `""` (not a real navigable destination)."""
    href = href.strip()
    if not href or href in ("#", "javascript:void(0)", "javascript:;"):
        return ""
    href = href.split("?")[0].split("#")[0].strip("/")
    if not href:
        return "Home"
    last_segment = href.split("/")[-1]
    words = re.split(r"[-_]+", last_segment)
    return " ".join(w.capitalize() for w in words if w)


# -- DE-01: Page Extractor ----------------------------------------------------

def extract_pages(html: str, screen_id: str) -> tuple[list[dict], list[dict]]:
    """DE-01: deterministic scan for navigable destinations — `<a href>`,
    icon-button routes (matched against `_ICON_ROUTE_HINTS`). Nav-bar/
    footer/sidebar links are just `<a>` tags in this markup model, so no
    separate handling is needed for them. Returns `(page_nodes,
    rendered_on_edges)`; ids are content-hashed, so re-extracting the
    same HTML is idempotent.
    """
    walker = MockupHTMLWalker()
    walker.feed(html)

    pages: dict[str, dict] = {}
    edges: list[dict] = []
    for _depth, tag, attrs_list in walker.elements:
        attrs = dict(attrs_list)
        name = ""
        if tag == "a" and attrs.get("href"):
            name = _page_name_from_href(attrs["href"])
        elif tag in ("button", "i", "span", "div"):
            haystack = f"{attrs.get('class', '')} {attrs.get('aria-label', '')}".lower()
            for hint, label in _ICON_ROUTE_HINTS.items():
                if hint in haystack:
                    name = label
                    break
        if not name:
            continue
        page_id = "page::" + _stable_id(name)
        pages.setdefault(page_id, {
            "id": page_id, "label": name, "kind": NodeKind.PAGE.value, "source_file": "",
        })
        edges.append({
            "source": screen_id, "target": page_id,
            "relation": "RENDERED_ON", "confidence": "INFERRED",
        })
    return list(pages.values()), edges


# -- DE-02: Interactive Element Detector (graph-writing wrapper) -------------

def extract_elements(html: str, page_id: str) -> tuple[list[dict], list[dict]]:
    """DE-02: wrap `detect_interactive_elements` (already fully built,
    see module docstring) into graph `InteractiveElement` nodes +
    `HAS_ELEMENT` edges. No detection logic lives in this function."""
    found = detect_interactive_elements(html)
    nodes: list[dict] = []
    edges: list[dict] = []
    for i, el in enumerate(found):
        element_id = "element::" + _stable_id(page_id, el.kind, el.selector_hint, str(i))
        nodes.append({
            "id": element_id, "label": el.selector_hint, "kind": NodeKind.INTERACTIVE_ELEMENT.value,
            "source_file": "", "element_kind": el.kind, "selector_hint": el.selector_hint,
            "derived_task_hints": list(el.derived_task_hints), "page_id": page_id,
        })
        edges.append({
            "source": page_id, "target": element_id,
            "relation": "HAS_ELEMENT", "confidence": "EXTRACTED",
        })
    return nodes, edges


# -- DE-03: Implied Navigation Tree ------------------------------------------

def find_implied_pages(page_nodes: Sequence[Node], known_labels: Sequence[str]) -> tuple[list[dict], list[dict]]:
    """DE-03: `Page` nodes with no case-insensitive counterpart in
    `known_labels` (the caller's own PRD-hierarchy page/screen names)
    become `ImpliedPage` nodes — "the UI has a Settings page but the PRD
    doesn't mention it." `IMPLIES_PAGE` edges point FROM the original
    `Page` node TO its `ImpliedPage` counterpart."""
    known = {k.lower() for k in known_labels}
    implied_nodes: list[dict] = []
    edges: list[dict] = []
    for p in page_nodes:
        if p.label.lower() in known:
            continue
        implied_id = "impliedpage::" + _stable_id(p.id)
        implied_nodes.append({
            "id": implied_id, "label": p.label, "kind": NodeKind.IMPLIED_PAGE.value, "source_file": "",
        })
        edges.append({
            "source": p.id, "target": implied_id,
            "relation": "IMPLIES_PAGE", "confidence": "INFERRED",
        })
    return implied_nodes, edges


# -- DE-04: Element-to-Task Decomposition ------------------------------------

def decompose_elements_to_tasks(element_nodes: Sequence[Node]) -> tuple[list[dict], list[dict]]:
    """DE-04: one `DerivedTaskHint` node per hint already present in an
    element's `derived_task_hints` (DE-02's own output, reused verbatim
    — not recomputed), linked via `DERIVES_TASK`. See module docstring
    for why this is a lightweight stand-in, not a real `:AtomicTask`."""
    nodes: list[dict] = []
    edges: list[dict] = []
    for el in element_nodes:
        for hint in el.properties.get("derived_task_hints", []):
            task_id = "derivedtaskhint::" + _stable_id(el.id, hint)
            nodes.append({
                "id": task_id, "label": hint, "kind": NodeKind.DERIVED_TASK_HINT.value,
                "source_file": "", "element_id": el.id, "hint": hint,
            })
            edges.append({
                "source": el.id, "target": task_id,
                "relation": "DERIVES_TASK", "confidence": "INFERRED",
            })
    return nodes, edges


# -- Combined entry point (the spec's own `decompose_page` tool) ------------

def decompose_page(html: str, screen_id: str) -> dict:
    """DE-01/02/04 combined: this screen's own interactive elements +
    their derived task hints, AND the other pages it links to. Returns
    write-ready `all_nodes`/`all_edges` (already correctly `kind`-tagged,
    so a caller can pass them straight to `replace_analysis_nodes` per
    kind, or split by `kind` first) alongside the same data broken out
    by category for direct inspection.
    """
    current_page = {
        "id": screen_id, "label": screen_id, "kind": NodeKind.PAGE.value, "source_file": "",
    }
    element_nodes, has_element_edges = extract_elements(html, screen_id)

    task_nodes: list[dict] = []
    derives_task_edges: list[dict] = []
    for el in element_nodes:
        for hint in el.get("derived_task_hints", []):
            task_id = "derivedtaskhint::" + _stable_id(el["id"], hint)
            task_nodes.append({
                "id": task_id, "label": hint, "kind": NodeKind.DERIVED_TASK_HINT.value,
                "source_file": "", "element_id": el["id"], "hint": hint,
            })
            derives_task_edges.append({
                "source": el["id"], "target": task_id,
                "relation": "DERIVES_TASK", "confidence": "INFERRED",
            })

    linked_pages, rendered_on_edges = extract_pages(html, screen_id)

    return {
        "screen_id": screen_id, "elements": element_nodes, "derived_tasks": task_nodes,
        "linked_pages": linked_pages,
        "all_nodes": [current_page] + element_nodes + task_nodes + linked_pages,
        "all_edges": has_element_edges + derives_task_edges + rendered_on_edges,
    }


# -- DE-04 follow-up: promote a DerivedTaskHint to a real AtomicTask --------

def _slug(text: str) -> str:
    return re.sub(r"[^0-9a-zA-Z]+", "_", text.strip().lower()).strip("_") or "task"


def promote_hint_to_task(hint_node: Node, page_label: str, layer: str = "Frontend") -> dict:
    """DE-04 follow-up: turns a `DerivedTaskHint` (a lightweight
    stand-in, per this module's own docstring) into a REAL, pushable
    `AtomicTask`-shaped dict — closing the gap the original DE-04 slice
    deliberately left open. `file_path` is a genuinely reasonable
    inferred default (`frontend/src/components/<page>/<hint>.tsx`), not
    a placeholder — a developer can rename it, but it's a real starting
    point, not a fake value nobody could act on. The caller is
    responsible for actually pushing the returned dict through
    `AtomicTaskBatch`/`push_tasks` (this function builds the shape,
    it doesn't reach into `cie.task_repository` itself, keeping this
    module's own dependency surface unchanged).
    """
    hint_text = hint_node.properties.get("hint", hint_node.label)
    element_kind = hint_node.properties.get("element_id", "")
    page_slug = _slug(page_label)
    hint_slug = _slug(hint_text)
    return {
        "name": f"{page_slug}_{hint_slug}",
        "task_type": "dev",
        "layer": layer,
        "action": "create",
        "file_path": f"frontend/src/components/{page_slug}/{hint_slug}.tsx",
        "description": (
            f"Implement '{hint_text}' for the {page_label} page "
            f"(derived from interactive element {element_kind or hint_node.id})."
        ),
    }


# -- DE-05/06/07: query surface ----------------------------------------------

def page_tree(pages: Sequence[Node], elements: Sequence[Node], edges: Sequence[EdgeRecord]) -> list[dict]:
    """DE-05: every page, with its interactive elements and their
    already-derived task hints."""
    element_ids_by_page: dict[str, list[str]] = {}
    for er in edges:
        if er.edge.relation == "HAS_ELEMENT":
            element_ids_by_page.setdefault(er.edge.source, []).append(er.edge.target)
    element_by_id = {e.id: e for e in elements}
    out: list[dict] = []
    for p in pages:
        page_elements = [
            element_by_id[eid] for eid in element_ids_by_page.get(p.id, []) if eid in element_by_id
        ]
        out.append({
            "page_id": p.id, "label": p.label,
            "elements": [
                {
                    "id": e.id, "kind": e.properties.get("element_kind", ""),
                    "derived_task_hints": e.properties.get("derived_task_hints", []),
                }
                for e in page_elements
            ],
        })
    return out


def element_coverage(page_id: str, elements: Sequence[Node], edges: Sequence[EdgeRecord]) -> dict:
    """DE-06: for one page, which elements have `DERIVES_TASK` edges
    (i.e. at least one decomposed task hint) and which don't."""
    covered_element_ids = {er.edge.source for er in edges if er.edge.relation == "DERIVES_TASK"}
    page_elements = [e for e in elements if e.properties.get("page_id") == page_id]
    uncovered = [e.id for e in page_elements if e.id not in covered_element_ids]
    return {
        "page_id": page_id,
        "total_elements": len(page_elements),
        "covered_count": len(page_elements) - len(uncovered),
        "uncovered_element_ids": uncovered,
    }


def implied_pages_report(implied_page_nodes: Sequence[Node]) -> list[dict]:
    """DE-07: every `ImpliedPage` — pages implied by the UI but missing
    from the PRD."""
    return [{"id": n.id, "label": n.label} for n in implied_page_nodes]
