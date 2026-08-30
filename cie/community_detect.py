"""RQ-04 + AI-03: Community-Aware Retrieval + Global Summarization.

The spec's own gap analysis undersold this one: it describes community
detection as "unclear which algorithm" — the real answer, confirmed by
reading every line that touches `Node.community` in this codebase, is
that NOTHING has ever computed or assigned it. `list_communities`/
`get_community` were fully wired read-paths with no write-path behind
them; every project loaded via `cie load` has `community: None` on
every node, so both always returned empty. RQ-04 can't be "add
summaries over existing communities" here — it has to start by actually
detecting them.

**Community detection**: `label_propagation` — synchronous label
propagation (Raghavan/Albert/Kumara 2007), a real, standard, well-known
graph-community algorithm, implemented here in pure Python rather than
adding a new dependency (no `networkx`/GDS in this project's
environment — confirmed, not assumed) for one algorithm. Deterministic
given a fixed node-processing order (stable-sorted ids, ties broken by
lowest label id each iteration) — repeatable across runs on an
unchanged graph, though NOT a guarantee of a canonical "best" partition
(label propagation is a heuristic, same caveat any community-detection
algorithm carries). Results are written onto EXISTING nodes'
`community` property via `Neo4jRepository.update_node_properties` — the
SAME patch-not-replace write path CI-06/08 already use for
`complexity_class`/`hot_path`.

**Community summaries** (RQ-04's other half + AI-03): for each detected
community, an LLM turns its members' labels/docstrings/signatures into
a short thematic summary (`core.llm`'s standard `Prompt[T]` +
`AGENT = LlmAgent(...)` + `ask(...)` shape, same as `cie.graphrag`).
Written as `CommunitySummary` nodes (`replace_analysis_nodes`, same
write path `CloneCluster`/`AntiPattern`/`DriftFinding` use) — WITH an
embedding (unlike those four), computed directly via `embed_text` at
write time, so `community_search` can reuse `semantic_search`'s
existing vector-index machinery rather than a new search algorithm.
"""
from __future__ import annotations

from collections import Counter, defaultdict
from dataclasses import dataclass
from typing import Optional

from pydantic import BaseModel

from cie.models import EdgeRecord, Node, NodeKind, SemanticMatch
from cie.repository import Repository

# (R5) `core.llm` imports are deferred into the two LLM/embedding call
# sites below: a module-level import of `core.llm` made this whole module
# unimportable in a standalone install, and 503'd three tools of which
# `community_detect_run` (label propagation — PURE) and `community_search`
# (lexical-fallback search — PURE) have no LLM dependency at all.

# Members shown to the LLM per community — a community can have hundreds
# of members; the prompt only needs enough to characterize the theme; a
# representative sample keeps the prompt small and the summary focused.
_MAX_MEMBERS_IN_PROMPT = 15


def label_propagation(
    nodes: list[Node], edges: list[EdgeRecord], max_iterations: int = 20,
) -> dict[str, int]:
    """Assign each node id a community label via synchronous label
    propagation over the undirected adjacency graph built from `edges`
    (every relation counts — `calls`, `contains`, `defines`, `extends`,
    `implements`, `TESTS` — community structure is about STRUCTURAL
    proximity, not any one relation type).

    Returns `{node_id: community_id}` for every node reachable from at
    least one edge — isolated nodes (no edges at all) are excluded, not
    given their own singleton community, since a "community" of one
    disconnected node isn't a meaningful grouping for RQ-04's thematic
    retrieval use case.
    """
    node_ids = sorted(n.id for n in nodes)
    known_ids = set(node_ids)
    adjacency: dict[str, set[str]] = defaultdict(set)
    for er in edges:
        a, b = er.edge.source, er.edge.target
        if a in known_ids and b in known_ids and a != b:
            adjacency[a].add(b)
            adjacency[b].add(a)

    connected_ids = sorted(nid for nid in node_ids if adjacency.get(nid))
    if not connected_ids:
        return {}

    id_to_index = {nid: i for i, nid in enumerate(connected_ids)}
    labels: dict[str, int] = {nid: id_to_index[nid] for nid in connected_ids}

    for _ in range(max(1, max_iterations)):
        changed = False
        for nid in connected_ids:
            neighbor_labels = [labels[n] for n in adjacency[nid] if n in labels]
            if not neighbor_labels:
                continue
            counts = Counter(neighbor_labels)
            max_count = max(counts.values())
            best_label = min(l for l, c in counts.items() if c == max_count)
            if best_label != labels[nid]:
                labels[nid] = best_label
                changed = True
        if not changed:
            break

    # Renumber to consecutive small ints, largest community first — a
    # cosmetic pass only (any consistent relabeling is equally valid),
    # done so `list_communities`' `ORDER BY size DESC, cid` output reads
    # naturally (community 0 is the biggest, not an arbitrary node index).
    sizes = Counter(labels.values())
    ordered_raw_labels = [l for l, _ in sizes.most_common()]
    renumber = {raw: i for i, raw in enumerate(ordered_raw_labels)}
    return {nid: renumber[label] for nid, label in labels.items()}


