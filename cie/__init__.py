"""cie-neo4j: a CLI querying engine for a knowledge graph stored in Neo4j.

Phase 1 scope: the querying engine. It reads nodes and edges persisted in Neo4j
and answers structural questions about a codebase (or any extracted corpus):
search, traversal, neighbor lookup, community membership, god nodes, graph
statistics, and shortest paths.
"""

from cie.models import (
    CallSite,
    Confidence,
    ContextHit,
    Edge,
    EdgeRecord,
    GraphStats,
    ImportRecord,
    MethodSignature,
    Node,
    NodeKind,
    NodeRecord,
    PathResult,
    SymbolMatch,
    TraversalMode,
    TraversalResult,
)
from cie.query import QueryEngine
from cie.repository import Repository

__all__ = [
    "CallSite",
    "Confidence",
    "ContextHit",
    "Edge",
    "EdgeRecord",
    "GraphStats",
    "ImportRecord",
    "MethodSignature",
    "Node",
    "NodeKind",
    "NodeRecord",
    "PathResult",
    "QueryEngine",
    "Repository",
    "SymbolMatch",
    "TraversalMode",
    "TraversalResult",
]

__version__ = "0.1.0a1"
