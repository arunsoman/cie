"""Connection settings for the Neo4j backend.

Reads from environment variables with sensible defaults. Centralized here so
the CLI and any future entry points share one source of truth.

cie's structural code graph shares ONE Neo4j database with the rest of
the app (the business/PRD graph, once core/graph/client.py is ported) —
same URI, same credentials. ``NEO4J_URI``/``NEO4J_USERNAME``/
``NEO4J_PASSWORD`` (the convention ``backend/`` already uses) are read
first; the legacy ``CIE_NEO4J_*`` vars still work as an explicit
override for the rare case cie's code graph should live in a
different database (or even a different Neo4j instance) than the business
graph — set them only if you actually want that split.
"""

from __future__ import annotations

import os
from dataclasses import dataclass


@dataclass(frozen=True)
class Neo4jConfig:
    uri: str
    user: str
    password: str
    database: str = "neo4j"

    @classmethod
    def from_env(cls) -> "Neo4jConfig":
        return cls(
            uri=os.environ.get("CIE_NEO4J_URI")
            or os.environ.get("NEO4J_URI", "bolt://localhost:7687"),
            user=os.environ.get("CIE_NEO4J_USER")
            or os.environ.get("NEO4J_USERNAME")
            or os.environ.get("NEO4J_USER", "neo4j"),
            password=os.environ.get("CIE_NEO4J_PASSWORD")
            or os.environ.get("NEO4J_PASSWORD", "password"),
            database=os.environ.get("CIE_NEO4J_DATABASE")
            or os.environ.get("NEO4J_DATABASE", "neo4j"),
        )
