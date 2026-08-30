"""HTTP tool-policy tests — the read-only-by-default guarantee for cie's
one less-trusted external surface.

`cie.tool_policy` existed as a classification with no real caller; the
HTTP surface (`cie/routes.py`) now routes every `POST /tools/{tool}`
dispatch and every mutating legacy REST route through it (2026-08-30 —
previously `run_tool` dispatched writes like `edit_file`/`delete_file`/
`apply_patch`/`run` straight to `ToolService` with no authorization, and
the task/hierarchy alias handlers were entirely outside `WRITE_TOOLS`).

These tests pin the four properties that make that adoption enforce
something rather than decorate it:

1. write tools are rejected server-side (403) no matter who the connecting
   client claims to be — including the bespoke alias handlers that
   `WRITE_TOOLS` cannot see;
2. discovery (`GET /tools`) never shows a tool the dispatcher would reject;
3. mutating legacy REST routes require an explicit write-policy opt-in;
4. a mutating request carrying a cross-origin `Origin` is rejected even
   when writes are allowed (the CSRF-to-localhost vector — a text/plain
   POST needs no CORS preflight to have side effects server-side).

Requires fastapi (the `[http]` extra) + httpx (starlette TestClient
transport); skipped cleanly when absent, mirroring
test_optional_dependency_envelope's approach. No Neo4j needed — the 403s
fire before any handler (and so before any backend touch), and the
write-allowed success path stubs its alias handler and never leaves the
process.
"""

from __future__ import annotations

import pytest

pytest.importorskip("fastapi")
pytest.importorskip("httpx")

from fastapi import FastAPI
from fastapi.testclient import TestClient

from cie import routes
from cie.envelope import envelope
from cie.tools import ToolService


@pytest.fixture()
def client(monkeypatch):
    """TestClient over the real router, with the HTTP env knobs cleared so
    every test starts from the shipped default (read-only inspector)."""
    monkeypatch.delenv("CIE_HTTP_POLICY", raising=False)
    monkeypatch.delenv("CIE_HTTP_ALLOW_WRITE", raising=False)
    monkeypatch.delenv("CIE_HTTP_ALLOWED_ORIGINS", raising=False)
    app = FastAPI()
    app.include_router(routes.router)
    return TestClient(app)


# ---------------------------------------------------------------------------
# 1. Write tools rejected by default — read-only, server-side
# ---------------------------------------------------------------------------


def test_write_tool_rejected_by_default(client):
    """A filesystem-mutating tool is 403 before the handler (and so before
    any graph/filesystem touch) under the default read-only policy."""
    r = client.post("/tools/edit_file", json={"relative_path": "x.py"})
    assert r.status_code == 403
    body = r.json()
    assert body["ok"] is False
    assert body["error"]["kind"] == "forbidden"
    assert body["error"]["message"].startswith("tool 'edit_file' not permitted")
    assert "CIE_HTTP_POLICY" in body.get("hint", "")


def test_write_alias_rejected_by_default(client):
    """`push_tasks` mutates the task repository but is a bespoke handler —
    invisible to WRITE_TOOLS. `HTTP_WRITE_ALIASES` must close exactly that
    gap, or the read-only default is fiction for task writes."""
    r = client.post("/tools/push_tasks", json={})
    assert r.status_code == 403
    body = r.json()
    assert body["error"]["kind"] == "forbidden"
    assert "not permitted" in body["error"]["message"]


def test_unknown_tool_is_404_not_403(client):
    """Authorization must not confuse a never-registered tool name with a
    policy denial: 404 first, and no policy leak about registered names."""
    r = client.post("/tools/definitely_not_a_tool", json={})
    assert r.status_code == 404
    assert r.json()["error"]["kind"] == "not_found"


def test_read_tool_passes_the_policy_gate(client):
    """An authorized read tool passes the gate — authorization failing
    closed for writes must not also fail the reads. Asserted as not-403:
    without a live Neo4j the engine falls back to its heuristic index, so
    the response may be anything the backend legitimately produces (200
    results, 404 validation, 503) — what it must NOT be is a policy
    denial."""
    r = client.post("/tools/search_symbol", json={"name": "nonexistent_sym"})
    assert r.status_code != 403
    error = r.json().get("error")
    if error is not None:
        assert error.get("kind") != "forbidden"


# ---------------------------------------------------------------------------
# 2. Discovery filtered to match the dispatcher
# ---------------------------------------------------------------------------


def _manifest_names(client, path: str = "/tools") -> set[str]:
    r = client.get(path)
    assert r.status_code == 200, r.text
    return {t["name"] for t in r.json()["results"]}


def test_get_tools_hides_write_tools_by_default(client):
    names = _manifest_names(client)
    for write_tool in ("edit_file", "delete_file", "apply_patch", "run", "reindex"):
        assert write_tool not in names
    assert "search_symbol" in names  # read tools still discoverable
    assert client.get("/tools").json()["http_policy"] == {
        "agent_type": "inspector", "allow_write": False,
    }


def test_get_tools_shows_write_tools_when_allowed(client, monkeypatch):
    monkeypatch.setenv("CIE_HTTP_POLICY", "orchestrator")
    names = _manifest_names(client)
    assert "edit_file" in names
    assert client.get("/tools").json()["http_policy"]["allow_write"] is True