def community_detect_run(repo: Repository, project: str = "") -> dict:
    """Compute communities (label propagation) over the full project
    graph and patch `community` onto the corresponding nodes. Returns a
    summary: community count and size distribution. Never raises for an
    empty/edge-free project — returns zero communities.
    """
    nodes, edges = repo.project_graph()
    labels = label_propagation(nodes, edges)
    if not labels:
        return {"communities": 0, "nodes_labeled": 0}
    updates = {nid: {"community": cid} for nid, cid in labels.items()}
    repo.update_node_properties(updates, project=project)
    sizes = Counter(labels.values())
    return {
        "communities": len(sizes),
        "nodes_labeled": len(labels),
        "largest_community_size": max(sizes.values()),
    }


class CommunitySummaryLlmOutput(BaseModel):
    """The LLM's entire output for one community: a short thematic
    summary (1-3 sentences) of what its members have in common."""

    summary: str


def _render_members(members: list[Node]) -> str:
    sample = members[:_MAX_MEMBERS_IN_PROMPT]
    lines = []
    for n in sample:
        blurb = n.docstring or n.signature or ""
        lines.append(f"- {n.label} ({n.kind}) in {n.source_file} — {blurb}")
    if len(members) > len(sample):
        lines.append(f"... and {len(members) - len(sample)} more members")
    return "\n".join(lines)


async def generate_community_summaries(repo: Repository, project: str = "") -> dict:
    """RQ-04 + AI-03: one LLM-generated summary per community from the
    most recent `community_detect_run`, written as `CommunitySummary`
    nodes with an embedding (best-effort — see `_maybe_embed`'s
    docstring) so `community_search` can rank them.

    Runs `list_communities()`'s communities SEQUENTIALLY (one `ask()`
    call per community) rather than concurrently — a project with many
    communities means many LLM calls either way; sequential keeps this
    function's error handling simple (one failed community doesn't need
    coordinating with N-1 in-flight others) and avoids an unbounded
    concurrent-request burst against whatever LLM provider is
    configured. Acceptable for an explicit, on-demand analysis pass, not
    a hot path.
    """
    community_counts = repo.list_communities()
    if not community_counts:
        repo.replace_analysis_nodes(NodeKind.COMMUNITY_SUMMARY.value, [], [], project=project)
        return {"summarized": 0}

    from core.llm import LlmAgent, Prompt, ask  # deferred: see the module R5 note

    @dataclass(frozen=True)
    class CommunitySummaryPrompt(Prompt[CommunitySummaryLlmOutput]):
        """Prompt built here (not module scope) — its base class lives in
        the host project's optional `core.llm` layer."""

        members_text: str

        def system_prompt(self) -> str:
            return (
                "You are summarizing one COMMUNITY (a structurally-connected "
                "cluster) of code symbols from a knowledge graph, for use in "
                "thematic search later. Given a sample of the community's "
                "member symbols (name, kind, file, and docstring/signature "
                "when available), write a SHORT (1-3 sentence) summary of "
                "the theme or responsibility this group of code shares — "
                "e.g. 'Authentication and session management: login, "
                "logout, and token refresh handlers.' Do not list every "
                "member by name; characterize the group's PURPOSE."
            )

        def render(self) -> str:
            return f"## Community members\n{self.members_text}"

    agent = LlmAgent(
        name="cie_community_summary", prompt_type=CommunitySummaryPrompt,
        output_type=CommunitySummaryLlmOutput,
    )

    summary_nodes: list[dict] = []
    failures: list[int] = []
    for community_id, size in community_counts:
        members = repo.get_community(community_id)
        if not members:
            continue
        try:
            output = await ask(CommunitySummaryPrompt(members_text=_render_members(members)))
            summary_text = output.summary
        except Exception:  # noqa: BLE001 - one bad community must not abort the rest
            failures.append(community_id)
            summary_text = ""
        if not summary_text:
            continue
        embedding = _maybe_embed(summary_text)
        summary_nodes.append({
            "id": f"communitysummary::{project or 'default'}::{community_id}",
            "label": summary_text[:80],
            "source_file": "", "source_location": "", "file_type": "analysis",
            "kind": NodeKind.COMMUNITY_SUMMARY.value, "signature": "",
            "line_start": 0, "line_end": 0, "docstring": summary_text,
            "embedding": embedding,
            "community_id": community_id, "member_count": size,
            "summary": summary_text,
        })

    repo.replace_analysis_nodes(
        NodeKind.COMMUNITY_SUMMARY.value, summary_nodes, [], project=project,
    )
    return {
        "summarized": len(summary_nodes),
        "total_communities": len(community_counts),
        "failed": failures,
    }


