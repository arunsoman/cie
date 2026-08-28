"""Three-level write-behind/read-through cache for cie's Neo4j reads and
writes (memory -> local disk -> Neo4j).

Implements docs/plans/graph-write-behind-cache-plan.md phases 1-2: this
module builds the two cache shapes (entity cache for by-id property
writes, query cache for traversal/search reads) and is not wired into
anything by importing it alone — see cie/task_repository.py and
cie/neo4j_repository.py for the actual call sites (plan phases 3-5).

Both caches share the same three physical layers:
  L1 - a per-process dict (exact, in-memory, fastest).
  L2 - one SQLite file per project, WAL mode + busy_timeout so concurrent
       threads don't hit "database is locked" (plan §7 Q1) -
       `<workspace>/<project>/.cie/graph_cache.db` (cie's own on-disk cache
       location; was ``.forge/`` when cie reused a host pipeline's workspace
       layout — see ``CIE_WORKSPACE_ROOT``).
  L3 - the real Neo4j database.
"""
from __future__ import annotations

import dataclasses
import functools
import importlib
import inspect
import json
import os
import sqlite3
import sys
import threading
import time
from collections import OrderedDict
from enum import Enum
from pathlib import Path
from typing import Any, Callable, Optional

#: Overridable for tests so a project's cache never touches the real repo
#: root's cie workspace directory.
#:
#: ``FORGE_WORKSPACE_ROOT`` is still read as a deprecated back-compat alias
#: (cie was once embedded in the forge pipeline and reused its workspace
#: layout); new deployments should set ``CIE_WORKSPACE_ROOT``.
_env_ws = (
    os.environ.get("CIE_WORKSPACE_ROOT")
    or os.environ.get("FORGE_WORKSPACE_ROOT")
)
WORKSPACE_ROOT = Path(_env_ws or "./.cie_workspaces")

DEFAULT_FLUSH_INTERVAL_S = 3.0
DEFAULT_FLUSH_THRESHOLD = 20
DEFAULT_QUERY_TTL_S = 5.0
DEFAULT_QUERY_MAX_ENTRIES = 2000


def _warn(msg: str) -> None:
    print(f"[graph_cache] WARNING: {msg}", file=sys.stderr)


def default_cache_db_path(project: str) -> Path:
    """`<workspace>/<project>/.cie/graph_cache.db` — cie's own on-disk cache
    location (was ``.forge/`` when cie reused a host pipeline's workspace
    layout; moved to a cie-owned path)."""
    return WORKSPACE_ROOT / (project or "_default") / ".cie" / "graph_cache.db"


# ---------------------------------------------------------------------------
# L2 connection helper — one sqlite3.Connection per (thread, db_path), WAL
# mode + busy_timeout (plan §7 Q1: default rollback-journal SQLite throws
# "database is locked" under concurrent writers without these two pragmas).
# ---------------------------------------------------------------------------

_local = threading.local()


def _connect(db_path: Path) -> sqlite3.Connection:
    cache: dict[str, sqlite3.Connection] = getattr(_local, "connections", None)
    if cache is None:
        cache = {}
        _local.connections = cache
    key = str(db_path)
    conn = cache.get(key)
    if conn is None:
        db_path.parent.mkdir(parents=True, exist_ok=True)
        conn = sqlite3.connect(str(db_path))
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("PRAGMA busy_timeout=5000")
        conn.execute(
            "CREATE TABLE IF NOT EXISTS pending_writes ("
            "  seq INTEGER PRIMARY KEY AUTOINCREMENT,"
            "  label TEXT NOT NULL,"
            "  entity_id TEXT NOT NULL,"
            "  props_json TEXT NOT NULL,"
            "  counters_json TEXT NOT NULL,"
            "  created_at REAL NOT NULL"
            ")"
        )
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_pending_writes_label "
            "ON pending_writes(label, entity_id, seq)"
        )
        # Real columns, not a parsed-repr() key (plan §2's "gap found this
        # round" — a cache_key-only row can't support TTL/label lookups).
        conn.execute(
            "CREATE TABLE IF NOT EXISTS query_cache ("
            "  cache_key TEXT PRIMARY KEY,"
            "  project TEXT NOT NULL,"
            "  method_name TEXT NOT NULL,"
            "  labels_read TEXT NOT NULL,"
            "  result_json TEXT NOT NULL,"
            "  cached_at REAL NOT NULL"
            ")"
        )
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_query_cache_project_labels "
            "ON query_cache(project, labels_read)"
        )
        conn.commit()
        cache[key] = conn
    return conn


