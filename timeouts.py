"""Hard wall-clock guard for Neo4j operations that can otherwise hang forever.

Root cause of the incident this module fixes (confirmed live 2026-08-04):
`Neo4jRepository.connect()` unconditionally runs `ensure_indices()` (a
`CREATE INDEX ... IF NOT EXISTS` DDL batch) on every connect, including
plain read commands like `cie health`. On this repo's shared Aura dev
instance, a `SHOW TRANSACTIONS` inspection found a ~2.7-hour-old *leaked*
read transaction (`MATCH (a:Node)-[r:RELATES]->(b:Node) ...`) still
holding a shared schema lock; `CREATE INDEX` needs the exclusive schema
lock, so it queued behind it — forever, from the client's point of view,
because none of this repo's four `GraphDatabase.driver(...)` call sites
set `connection_timeout`/`max_transaction_retry_time`, and neither of
those actually bounds a query that's already running but blocked on a
lock anyway (confirmed by direct testing: even `session.run(stmt,
timeout=8)`, which sets the server-side transaction timeout, did not
unblock the queued schema operation within 90s — the wait is for lock
*acquisition*, which precedes the transaction's own execution budget).

The only way to guarantee a caller gets control back is a client-side
wall-clock deadline enforced independently of whatever the driver/server
are doing. `run_with_timeout` runs the blocking call on a worker thread
and gives up waiting on it after `timeout` seconds; the worker thread
itself may leak (Python cannot forcibly kill a thread blocked in a C
socket read), but the caller is unblocked either way, with a diagnostic
that points at the actual class of problem instead of a silent hang.
"""

from __future__ import annotations

import concurrent.futures
from typing import Any, Callable, TypeVar

T = TypeVar("T")

# One shared, small pool for guarded calls. Threads that time out are
# abandoned (not joined) rather than blocking process exit — CLI processes
# are short-lived and the interpreter will tear the pool down on exit;
# long-lived server processes (the /tools/{tool} HTTP surface) accumulate at
# most a handful of these over the process lifetime, which is an acceptable
# trade-off against the alternative (every request hanging forever).
_EXECUTOR = concurrent.futures.ThreadPoolExecutor(
    max_workers=16, thread_name_prefix="cie-neo4j-guard"
)


class Neo4jOperationTimeout(RuntimeError):
    """A Neo4j operation exceeded its independently-enforced wall-clock budget.

    Distinct from the driver's own `ServiceUnavailable`/`SessionExpired`
    errors: those fire when the driver detects a *broken* connection. This
    fires when the connection is fine but the server-side transaction is
    stuck (most commonly: queued behind another transaction's lock).
    """


def run_with_timeout(
    fn: Callable[..., T], *args: Any, timeout: float, description: str, **kwargs: Any
) -> T:
    """Run ``fn(*args, **kwargs)`` on a worker thread, bounded by ``timeout``.

    Raises :class:`Neo4jOperationTimeout` (chained from the underlying
    ``concurrent.futures.TimeoutError``) if ``fn`` hasn't returned within
    ``timeout`` seconds. Any exception ``fn`` itself raises propagates
    unchanged.
    """
    future = _EXECUTOR.submit(fn, *args, **kwargs)
    try:
        return future.result(timeout=timeout)
    except concurrent.futures.TimeoutError as exc:
        raise Neo4jOperationTimeout(
            f"{description} did not complete within {timeout:.0f}s. "
            "This almost always means another transaction — possibly "
            "leaked from a killed/crashed process — is holding a lock on "
            "the shared Neo4j instance. Diagnose with `SHOW TRANSACTIONS` "
            "(Cypher) and terminate the offending one with "
            "`TERMINATE TRANSACTIONS '<transactionId>'` if it is stale."
        ) from exc
