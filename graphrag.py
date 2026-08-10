"""AI-01: GraphRAG question-answering with citations.

Pipeline: `cie.query_plan.classify` (AI-05) picks a retrieval strategy;
for the common case (`hybrid_search`) `hybrid_search` (RQ-01) retrieves
candidate nodes, `rerank` (RQ-07) reorders them by an LLM's relevance
judgment, `entity_context` (AI-02) expands the top few hits into their
callers/callees/tests/class-hierarchy neighborhood, and
`_assemble_context` (AI-06) turns both into one deduplicated,
salience-ordered, token-budgeted context block; a final LLM call
(`core.llm`'s standard `Prompt[T]` + `AGENT = LlmAgent(...)` + `ask(...)`
shape) turns that context + the question into an answer.

Citations are DELIBERATELY not part of the LLM's own output. The LLM only
ever produces the answer TEXT (`QaLlmOutput`, a single `answer: str`
field) — every `Citation` this module returns (`source_file`,
`source_location`, `confidence`, `node_id`) is built by `qa()` itself
straight from the `hybrid_search` results that grounded the prompt, never
asked of or filled in by the model. This is a hard grounding-integrity
requirement, not a style preference: a model asked to also emit citations
could plausibly hallucinate a file/line that LOOKS right but was never
actually retrieved — building them server-side from real retrieval data
makes that class of error structurally impossible, not just unlikely.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional

from pydantic import BaseModel

from core.llm import LlmAgent, Prompt, ask
from cie.models import HybridMatch
from cie.query import QueryEngine
from cie import query_plan

# AI-06: per-query-type token budgets from spec item AI-06's own table
# ("GraphRAG context <= 4K tokens"). Token count is APPROXIMATED as
# chars // 4 (a standard rough heuristic for English prose/code text) —
# this codebase has no tokenizer dependency to count exactly, and an
# approximation that's honest about being one is better than a fake
# precise-looking count.
_CONTEXT_TOKEN_BUDGET = 4000
_CHARS_PER_TOKEN = 4
_CONTEXT_CHAR_CAP = _CONTEXT_TOKEN_BUDGET * _CHARS_PER_TOKEN

# How many of hybrid_search's top hits get expanded via entity_context.
# Expanding every hit would multiply Neo4j round trips for diminishing
# grounding value; the top few are overwhelmingly where the real answer
# lives for a well-formed question.
_EXPAND_TOP_N = 3

# How many hybrid_search hits to retrieve in total (citations cover all of
# these, even the ones not expanded via entity_context).
_RETRIEVE_TOP_K = 8


def _estimate_tokens(text: str) -> int:
    """AI-06's documented token approximation — see the budget constants'
    comment above for why this is chars // 4, not a real tokenizer."""
    return len(text) // _CHARS_PER_TOKEN


class QaLlmOutput(BaseModel):
    """The LLM's ENTIRE output: just the answer text. See this module's
    docstring for why citations are never part of what the model
    produces."""

    answer: str


@dataclass(frozen=True)
class Citation:
    """One grounding citation, built entirely from a retrieved node — see
    module docstring for why this is never LLM-generated. `confidence` is
    the retrieved node's `hybrid_search` combined score (0..1, already a
    real computed number from real signals — see `cie.models.HybridMatch`
    — not the EXTRACTED/INFERRED/AMBIGUOUS edge-confidence enum used
    elsewhere in cie, since a hybrid_search hit is a NODE match, not an
    edge)."""

    source_file: str
    source_location: str
    confidence: float
    node_id: str


@dataclass(frozen=True)
class QaResult:
    """qa()'s return shape: an answer plus its full grounding trail —
    the citations AND the raw retrieved node ids (a superset of what the
    citations alone show, so a caller can inspect exactly what was
    retrieved even if it didn't become a citation)."""

    answer: str
    citations: tuple[Citation, ...] = field(default_factory=tuple)
    retrieved_node_ids: tuple[str, ...] = field(default_factory=tuple)


