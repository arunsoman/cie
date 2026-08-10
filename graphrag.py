"""AI-01: GraphRAG question-answering with citations.

Pipeline: `hybrid_search` (item 4, RQ-01) retrieves candidate nodes for
`question`; `entity_context` (item 5, AI-02) expands the top few hits into
their callers/callees/tests/class-hierarchy neighborhood; both are
assembled into one compact, char-capped context block; an LLM call
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

Context assembly is deliberately simple: a straight text join of the
retrieved nodes' signatures/docstrings plus each expanded entity's
immediate neighborhood, bounded by a generous character cap
(`_CONTEXT_CHAR_CAP`). This is NOT the full dedup/salience token-budget
system described in spec item AI-06 — that is explicitly out of scope for
this slice (see be-v2/docs/cie-grounding-slice-implementation.md's item 6
caveats).
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional

from pydantic import BaseModel

from core.llm import LlmAgent, Prompt, ask
from cie.models import HybridMatch
from cie.query import QueryEngine

# Context-block char cap — generous but bounded, per this module's own
# docstring ("keep this simple... do NOT build the full token-budget
# system"). ~8000 chars is comfortably inside every configured model's
# context window for this codebase's providers while still being large
# enough to carry several retrieved symbols plus their neighborhoods.
_CONTEXT_CHAR_CAP = 8000

# How many of hybrid_search's top hits get expanded via entity_context.
# Expanding every hit would multiply Neo4j round trips for diminishing
# grounding value; the top few are overwhelmingly where the real answer
# lives for a well-formed question.
_EXPAND_TOP_N = 3

# How many hybrid_search hits to retrieve in total (citations cover all of
# these, even the ones not expanded via entity_context).
_RETRIEVE_TOP_K = 8


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


def _assemble_context(matches: list[HybridMatch], contexts: list[dict]) -> str:
    """Compact, char-capped text block — see module docstring for why
    this is a plain join, not a salience/dedup system."""
    lines: list[str] = ["### Retrieved symbols"]
    for m in matches:
        n = m.node
        lines.append(f"- {n.label} ({n.kind}) in {n.source_file}:{n.source_location}")
        if n.signature:
            lines.append(f"  signature: {n.signature}")
        if n.docstring:
            lines.append(f"  docstring: {n.docstring}")

    for ctx in contexts:
        node = ctx.get("node")
        if node is None:
            continue
        lines.append(f"\n### {node.label} neighborhood")
        callers = ctx.get("callers") or []
        if callers:
            names = ", ".join(r.source_label for r in callers[:5])
            lines.append(f"  called by: {names}")
        callees = ctx.get("callees") or []
        if callees:
            names = ", ".join(r.target_label for r in callees[:5])
            lines.append(f"  calls: {names}")
        tests = ctx.get("tests") or []
        if tests:
            names = ", ".join(r.source_label for r in tests[:5])
            lines.append(f"  tested by: {names}")
        hierarchy = ctx.get("class_hierarchy") or {}
        ancestors = hierarchy.get("ancestors") or []
        if ancestors:
            lines.append(f"  extends: {', '.join(a.label for a in ancestors)}")
        interfaces = hierarchy.get("interfaces") or []
        if interfaces:
            lines.append(f"  implements: {', '.join(i.label for i in interfaces)}")

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


async def qa(
    question: str, project: str = "", engine: Optional[QueryEngine] = None,
) -> QaResult:
    """AI-01: retrieve -> expand -> assemble -> ask, with a citation trail.

    Args:
        question: the natural-language question.
        project: project namespace, resolved via `cie.factory.get_engine`
            when `engine` isn't supplied directly.
        engine: an already-built `QueryEngine` to use instead of
            resolving one from `project` — `ToolService.qa` passes its
            OWN already-project-scoped engine here (it doesn't track a
            separate project string of its own), so this call doesn't
            redundantly re-resolve one through `cie.factory`.

    Returns:
        A `QaResult` — never raises for "no context found" (degrades to
        an explicit "not enough context" answer with no citations); DOES
        propagate a real LLM-call failure (network/provider error), same
        as every other `core.llm.ask` caller in this codebase.
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

    matches = engine.hybrid_search(question, top_k=_RETRIEVE_TOP_K)
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