def _maybe_embed(text: str) -> list[float]:
    """Best-effort embedding for a community summary — degrades to `[]`
    on any embedding failure (unreachable API, no key configured, or the
    host project's `core.llm` layer absent entirely — R5: the import is
    HERE, with its absence treated like any other embedding failure),
    same "queryable nice-to-have, never a hard dependency" convention
    `Neo4jRepository._maybe_compute_embeddings` already uses for the
    structural load path. A summary without an embedding still gets
    written and is still findable via `community_search`'s lexical
    fallback (label/summary-text substring match), just not via the
    vector leg.
    """
    try:
        from core.llm.embed_text import embed_text  # deferred: see the R5 note

        return list(embed_text(text, input_type="passage"))
    except Exception:  # noqa: BLE001
        return []


def community_search(repo: Repository, query: str, top_k: int = 5) -> list[dict]:
    """RQ-04: thematic search over `CommunitySummary` nodes. Reuses
    `semantic_search` directly (no new search algorithm — community
    summaries are regular embedded `:Node`s) plus a lexical fallback
    substring match for summaries that have no embedding (see
    `_maybe_embed`). Both `semantic_search` and `analysis_nodes` are
    Repository protocol methods, so this stays at the same layer
    `cie.clone_detect`/`cie.perf_analyze`/`cie.drift_detect` already
    operate at — no reach into `QueryEngine`'s internals needed.
    """
    if not query or not query.strip():
        return []
    query = query.strip()
    matches: list[SemanticMatch] = [
        m for m in repo.semantic_search(query, top_k=top_k * 3)
        if m.node.kind == NodeKind.COMMUNITY_SUMMARY.value
    ]
    seen_ids = {m.node.id for m in matches}
    needle = query.lower()
    lexical_fallback = [
        n for n in repo.analysis_nodes(NodeKind.COMMUNITY_SUMMARY.value)
        if n.id not in seen_ids and needle in (n.docstring or "").lower()
    ]
    results = [
        {"community_id": m.node.properties.get("community_id"),
         "member_count": m.node.properties.get("member_count"),
         "summary": m.node.properties.get("summary", m.node.docstring), "score": m.score}
        for m in matches
    ]
    results.extend({
        "community_id": n.properties.get("community_id"),
        "member_count": n.properties.get("member_count"),
        "summary": n.properties.get("summary", n.docstring), "score": None,
    } for n in lexical_fallback)
    return results[:top_k]