@dataclass(frozen=True)
class QaPrompt(Prompt[QaLlmOutput]):
    question: str
    context: str

    def system_prompt(self) -> str:
        return (
            "You are a code-grounded question-answering assistant for a "
            "software codebase's knowledge graph. Answer the question "
            "using ONLY the retrieved context provided below — every "
            "symbol, file, and relationship you reference must come from "
            "that context. If the context does not contain enough "
            "information to answer confidently, say so explicitly rather "
            "than guessing or inventing file paths, function names, or "
            "behavior not shown in the context. Do not include citations "
            "or file/line references in your answer text — those are "
            "attached separately by the caller from the same retrieved "
            "data; just answer the question in plain prose."
        )

    def render(self) -> str:
        return (
            f"## Question\n{self.question}\n\n"
            f"## Retrieved context\n{self.context}"
        )


AGENT = LlmAgent(name="cie_graphrag_qa", prompt_type=QaPrompt, output_type=QaLlmOutput)


class RerankLlmOutput(BaseModel):
    """RQ-07's ENTIRE LLM output: a relevance-ordered list of node ids
    drawn from the candidates it was shown (most-relevant first). The
    model is never asked to invent new ids — `rerank` below drops any id
    it returns that wasn't in the original candidate set, same
    grounding-integrity discipline as `QaLlmOutput`."""

    ranked_node_ids: list[str]


@dataclass(frozen=True)
class RerankPrompt(Prompt[RerankLlmOutput]):
    question: str
    candidates: str

    def system_prompt(self) -> str:
        return (
            "You are a relevance-ranking assistant for a code knowledge "
            "graph. Given a question and a list of candidate symbols "
            "(each with an id), return `ranked_node_ids`: the candidates' "
            "ids reordered from MOST to LEAST relevant to answering the "
            "question. You may drop candidates that are clearly "
            "irrelevant, but only return ids that appeared in the "
            "candidate list — never invent a new id."
        )

    def render(self) -> str:
        return f"## Question\n{self.question}\n\n## Candidates\n{self.candidates}"


RERANK_AGENT = LlmAgent(
    name="cie_graphrag_rerank", prompt_type=RerankPrompt, output_type=RerankLlmOutput,
)


def _render_candidates(matches: list[HybridMatch]) -> str:
    lines = []
    for m in matches:
        n = m.node
        blurb = n.docstring or n.signature or ""
        lines.append(f"- {n.id}: {n.label} ({n.kind}) — {blurb}")
    return "\n".join(lines)


async def rerank(question: str, matches: list[HybridMatch], top_k: Optional[int] = None) -> list[HybridMatch]:
    """RQ-07: LLM-based reranking of `hybrid_search` results (a
    cross-encoder model, the spec's other suggested option, would add a
    new ML dependency this codebase doesn't otherwise need — an LLM call
    reuses infrastructure `qa()` already depends on, per the spec's own
    "alternatively, use an LLM to rerank" fallback).

    Degrades to the ORIGINAL hybrid_search order on any LLM failure
    (network/provider error, malformed output) — a reranker hiccup must
    never break retrieval entirely, only fail to improve it.
    """
    if len(matches) <= 1:
        return matches[: top_k or len(matches)]
    try:
        output = await ask(RerankPrompt(
            question=question, candidates=_render_candidates(matches),
        ))
        by_id = {m.node.id: m for m in matches}
        ranked = [by_id[nid] for nid in output.ranked_node_ids if nid in by_id]
        seen = {m.node.id for m in ranked}
        ranked.extend(m for m in matches if m.node.id not in seen)
        return ranked[: top_k or len(matches)]
    except Exception:  # noqa: BLE001 - degrade to original order, don't raise
        # Covers BOTH a failed `ask()` call (network/provider error) AND
        # a malformed response from an unreliable model (missing/wrong-
        # shaped `ranked_node_ids`) — either way, a reranker hiccup must
        # never break retrieval entirely, only fail to improve it.
        return matches[: top_k or len(matches)]