# ===========================================================================
# Entity cache — exact, by-(label, id) reads/writes (plan §2/§3).
# ===========================================================================


class EntityCache:
    """Write-behind cache for by-(label, id) property writes.

    L1 is two dicts guarded by one lock: ``_entity[(label, id)] ->
    {merged pending props}`` and ``_counters[(label, id)] -> {counter
    name -> summed delta}`` (props overlay-merge last-write-wins;
    counters ADD across coalesced writes so a burst of
    ``set_status_cached`` calls for the same task still increments
    ``attempts`` by the right total instead of collapsing to +1 — see
    the plan's §3 durability discussion, extended here since the plan's
    literal "props dict overlay" shape alone would have silently
    undercounted attempts).

    Every ``write()`` call mirrors synchronously to L2 (an INSERT, not a
    read-modify-write, so concurrent writers to the same key never race
    each other in SQLite — durability boundary is this INSERT's commit)
    BEFORE acquiring the lock to mutate L1 — see the "which lock, and in
    what order" correction in plan §3. Only the one dedicated flusher
    thread ever executes a flush (worker threads that cross the
    usage-based threshold just wake it); the flush snapshot is a full
    dict swap under the lock (cheap, O(1)), which gives the same
    guarantee the plan's per-key-snapshot prose describes — a write that
    lands mid-flush goes into the fresh post-swap dict, never lost.
    """

    def __init__(
        self,
        project: str,
        db_path: Optional[Path] = None,
        flush_interval_s: float = DEFAULT_FLUSH_INTERVAL_S,
        flush_threshold: int = DEFAULT_FLUSH_THRESHOLD,
        on_write_label: Optional[Callable[[str], None]] = None,
    ):
        self.project = project
        self._db_path = db_path or default_cache_db_path(project)
        self._flush_interval_s = flush_interval_s
        self._flush_threshold = flush_threshold
        #: Called (label,) after every write()'s L1 mutation — the
        #: label-scoped query-cache invalidation hook (plan §4). None
        #: means "no query cache wired for this project" (fine — TTL
        #: alone still bounds staleness).
        self._on_write_label = on_write_label

        self._lock = threading.Lock()
        self._entity: dict[tuple[str, str], dict] = {}
        self._counters: dict[tuple[str, str], dict[str, int]] = {}
        #: seq -> (label, id) for every L2 row currently represented in
        #: the L1 dicts above — lets a flush delete exactly the L2 rows
        #: it just applied, nothing more, nothing less (plan §3's "gap
        #: found this round": pending_writes must shrink, not just grow).
        self._pending_seqs: dict[tuple[str, str], list[int]] = {}
        self._pending_count = 0

        self._flush_fns: dict[str, Callable[[list[dict]], None]] = {}

        self._wake = threading.Event()
        self._stop = threading.Event()
        self._flusher = threading.Thread(
            target=self._flush_loop, name=f"entity-cache-flusher[{project}]", daemon=True,
        )
        self._flusher.start()

    # -- registration / crash recovery -------------------------------------

    def register_flush(self, label: str, fn: Callable[[list[dict]], None]) -> None:
        """Register the batched-write callback for `label` (receives
        ``[{"id": ..., "props": {...}, "counters": {...}}, ...]`` and must
        write it to Neo4j, e.g. via one UNWIND). Replays any of this
        label's unflushed L2 rows into Neo4j immediately, synchronously,
        before returning — "before anything else in this process touches
        that project's cache" (plan §3's crash-recovery contract), scoped
        to whichever label is being registered right now.
        """
        with self._lock:
            self._flush_fns[label] = fn
        self._replay_label(label, fn)

    def _replay_label(self, label: str, fn: Callable[[list[dict]], None]) -> None:
        conn = _connect(self._db_path)
        rows = conn.execute(
            "SELECT seq, entity_id, props_json, counters_json FROM pending_writes "
            "WHERE label = ? ORDER BY seq ASC",
            (label,),
        ).fetchall()
        if not rows:
            return
        merged: "OrderedDict[str, dict]" = OrderedDict()
        seqs: list[int] = []
        for seq, entity_id, props_json, counters_json in rows:
            seqs.append(seq)
            entry = merged.setdefault(entity_id, {"props": {}, "counters": {}})
            entry["props"].update(json.loads(props_json))
            for k, v in json.loads(counters_json).items():
                entry["counters"][k] = entry["counters"].get(k, 0) + v
        batch = [
            {"id": entity_id, "props": e["props"], "counters": e["counters"]}
            for entity_id, e in merged.items()
        ]
        try:
            fn(batch)
        except Exception as exc:  # noqa: BLE001 — surfaced as a warning, not
            # fatal: leaving the rows in L2 means the next open (or the
            # regular flusher, once a matching in-memory entry exists) gets
            # another chance instead of silently losing crash-recovery data.
            _warn(f"crash-recovery replay failed for label {label!r} "
                  f"({self.project!r}): {exc}")
            return
        placeholders = ",".join("?" * len(seqs))
        conn.execute(f"DELETE FROM pending_writes WHERE seq IN ({placeholders})", seqs)
        conn.commit()

    # -- write path ----------------------------------------------------------

    def write(self, label: str, entity_id: str, props: dict, counters: Optional[dict] = None) -> None:
        """Queue a property write for `(label, entity_id)`. Durable the
        moment this call returns (L2 mirror committed first); visible to
        `read()` in this process immediately (L1 overlay)."""
        counters = counters or {}
        now = time.time()
        conn = _connect(self._db_path)
        cur = conn.execute(
            "INSERT INTO pending_writes (label, entity_id, props_json, counters_json, created_at) "
            "VALUES (?, ?, ?, ?, ?)",
            (label, entity_id, json.dumps(props), json.dumps(counters), now),
        )
        conn.commit()
        seq = cur.lastrowid

        key = (label, entity_id)
        with self._lock:
            self._entity.setdefault(key, {}).update(props)
            entry_counters = self._counters.setdefault(key, {})
            for k, v in counters.items():
                entry_counters[k] = entry_counters.get(k, 0) + v
            self._pending_seqs.setdefault(key, []).append(seq)
            self._pending_count += 1
            should_wake = self._pending_count >= self._flush_threshold

        if self._on_write_label is not None:
            try:
                self._on_write_label(label)
            except Exception as exc:  # noqa: BLE001 — invalidation is a
                # correctness nicety (bounded by TTL either way), never
                # allowed to break the write path itself.
                _warn(f"query-cache invalidation hook failed for label {label!r}: {exc}")

        if should_wake:
            self._wake.set()

    def read(self, label: str, entity_id: str) -> Optional[dict]:
        """Pending-write overlay for `(label, entity_id)`, or None if
        nothing is queued (caller falls through to L3). Exact — a read
        for a node this process has a pending write for returns that
        pending value (plan §2's entity-cache consistency guarantee)."""
        with self._lock:
            entry = self._entity.get((label, entity_id))
            return dict(entry) if entry is not None else None

    # -- flush -----------------------------------------------------------

    def _flush_loop(self) -> None:
        while not self._stop.is_set():
            woke = self._wake.wait(timeout=self._flush_interval_s)
            if self._stop.is_set():
                break
            self._wake.clear()
            self._do_flush()

    def _do_flush(self) -> None:
        with self._lock:
            if not self._entity:
                return
            snap_entity, self._entity = self._entity, {}
            snap_counters, self._counters = self._counters, {}
            snap_seqs, self._pending_seqs = self._pending_seqs, {}
            self._pending_count = 0
            flush_fns = dict(self._flush_fns)

        by_label: dict[str, list[dict]] = {}
        for (label, entity_id), props in snap_entity.items():
            by_label.setdefault(label, []).append({
                "id": entity_id, "props": props,
                "counters": snap_counters.get((label, entity_id), {}),
            })

        flushed_seqs: list[int] = []
        failed: dict[tuple[str, str], dict] = {}
        failed_counters: dict[tuple[str, str], dict] = {}
        failed_seqs: dict[tuple[str, str], list[int]] = {}
        for label, rows in by_label.items():
            fn = flush_fns.get(label)
            if fn is None:
                # No writer registered for this label yet — keep it pending
                # rather than dropping it silently.
                for row in rows:
                    key = (label, row["id"])
                    failed[key] = row["props"]
                    failed_counters[key] = row["counters"]
                    failed_seqs[key] = snap_seqs.get(key, [])
                continue
            try:
                fn(rows)
            except Exception as exc:  # noqa: BLE001 — a flush failure must
                # never crash the flusher thread; the rows survive into the
                # next flush attempt instead (merged back into L1 below).
                _warn(f"flush failed for label {label!r} ({self.project!r}): {exc}")
                for row in rows:
                    key = (label, row["id"])
                    failed[key] = row["props"]
                    failed_counters[key] = row["counters"]
                    failed_seqs[key] = snap_seqs.get(key, [])
                continue
            for row in rows:
                flushed_seqs.extend(snap_seqs.get((label, row["id"]), []))

        if flushed_seqs:
            conn = _connect(self._db_path)
            placeholders = ",".join("?" * len(flushed_seqs))
            conn.execute(f"DELETE FROM pending_writes WHERE seq IN ({placeholders})", flushed_seqs)
            conn.commit()

        if failed:
            with self._lock:
                for key, props in failed.items():
                    self._entity.setdefault(key, {})
                    # A newer write may have arrived on this key since the
                    # snapshot — merge the failed (older) props underneath
                    # it rather than clobbering, then re-merge the newer
                    # ones back on top.
                    merged = dict(props)
                    merged.update(self._entity[key])
                    self._entity[key] = merged
                    entry_counters = self._counters.setdefault(key, {})
                    for k, v in failed_counters[key].items():
                        entry_counters[k] = entry_counters.get(k, 0) + v
                    self._pending_seqs.setdefault(key, [])
                    self._pending_seqs[key] = failed_seqs[key] + self._pending_seqs[key]
                    self._pending_count += 1

    def flush_key(self, label: str, entity_id: str) -> None:
        """Synchronously flush just this one key, right now, on the
        caller's thread. For the rare same-call case where a write needs
        its node to exist immediately for a follow-up MATCH (e.g.
        push_tasks attaching an ApiSpec/TestTriad/dependency edge to the
        AtomicTask node it just wrote) — everything else stays on the
        deferred/batched path."""
        key = (label, entity_id)
        with self._lock:
            props = self._entity.pop(key, None)
            if props is None:
                return
            counters = self._counters.pop(key, {})
            seqs = self._pending_seqs.pop(key, [])
            self._pending_count = max(0, self._pending_count - 1)
            fn = self._flush_fns.get(label)

        if fn is None:
            # Put it back — nothing can write it yet.
            with self._lock:
                self._entity[key] = props
                self._counters[key] = counters
                self._pending_seqs[key] = seqs
                self._pending_count += 1
            return

        row = {"id": entity_id, "props": props, "counters": counters}
        try:
            fn([row])
        except Exception as exc:  # noqa: BLE001 — same recovery contract as _do_flush
            _warn(f"flush_key failed for {label}/{entity_id} ({self.project!r}): {exc}")
            with self._lock:
                merged = dict(props)
                merged.update(self._entity.get(key, {}))
                self._entity[key] = merged
                entry_counters = self._counters.setdefault(key, {})
                for k, v in counters.items():
                    entry_counters[k] = entry_counters.get(k, 0) + v
                self._pending_seqs.setdefault(key, [])
                self._pending_seqs[key] = seqs + self._pending_seqs[key]
                self._pending_count += 1
            return

        if seqs:
            conn = _connect(self._db_path)
            placeholders = ",".join("?" * len(seqs))
            conn.execute(f"DELETE FROM pending_writes WHERE seq IN ({placeholders})", seqs)
            conn.commit()

    def flush_sync(self) -> None:
        """Flush everything on the caller's thread — graceful shutdown."""
        self._do_flush()

    def close(self) -> None:
        self.flush_sync()
        self._stop.set()
        self._wake.set()
        self._flusher.join(timeout=5.0)


