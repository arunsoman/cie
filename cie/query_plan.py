"""AI-05: Deterministic Query Planning.

A rule-based (NOT LLM-based — "deterministic" is the point) classifier
that maps a natural-language question to one of four retrieval
strategies, mirroring the spec's own suggested mapping:

- "what is X?" / "what does X do?" -> **entity_lookup**
- "how does X connect to Y?" / "relationship between X and Y" -> **path_query**
- "what depends on X?" / "who calls X?" -> **impact_analysis**
- anything else -> **hybrid_search** (the safe, broad default)

The honest limitation: symbol extraction only recognizes EXPLICIT
references — a name wrapped in backticks/quotes (`` `widget_factory` ``,
`"UserService"`). A free-text question naming a symbol WITHOUT
quoting/backticking it (`"what does widget factory do"`, no backticks)
extracts zero symbols and falls through to `hybrid_search` — this
module never guesses at which bare English word might be a code symbol;
a wrong guess would be worse than the honest "not confident enough"
fallback. This is a deliberate precision-over-recall choice, not an
oversight — `cie.graphrag.qa` only takes the entity_lookup fast path
when this classifier is actually confident.
"""
from __future__ import annotations

import re

STRATEGY_ENTITY_LOOKUP = "entity_lookup"
STRATEGY_PATH_QUERY = "path_query"
STRATEGY_IMPACT_ANALYSIS = "impact_analysis"
STRATEGY_HYBRID_SEARCH = "hybrid_search"

_QUOTED_SYMBOL_RE = re.compile(r"[`'\"]([A-Za-z_][A-Za-z0-9_.]*)[`'\"]")

# Order matters: impact/path phrasing is checked before the more generic
# "what is/does" lookup phrasing, since a question can contain both
# ("what depends on `foo`?" also starts with "what") — the more
# SPECIFIC strategy should win.
_IMPACT_KEYWORDS = (
    "depends on", "affects", "breaks if", "impact of", "what uses",
    "who calls", "callers of", "what calls",
)
_PATH_KEYWORDS = (
    "connect to", "connected to", "relationship between", "path between",
    "related to", "how does", "how is",
)
_LOOKUP_KEYWORDS = ("what is", "what does", "explain", "describe", "define")


def extract_symbols(question: str) -> list[str]:
    """Explicit backtick/quote-wrapped symbol references only — see
    module docstring for why bare-word extraction is deliberately not
    attempted."""
    return _QUOTED_SYMBOL_RE.findall(question)


def classify(question: str) -> dict:
    """Returns `{"strategy", "symbols", "reasoning"}`. Falls back to
    `hybrid_search` whenever the keyword+symbol combination needed for a
    more specific strategy isn't both present — a confident wrong guess
    is worse than an honest "not sure, search broadly"."""
    if not question or not question.strip():
        return {"strategy": STRATEGY_HYBRID_SEARCH, "symbols": [], "reasoning": "empty question"}
    lowered = question.lower()
    symbols = extract_symbols(question)

    if symbols and any(k in lowered for k in _IMPACT_KEYWORDS):
        return {
            "strategy": STRATEGY_IMPACT_ANALYSIS, "symbols": symbols[:1],
            "reasoning": "impact/dependency phrasing + an explicit symbol reference",
        }
    if len(symbols) >= 2 and any(k in lowered for k in _PATH_KEYWORDS):
        return {
            "strategy": STRATEGY_PATH_QUERY, "symbols": symbols[:2],
            "reasoning": "relationship phrasing + two explicit symbol references",
        }
    if len(symbols) == 1 and any(k in lowered for k in _LOOKUP_KEYWORDS):
        return {
            "strategy": STRATEGY_ENTITY_LOOKUP, "symbols": symbols,
            "reasoning": "definitional phrasing + a single explicit symbol reference",
        }
    return {
        "strategy": STRATEGY_HYBRID_SEARCH, "symbols": symbols,
        "reasoning": "no confident strategy match (see module docstring on "
                     "why bare-word symbol guessing is avoided); broad retrieval",
    }