def _assemble_context(matches: list[HybridMatch], contexts: list[dict]) -> str:
    """AI-06: deduplicated, salience-ordered, token-budgeted context
    block.

    Salience order: `matches` is already rank-ordered (by `hybrid_search`
    or `rerank`) — this function processes it top-to-bottom and STOPS
    adding full detail once `_CONTEXT_TOKEN_BUDGET` is spent, so a
    lower-ranked match never displaces a higher-ranked one's detail. A
    match that arrives after the budget is exhausted still gets ONE
    compact summary line (never silently vanishes from the caller's view
    of what was retrieved) rather than nothing.

    Deduplication: a node that shows up BOTH as a top-level retrieved
    symbol AND inside another match's expanded neighborhood (a common
    callers/callees overlap) is only rendered with full detail once —
    `_seen_ids` tracks this across both passes.
    """
    lines: list[str] = ["### Retrieved symbols"]
    budget = _CONTEXT_TOKEN_BUDGET
    seen_ids: set[str] = set()

    for m in matches:
        n = m.node
        if n.id in seen_ids:
            continue
        seen_ids.add(n.id)
        full = [f"- {n.label} ({n.kind}) in {n.source_file}:{n.source_location}"]
        if n.signature:
            full.append(f"  signature: {n.signature}")
        if n.docstring:
            full.append(f"  docstring: {n.docstring}")
        full_text = "\n".join(full)
        if _estimate_tokens(full_text) <= budget:
            lines.append(full_text)
            budget -= _estimate_tokens(full_text)
        else:
            # Salience-truncated: still listed (never silently dropped),
            # just without the detail that would have blown the budget.
            summary = f"- {n.label} ({n.kind}) in {n.source_file}"
            lines.append(summary)
            budget -= _estimate_tokens(summary)

    for ctx in contexts:
        node = ctx.get("node")
        if node is None or budget <= 0:
            continue
        block: list[str] = [f"\n### {node.label} neighborhood"]
        callers = [
            r for r in (ctx.get("callers") or [])
            if r.edge.source not in seen_ids
        ][:5]
        if callers:
            block.append(f"  called by: {', '.join(r.source_label for r in callers)}")
        callees = [
            r for r in (ctx.get("callees") or [])
            if r.edge.target not in seen_ids
        ][:5]
        if callees:
            block.append(f"  calls: {', '.join(r.target_label for r in callees)}")
        tests = ctx.get("tests") or []
        if tests:
            block.append(f"  tested by: {', '.join(r.source_label for r in tests[:5])}")
        hierarchy = ctx.get("class_hierarchy") or {}
        ancestors = hierarchy.get("ancestors") or []
        if ancestors:
            block.append(f"  extends: {', '.join(a.label for a in ancestors)}")
        interfaces = hierarchy.get("interfaces") or []
        if interfaces:
            block.append(f"  implements: {', '.join(i.label for i in interfaces)}")
        if len(block) == 1:
            continue  # nothing new beyond the header — skip entirely
        block_text = "\n".join(block)
        if _estimate_tokens(block_text) > budget:
            continue  # neighborhood detail is the first thing cut when tight
        lines.append(block_text)
        budget -= _estimate_tokens(block_text)

    # Final hard cap: a belt-and-suspenders bound in raw chars, in case
    # the per-block budget bookkeeping above still lets the total run a
    # little over (e.g. many small blocks each individually under
    # budget but summing past it near the boundary).
    return "\n".join(lines)[:_CONTEXT_CHAR_CAP]


def _build_citations(matches: list[HybridMatch]) -> list[Citation]:
    return [
        Citation(
            source_file=m.node.source_file,
            source_location=m.node.source_location,
            confidence=m.score,
            node_id=m.node.id,
        )
        for m in matches
    ]