# ===========================================================================
# L2 result codec — cached query results are frozen dataclasses (cie.models
# — Node, ContextHit, SymbolMatch, ...) and str-backed Enums, neither of
# which `json.dumps` handles on its own. L1 needs no codec at all (it holds
# the live Python objects the method actually returned); only L2's
# `result_json` column round-trips through this. Generic over any current
# or future cie.models dataclass/enum shape, not hand-written per method,
# since Phase 5's own plan explicitly anticipates "the rest of its ~30 read
# methods" eventually going through the same decorator.
# ===========================================================================


def _encode_result(v: Any) -> Any:
    if dataclasses.is_dataclass(v) and not isinstance(v, type):
        cls = type(v)
        return {
            "__dataclass__": f"{cls.__module__}.{cls.__qualname__}",
            "fields": {f.name: _encode_result(getattr(v, f.name)) for f in dataclasses.fields(v)},
        }
    if isinstance(v, Enum):
        cls = type(v)
        return {"__enum__": f"{cls.__module__}.{cls.__qualname__}", "value": v.value}
    if isinstance(v, tuple):
        return {"__tuple__": [_encode_result(x) for x in v]}
    if isinstance(v, list):
        return [_encode_result(x) for x in v]
    if isinstance(v, dict):
        return {k: _encode_result(x) for k, x in v.items()}
    return v


