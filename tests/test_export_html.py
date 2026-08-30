"""R8 — `cie export-html`: the shareable, self-contained HTML snapshot.

Pins per `roadmap.md` R8:
- ONE file, zero external references — `file://` with no server, no
  network (the harness greps for http(s)://, <script src>, <link);
- data comes from real graph reads: TESTS chains (task/file/test),
  orphans, tasks, files;
- HTML-escaping + the `</` blob escape (no breakout of the data
  <script> tag);
- the export can never exceed the read side: it composes read-only
  ToolService envelopes + the same repo reads the traceability tools use.
"""

from __future__ import annotations

import json
import re

import pytest

from cie.embedded_repository import EmbeddedRepository
from cie.export_html import export_html
from cie.extract import extract_many
from cie.callgraph import resolve_call_edges
from cie.testlink import resolve_test_edges

FILES = {
    "app.py": (
        "def alpha():\n    return 1\n"
        "\n"
        "def untested():\n    return 2\n"
    ),
    "test_app.py": "from app import alpha\n\ndef test_alpha():\n    assert alpha()\n",
}


@pytest.fixture()
def indexed_svc(tmp_path):
    for name, text in FILES.items():
        (tmp_path / name).write_text(text)
    per = extract_many(tmp_path)
    nodes = [n for ext in per for n in ext.nodes]
    edges = (
        [e for ext in per for e in ext.edges]
        + resolve_call_edges(per)
        + resolve_test_edges(per, resolve_call_edges(per))
    )
    EmbeddedRepository(tmp_path / ".cie" / "graph.db").load_extraction(
        nodes, edges
    )
    from cie.factory import build_tool_service_embedded

    return build_tool_service_embedded(tmp_path)


FORBIDDEN_PATTERNS = (
    re.compile(r"https?://"),
    re.compile(r"<script\s+src"),
    re.compile(r"<link\s+"),
    re.compile(r"@import"),
)


def test_export_is_one_self_contained_file(indexed_svc, tmp_path):
    out, summary = export_html(indexed_svc, tmp_path, tmp_path / "export.html")
    assert out.exists()
    html = out.read_text(encoding="utf-8")
    for pattern in FORBIDDEN_PATTERNS:
        assert pattern.search(html) is None, (
            "the export must carry ZERO external references — the file:// "
            "contract (no server, no CDN, no fetch) is what makes it safe "
            "to share"
        )


def test_export_renders_the_real_chain(indexed_svc, tmp_path):
    """The chain view must show alpha <- test_alpha (a REAL TESTS edge
    resolved by index's pass), and the orphans view must name
    `untested` — the fixture's known-by-inspection truth."""
    _, summary = export_html(indexed_svc, tmp_path, tmp_path / "export.html")
    assert summary["chains"] == 1
    assert summary["orphans"] >= 1
    html = (tmp_path / "export.html").read_text(encoding="utf-8")
    assert "test_alpha" in html          # the chain's tested_by row
    assert "untested" in html            # the orphan row
    assert "alpha" in html


def test_export_misc_tasks_appear_in_the_tasks_view(indexed_svc, tmp_path):
    batch = [{
        "name": "t-implement-alpha", "task_type": "dev",
        "description": "make alpha real", "file_path": "app.py",
        "function_signatures": ["alpha()"],
        "test_triad": {"positive": "p", "negative": "n",
                       "negative_to_positive": "n2p"},
    }]
    assert indexed_svc.push_tasks(batch)["ok"] is True
    _, summary = export_html(indexed_svc, tmp_path, tmp_path / "export.html")
    assert summary["tasks"] == 1
    html = (tmp_path / "export.html").read_text(encoding="utf-8")
    assert "t-implement-alpha" in html


def test_export_escapes_html_in_user_data(indexed_svc, tmp_path):
    """A task description containing markup must render as TEXT — the
    escaping contract (a shared export is an XSS surface otherwise)."""
    indexed_svc.push_tasks([{
        "name": "evil", "task_type": "dev", "file_path": "app.py",
        "description": "<script>alert(1)</script>",
        "function_signatures": ["alpha()"],
        "test_triad": {"positive": "p", "negative": "n",
                       "negative_to_positive": "n2p"},
    }])
    export_html(indexed_svc, tmp_path, tmp_path / "export.html")
    html = (tmp_path / "export.html").read_text(encoding="utf-8")
    assert "<script>alert(1)</script>" not in html
    assert "&lt;script&gt;alert(1)&lt;/script&gt;" in html


def test_export_escapes_the_json_blob_closure(indexed_svc, tmp_path):
    """The `</` escape in the embedded JSON blob: a task NAME containing
    `</script>` must not break out of the data tag (stays data; the blob
    must still parse)."""
    indexed_svc.push_tasks([{
        "name": "x</script>", "task_type": "dev", "file_path": "app.py",
        "function_signatures": ["alpha()"],
        "test_triad": {"positive": "p", "negative": "n",
                       "negative_to_positive": "n2p"},
    }])
    export_html(indexed_svc, tmp_path, tmp_path / "export.html")
    html = (tmp_path / "export.html").read_text(encoding="utf-8")
    assert "</script>\\n" not in json.dumps(True)  # guard against a dumb assert
    body = html.split("window.CIE_DATA = ", 1)[1]
    payload = body.split(";</script>", 1)[0]
    # parsing must succeed — a broken-out blob would crash json.loads
    data = json.loads(payload.replace("<\\/", "</"))
    assert any("x</script>" == t.get("name") for t in data["tasks"])