def _entity_lookup_matches(engine: QueryEngine, symbol: str) -> list[HybridMatch]:
    """AI-05's entity_lookup fast path: build a single-element
    `HybridMatch` list directly from `get_node` (skipping
    `hybrid_search` entirely) so `qa()`'s downstream assembly/citation
    code can treat this exactly like a one-hit search result — one
    shape, not a second code path for the fast case. `score=1.0`: the
    symbol was named EXPLICITLY by the question (backtick/quoted), which
    is a stronger confidence signal than any retrieval score."""
    node_record = engine.get_node(symbol)
    if node_record is None:
        return []
    return [HybridMatch(
        node=node_record.node, score=1.0,
        lexical_score=1.0, dense_score=0.0, graph_score=0.0,
    )]


async def qa(
    question: str, project: str = "", engine: Optional[QueryEngine] = None,
    use_reranking: bool = True,
) -> QaResult:
    """AI-01: plan -> retrieve -> rerank -> expand -> assemble -> ask,
    with a citation trail.

    Args:
        question: the natural-language question.
        project: project namespace, resolved via `cie.factory.get_engine`
            when `engine` isn't supplied directly.
        engine: an already-built `QueryEngine` to use instead of
            resolving one from `project` — `ToolService.qa` passes its
            OWN already-project-scoped engine here (it doesn't track a
            separate project string of its own), so this call doesn't
            redundantly re-resolve one through `cie.factory`.
        use_reranking: RQ-07's LLM reranking pass over `hybrid_search`
            results. `True` by default; `False` skips a second LLM call
            when the caller wants lower latency/cost over precision, or
            in tests that want to isolate retrieval from reranking.

    Returns:
        A `QaResult` — never raises for "no context found" (degrades to
        an explicit "not enough context" answer with no citations); DOES
        propagate a real LLM-call failure (network/provider error), same
        as every other `core.llm.ask` caller in this codebase.

    AI-05 note: `cie.query_plan.classify` is consulted for a single fast
    path — `entity_lookup` with exactly one confidently-extracted symbol
    skips `hybrid_search`/`rerank` entirely and grounds directly on that
    node via `entity_context`. Every other strategy (`impact_analysis`,
    `path_query`, and the `hybrid_search` default) still runs the SAME
    hybrid_search+rerank+expand pipeline below — see this module's
    docstring in `cie/query_plan.py` for why a broader rewiring per
    strategy was cut for scope (a wrong deterministic guess would be
    worse than the existing broad-retrieval default).
    """
    if not question or not question.strip():
        return QaResult(answer="", citations=(), retrieved_node_ids=())
    question = question.strip()
    if engine is None:
        # Imported here, not at module level: cie.factory imports
        # cie.tools (for ToolService), and cie.tools imports this module
        # (for ToolService.qa) — a module-level `from cie import factory`
        # here would be a genuine circular import, not a hypothetical one.
        from cie import factory

        engine = factory.get_engine(project)

    plan = query_plan.classify(question)
    matches: list[HybridMatch] = []
    if plan["strategy"] == query_plan.STRATEGY_ENTITY_LOOKUP and plan["symbols"]:
        matches = _entity_lookup_matches(engine, plan["symbols"][0])

    if not matches:
        matches = engine.hybrid_search(question, top_k=_RETRIEVE_TOP_K)
        if matches and use_reranking:
            matches = await rerank(question, matches, top_k=_RETRIEVE_TOP_K)

    if not matches:
        return QaResult(
            answer="I don't have enough indexed context in this codebase's "
                   "knowledge graph to answer that question.",
            citations=(), retrieved_node_ids=(),
        )

    contexts: list[dict] = []
    for match in matches[:_EXPAND_TOP_N]:
        ctx = engine.entity_context(match.node.label)
        if ctx:
            contexts.append(ctx)

    context_block = _assemble_context(matches, contexts)
    llm_output = await ask(QaPrompt(question=question, context=context_block))

    return QaResult(
        answer=llm_output.answer,
        citations=tuple(_build_citations(matches)),
        retrieved_node_ids=tuple(m.node.id for m in matches),
    )