def _import_symbol(dotted: str) -> Any:
    module_name, _, qualname = dotted.rpartition(".")
    obj: Any = importlib.import_module(module_name)
    for part in qualname.split("."):
        obj = getattr(obj, part)
    return obj


def _decode_result(v: Any) -> Any:
    if isinstance(v, dict):
        if "__dataclass__" in v:
            cls = _import_symbol(v["__dataclass__"])
            fields = {k: _decode_result(x) for k, x in v["fields"].items()}
            return cls(**fields)
        if "__enum__" in v:
            cls = _import_symbol(v["__enum__"])
            return cls(v["value"])
        if "__tuple__" in v:
            return tuple(_decode_result(x) for x in v["__tuple__"])
        return {k: _decode_result(x) for k, x in v.items()}
    if isinstance(v, list):
        return [_decode_result(x) for x in v]
    return v


# ===========================================================================
# Query cache — TTL + label-scoped invalidation (plan §2/§4).
# ===========================================================================


class QueryCache:
    """TTL + label-scoped-invalidation cache for traversal/search reads
    that don't key on a single (label, id).

    L1 is an ``OrderedDict`` used as a bounded LRU (plan §2: "`query`
    needs an explicit size bound... not just lazy TTL-expiry-on-next-
    lookup" — the exact OOM-kill failure mode `core/tasks/registry.py`
    hit live, per §1). L2 gives every row real ``project``/
    ``labels_read``/``cached_at`` columns (not a parsed-repr() key —
    see plan §2's "gap found this round") so both TTL checks and
    label-scoped invalidation can query them directly.
    """

    def __init__(
        self,
        project: str,
        db_path: Optional[Path] = None,
        ttl_s: float = DEFAULT_QUERY_TTL_S,
        max_entries: int = DEFAULT_QUERY_MAX_ENTRIES,
    ):
        self.project = project
        self._db_path = db_path or default_cache_db_path(project)
        self._ttl_s = ttl_s
        self._max_entries = max_entries
        self._lock = threading.Lock()
        #: cache_key -> (result, cached_at, labels_read: frozenset[str])
        self._query: "OrderedDict[Any, tuple[Any, float, frozenset]]" = OrderedDict()

    def get(self, cache_key: Any) -> Optional[Any]:
        with self._lock:
            entry = self._query.get(cache_key)
            if entry is not None:
                result, cached_at, _labels = entry
                if time.time() - cached_at <= self._ttl_s:
                    self._query.move_to_end(cache_key)
                    return result
                del self._query[cache_key]
                # fall through to L2 in case a fresher L2 row exists
                # (shouldn't normally happen, but never serve stale-L1
                # over fresh-L2 if both are present)

        conn = _connect(self._db_path)
        row = conn.execute(
            "SELECT result_json, labels_read, cached_at FROM query_cache WHERE cache_key = ?",
            (repr(cache_key),),
        ).fetchone()
        if row is None:
            return None
        result_json, labels_read, cached_at = row
        if time.time() - cached_at > self._ttl_s:
            return None
        result = _decode_result(json.loads(result_json))
        labels = frozenset(labels_read.split(",")) if labels_read else frozenset()
        # Narrow residual race, accepted rather than solved: an
        # invalidate_labels() for this same key could run between the L2
        # SELECT just above and _promote() below, deleting the L2 row
        # this call already read — _promote() would then re-seed L1 with
        # a result that's simultaneously being invalidated. Unversioned,
        # so not detectable here; same class of accepted staleness
        # window the plan calls out for cross-process reads (§5) and
        # same-process query-vs-pending-entity-write races (§4's
        # "Cross-process note" correction) — bounded by TTL either way,
        # and not reachable by any write path wired in phases 3-5 today
        # (those only ever write the AtomicTask label; nothing cached by
        # @cached_query in this phase reads it).
        self._promote(cache_key, result, cached_at, labels)
        return result

    def _promote(self, cache_key: Any, result: Any, cached_at: float, labels: frozenset) -> None:
        with self._lock:
            self._query[cache_key] = (result, cached_at, labels)
            self._query.move_to_end(cache_key)
            self._evict_if_needed()

    def _evict_if_needed(self) -> None:
        # Caller already holds self._lock.
        while len(self._query) > self._max_entries:
            self._query.popitem(last=False)

    def put(self, cache_key: Any, method_name: str, labels_read: frozenset, result: Any) -> None:
        cached_at = time.time()
        with self._lock:
            self._query[cache_key] = (result, cached_at, labels_read)
            self._query.move_to_end(cache_key)
            self._evict_if_needed()

        conn = _connect(self._db_path)
        conn.execute(
            "INSERT OR REPLACE INTO query_cache "
            "(cache_key, project, method_name, labels_read, result_json, cached_at) "
            "VALUES (?, ?, ?, ?, ?, ?)",
            (repr(cache_key), self.project, method_name, ",".join(sorted(labels_read)),
             json.dumps(_encode_result(result)), cached_at),
        )
        conn.commit()

    def invalidate_labels(self, project: str, labels: set) -> None:
        """Drop every cached query (L1 and L2) whose declared label-set
        intersects `labels`, scoped to `project` (plan §4: over-
        invalidating across projects is wasteful but not wrong; under-
        invalidating IS wrong, so project-scoping this matters).

        L1 and L2 are cleared under the SAME lock acquisition as one
        unit, not as two independent steps — `get()`'s L1 check also
        takes this lock, so a concurrent `get()` either observes the
        full pre-invalidation state (a legitimate, still-valid-at-that-
        instant read) or the fully-post-invalidation state (L1 miss,
        and L2 already clean too, so it correctly falls through to L3
        instead of re-promoting a row this method deleted a moment
        earlier — an interleaving where L1 is cleared but L2 isn't yet
        would let exactly that stale re-promotion happen, live)."""
        if project != self.project:
            return
        with self._lock:
            stale = [k for k, (_r, _c, l) in self._query.items() if l & labels]
            for k in stale:
                del self._query[k]

            conn = _connect(self._db_path)
            rows = conn.execute(
                "SELECT cache_key, labels_read FROM query_cache WHERE project = ?", (project,),
            ).fetchall()
            doomed = [
                cache_key for cache_key, labels_read in rows
                if set(labels_read.split(",")) & labels
            ]
            if doomed:
                placeholders = ",".join("?" * len(doomed))
                conn.execute(f"DELETE FROM query_cache WHERE cache_key IN ({placeholders})", doomed)
                conn.commit()


