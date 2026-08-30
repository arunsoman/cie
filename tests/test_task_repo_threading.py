"""The embedded task repository must survive cross-thread use.

Found on the LIVE MCP surface (tool-test-lab/surface_conformance.py,
2026-08-30): `get_task`, `list_pending_tasks`, `task_dependency_closure`
and `blame_history` all crashed with `ProgrammingError: SQLite objects
created in a thread can only be used in that same thread` — cie-mcp
builds the repository's connection once at startup (main thread) and
then runs tool handlers in anyio worker threads. In-process test suites
never catch this because everything there runs in the main thread.

The fix makes the connection `check_same_thread=False` behind a shared
lock (`_ThreadSafeSQLite`). This file pins the cross-thread contract:
one repository instance hammered by several threads must not raise.
"""

from __future__ import annotations

import threading

import pytest

from cie.embedded_task_repository import EmbeddedTaskRepository, _ThreadSafeSQLite


@pytest.fixture()
def repo(tmp_path):
    return EmbeddedTaskRepository(tmp_path / ".cie" / "tasks.db", project="thread-test")


def test_threadsafe_connection_accepts_other_threads(tmp_path):
    """Unit: the wrapper connection itself usable from a non-creator thread."""
    conn = _ThreadSafeSQLite(str(tmp_path / "t.db"))

    errors: list[Exception] = []

    def hammer() -> None:
        try:
            cur = conn.execute("SELECT 1")
            cur.fetchall()
            conn.commit()
        except Exception as exc:  # noqa: BLE001
            errors.append(exc)

    threads = [threading.Thread(target=hammer) for _ in range(6)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    assert errors == [], f"cross-thread sqlite use raised: {errors}"


def test_one_repo_instance_hammered_from_many_threads(repo):
    """The live crash was exactly this shape: ONE repo, N worker threads."""
    errors: list[Exception] = []

    def reader() -> None:
        try:
            for _ in range(25):
                repo.list_pending()
                repo.get_task("no-such-task")
        except Exception as exc:  # noqa: BLE001
            errors.append(exc)

    threads = [threading.Thread(target=reader) for _ in range(6)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    assert errors == [], f"concurrent task-repo reads raised: {errors[:3]}"


def test_concurrent_reads_are_consistent(repo):
    """Same ops, same answer, from different threads — no corruption."""
    expected = repo.list_pending()

    results: list[list | Exception] = []
    results_lock = threading.Lock()

    def reader() -> None:
        try:
            results_lock.acquire()
            results.append(repo.list_pending())
        except Exception as exc:  # noqa: BLE001 - collected, not raised
            results.append(exc)
        finally:
            results_lock.release()

    threads = [threading.Thread(target=reader) for _ in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    for r in results:
        assert not isinstance(r, Exception)
        assert len(r) == len(expected)


@pytest.mark.parametrize("exc_builder", [lambda: RuntimeError("boom")])
def test_proxy_still_passes_through_normal_errors(tmp_path, exc_builder):
    """The wrapper must not swallow sqlite semantics: a bad SQL statement
    still raises, lock or not."""
    conn = _ThreadSafeSQLite(str(tmp_path / "e.db"))
    with pytest.raises(Exception):
        conn.execute("SELECT * FROM no_such_table")