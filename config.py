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
    # Driver-level connection/retry bounds. These only bound connection
    # *acquisition* and *retryable* transient errors — they do NOT bound a
    # query that is already running but blocked waiting on a lock held by
    # another (possibly leaked) transaction elsewhere on a shared instance.
    # See `query_timeout_s`/`schema_timeout_s` below for that case
    # (confirmed live 2026-08-04: a `CREATE INDEX ... IF NOT EXISTS` no-op
    # hung indefinitely — no timeout, no error — behind a ~2.7-hour-old
    # leaked read transaction holding a shared schema lock on this repo's
    # Aura dev instance; every prior `GraphDatabase.driver(...)` call site
    # set none of these, so a stuck/unreachable Aura instance could hang
    # every `cie` CLI invocation and every `/tools/{tool}` HTTP call
    # forever with zero diagnostic).
    connect_timeout_s: float = 10.0
    max_retry_time_s: float = 15.0
    # Pool sizing — previously left at the driver's own defaults
    # (max_connection_pool_size=100, connection_acquisition_timeout=60s),
    # undocumented and un-tunable from here. forge's story-level
    # concurrency (AdaptiveConcurrencyLimiter, ceiling 8) plus its own
    # per-story tool calls stays well under 100 concurrent connections, so
    # the pool itself was never actually the constraint — but leaving it
    # implicit meant no one place to look to confirm that. Explicit and
    # env-overridable like every other bound here.
    max_pool_size: int = 100
    connection_acquisition_timeout_s: float = 60.0
    # Hard, independently-enforced wall-clock budgets applied in
    # `cie.timeouts.run_with_timeout` around the actual query round trip
    # (`Neo4jRepository._run`/`load_extraction`/`reindex_file`) and schema
    # bootstrap (`ensure_indices`) — these are what actually bound a
    # lock-wait hang, since driver-level timeouts above do not.
    query_timeout_s: float = 30.0
    schema_timeout_s: float = 15.0
    # `cie load`/`reindex` can legitimately run longer than a single read
    # query (bulk UNWIND writes across thousands of nodes/edges), so they
    # get their own, larger budget rather than sharing `query_timeout_s`.
    write_timeout_s: float = 120.0

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
            connect_timeout_s=float(
                os.environ.get("CIE_NEO4J_CONNECT_TIMEOUT_S", "10")
            ),
            max_retry_time_s=float(
                os.environ.get("CIE_NEO4J_MAX_RETRY_TIME_S", "15")
            ),
            max_pool_size=int(
                os.environ.get("CIE_NEO4J_MAX_POOL_SIZE", "100")
            ),
            connection_acquisition_timeout_s=float(
                os.environ.get("CIE_NEO4J_CONNECTION_ACQUISITION_TIMEOUT_S", "60")
            ),
            query_timeout_s=float(
                os.environ.get("CIE_NEO4J_QUERY_TIMEOUT_S", "30")
            ),
            schema_timeout_s=float(
                os.environ.get("CIE_NEO4J_SCHEMA_TIMEOUT_S", "15")
            ),
            write_timeout_s=float(
                os.environ.get("CIE_NEO4J_WRITE_TIMEOUT_S", "120")
            ),
        )

    def driver_kwargs(self) -> dict:
        """kwargs for `GraphDatabase.driver(...)` — connection-level bounds
        only (see the field docstrings above for why this isn't sufficient
        on its own)."""
        return {
            "connection_timeout": self.connect_timeout_s,
            "max_transaction_retry_time": self.max_retry_time_s,
            "max_connection_pool_size": self.max_pool_size,
            "connection_acquisition_timeout": self.connection_acquisition_timeout_s,
        }