# ===========================================================================
# cached_query — key-building + decorator (plan §4).
# ===========================================================================


def _hashable(v: Any) -> Any:
    """Normalize a bound argument value so it can sit inside a tuple cache
    key — lists/tuples become tuples (recursively), dicts become sorted
    (key, value) tuples (recursively)."""
    if isinstance(v, (list, tuple)):
        return tuple(_hashable(x) for x in v)
    if isinstance(v, dict):
        return tuple(sorted((k, _hashable(x)) for k, x in v.items()))
    return v


def cached_query(labels: frozenset = frozenset()):
    """Decorator for a `Neo4jRepository` read method with no single
    (label, id) to key on. `labels` is the set of node labels this
    method's Cypher reads from — used for label-scoped invalidation
    (plan §4).

    Key: `(self._project, method_name, tuple(sorted(bound_args.items())))`
    — see plan §4 for why each step below is load-bearing (positional-arg
    binding, `self` handling, recursive hashability normalization,
    `apply_defaults()`, and folding a method's own `project` override
    kwarg into `self._project` instead of keying on both independently).

    Caching is opt-in per instance, off by default: the wrapper only
    caches when `self._query_cache` is a real `QueryCache` (set by
    `cie.factory.get_engine()`, the one production construction path).
    A bare `Neo4jRepository(driver, ...)` — every existing test in this
    suite, and any other direct construction — has `self._query_cache =
    None` and calls straight through, byte-identical to this class's
    behavior before this cache existed. Without this, EVERY direct
    construction would silently share the same per-process, per-project
    `QueryCache` singleton (keyed only on `self._project`, which
    defaults to `""` for nearly every test in this suite) — one test's
    fake driver response would leak into a completely unrelated test
    that happens to call the same method with the same args.
    """

    def decorator(method):
        sig = inspect.signature(method)  # raw function — `self` is param 1

        @functools.wraps(method)
        def wrapper(self, *args, **kwargs):
            cache = getattr(self, "_query_cache", None)
            if cache is None:
                return method(self, *args, **kwargs)

            bound = sig.bind(self, *args, **kwargs)
            bound.apply_defaults()
            bound_args = dict(bound.arguments)
            del bound_args["self"]
            bound_args = {k: _hashable(v) for k, v in bound_args.items()}
            project = bound_args.pop("project", None) or self._project
            cache_key = (project, method.__name__, tuple(sorted(bound_args.items())))

            hit = cache.get(cache_key)
            if hit is not None:
                return hit

            result = method(self, *args, **kwargs)
            cache.put(cache_key, method.__name__, labels, result)
            return result

        wrapper._cached_query_labels = labels  # type: ignore[attr-defined]
        return wrapper

    return decorator