# ---------------------------------------------------------------------------
# 3. The explicit opt-in actually works (env override), for ToolService
#    methods and alias handlers alike — with the success path stubbed so
#    nothing real is written by the test
# ---------------------------------------------------------------------------


def stub_alias_push_tasks(monkeypatch) -> None:
    monkeypatch.setitem(
        routes.TOOLS, "push_tasks",
        lambda kwargs, project: envelope("push_tasks", {"pushed": 1}),
    )


def test_cie_http_policy_orchestrator_allows_writes(client, monkeypatch):
    monkeypatch.setenv("CIE_HTTP_POLICY", "orchestrator")
    stub_alias_push_tasks(monkeypatch)
    r = client.post("/tools/push_tasks", json={})
    assert r.status_code == 200
    assert r.json()["ok"] is True


def test_cie_http_allow_write_env_shortcut(client, monkeypatch):
    monkeypatch.setenv("CIE_HTTP_ALLOW_WRITE", "1")
    stub_alias_push_tasks(monkeypatch)
    r = client.post("/tools/push_tasks", json={})
    assert r.status_code == 200
    assert r.json()["ok"] is True


def test_cie_http_policy_miner_rejects_writes(client, monkeypatch):
    """The middle policy (REQUIREMENT_MINER: allow_write=False) is reachable
    by name, not just inspector/orchestrator."""
    monkeypatch.setenv("CIE_HTTP_POLICY", "miner")
    assert client.post("/tools/push_tasks", json={}).status_code == 403


# ---------------------------------------------------------------------------
# 4. Cross-origin mutating requests are rejected even when writes are on
# ---------------------------------------------------------------------------


def test_cross_origin_write_rejected(client, monkeypatch):
    """With writes ALLOWED, a browser-page Origin is still refused — this
    is the CSRF-to-localhost vector the text/plain trick enables."""
    monkeypatch.setenv("CIE_HTTP_POLICY", "orchestrator")
    stub_alias_push_tasks(monkeypatch)
    r = client.post("/tools/push_tasks", json={},
                    headers={"Origin": "https://evil.example"})
    assert r.status_code == 403
    assert r.json()["error"]["kind"] == "forbidden"
    assert "cross-origin" in r.json()["error"]["message"].lower()


def test_cross_origin_write_allowed_for_listed_origin(client, monkeypatch):
    monkeypatch.setenv("CIE_HTTP_POLICY", "orchestrator")
    monkeypatch.setenv("CIE_HTTP_ALLOWED_ORIGINS", "https://console.example")
    stub_alias_push_tasks(monkeypatch)
    r = client.post("/tools/push_tasks", json={},
                    headers={"Origin": "https://console.example"})
    assert r.status_code == 200
    assert r.json()["ok"] is True


def test_legacy_rest_writes_gated_by_default(client):
    """Legacy REST task writes get the same 403 (valid body — the guard,
    not a 422, must be what stops the request)."""
    r = client.put("/tasks/whatever/status", json={"status": "done"})
    assert r.status_code == 403


def test_legacy_rest_cross_origin_blocked_even_when_allowed(client, monkeypatch):
    monkeypatch.setenv("CIE_HTTP_POLICY", "orchestrator")
    r = client.put("/tasks/whatever/status", json={"status": "done"},
                   headers={"Origin": "https://evil.example"})
    assert r.status_code == 403


# ---------------------------------------------------------------------------
# 5. Structural invariants — the write-alias set can't silently rot
# ---------------------------------------------------------------------------


def test_write_aliases_never_shadow_toolservice_methods():
    """/tools/{tool} aliases must be exactly the names that are NOT
    ToolService methods — if one ever becomes a real method, authorize()
    already covers it via WRITE_TOOLS and the alias entry is dead (and if
    it isn't in WRITE_TOOLS either, coverage has a hole)."""
    real_tools = {
        name for name in vars(ToolService)
        if not name.startswith("_") and callable(getattr(ToolService, name))
    }
    assert routes.HTTP_WRITE_ALIASES & real_tools == set()


def test_every_write_alias_is_a_registered_dispatcher_key():
    """/tools/{tool} must actually accept every name in the alias set, or
    POSTing it would 404 where the policy says forbidden."""
    assert set(routes.HTTP_WRITE_ALIASES) <= set(routes.TOOLS)


def test_discovery_filter_covers_every_toolservice_write_tool(client):
    """GET /tools under read-only must hide at least WRITE_TOOLS itself —
    i.e. filtering is by the same sets the dispatcher enforces, not a
    third hand-maintained list (that would be the drift bug class this
    file exists to prevent). describe() only lists methods ToolService
    actually has, so the hidden set is the intersection with real method
    names."""
    service_names = {
        name for name in vars(ToolService)
        if not name.startswith("_") and callable(getattr(ToolService, name))
    }
    hidden = routes.WRITE_TOOLS & service_names
    assert hidden  # sanity: WRITE_TOOLS is keyed to real ToolService methods
    names = _manifest_names(client)
    assert not (names & hidden)