# ===========================================================================
# Per-project singletons — mirrors cie/factory.py's engine/task-repo cache.
# ===========================================================================

_entity_caches: dict[str, EntityCache] = {}
_query_caches: dict[str, QueryCache] = {}
_registry_lock = threading.Lock()


def get_entity_cache(
    project: str = "",
    flush_fns: Optional[dict[str, Callable[[list[dict]], None]]] = None,
) -> EntityCache:
    """Process-wide EntityCache for one project namespace, cached — the
    same instance is shared by every writer for that project (e.g.
    CieReporter.status() and Neo4jTaskRepository.push_tasks both write
    the "AtomicTask" label into the SAME cache, so their writes coalesce
    into the same flush instead of each keeping a private, unrelated
    cache). `flush_fns` is applied (via `register_flush`, which also
    replays any of that label's already-pending L2 rows) on every call —
    harmless/idempotent to re-register the same label twice."""
    with _registry_lock:
        cache = _entity_caches.get(project)
        if cache is None:
            cache = EntityCache(
                project,
                on_write_label=lambda label, _p=project: get_query_cache(_p).invalidate_labels(_p, {label}),
            )
            _entity_caches[project] = cache
    for label, fn in (flush_fns or {}).items():
        cache.register_flush(label, fn)
    return cache


def get_query_cache(project: str = "") -> QueryCache:
    """Process-wide QueryCache for one project namespace, cached."""
    with _registry_lock:
        cache = _query_caches.get(project)
        if cache is None:
            cache = QueryCache(project)
            _query_caches[project] = cache
        return cache


def reset_caches() -> None:
    """Test/reload hook — mirrors cie/factory.py::reset_caches(). Closes
    every entity cache's flusher thread before dropping references."""
    with _registry_lock:
        caches = list(_entity_caches.values())
        _entity_caches.clear()
        _query_caches.clear()
    for cache in caches:
        try:
            cache.close()
        except Exception:  # noqa: BLE001 — best-effort teardown
            pass
