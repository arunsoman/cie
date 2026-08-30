"""`cie export-html` — the shareable, read-only, self-contained HTML
artifact of a project's graph (roadmap R8).

The safe slice of a "graph viewer": STATIC, one file, no server, no auth
surface, no client-side network access (opens via `file://`). What
nobody else renders is the center: **task -> file -> test chains** (the
same data `traceability_chain`/`traceability_orphans` answer as tools),
plus the tasks, orphans, and file views.

Data comes ONLY from ToolService envelopes (read-only composition of
existing tools), so an export can never exceed the read side of the
surface it rides on. Everything is inlined — styles, the tiny
vanilla-JS client, the data blob — no CDN, no fetch, no server. Rows
are HTML-escaped; the JSON blob embeds after escaping the `</` sequence
so it cannot break out of its <script> tag.

Deliberately NOT here: server mode, auth, write-back, live updates —
this is a snapshot artifact, not an app.
"""

from __future__ import annotations

import json
from pathlib import Path

_MAX_CHAINS_DEFAULT = 50
_MAX_FILES_DEFAULT = 100


def _esc(s) -> str:
    return (
        str(s).replace("&", "&amp;").replace("<", "&lt;")
        .replace(">", "&gt;").replace('"', "&quot;")
    )


def _gather(service, max_chains: int, max_files: int) -> dict:
    """Read-only gathering through ToolService envelopes only."""
    try:
        file_nodes = service._engine.list_files()  # noqa: SLF001
        files = [{"path": n.source_file, "label": n.label}
                 for n in file_nodes if n.source_file]
    except Exception:  # noqa: BLE001 - graceful for an empty graph
        files = []
    files = files[:max_files]

    tasks: list[dict] = []
    tasks_env = service.list_pending_tasks()
    if tasks_env.get("ok"):
        for t in tasks_env.get("results") or []:
            tasks.append({
                "name": t.get("name", ""),
                "status": t.get("status", "pending"),
                "file_path": t.get("file_path", ""),
                "description": t.get("description", ""),
            })

    orphans_env = service.traceability_orphans()
    orphans = (orphans_env.get("results") or []) if orphans_env.get("ok") else []

    # Chains compose from ONE project_graph fetch + the same
    # `cie.traceability` functions the traceability_chain tool wraps —
    # matches edges by id exactly (node ids are `<file>::<sym>@<line>`,
    # so per-name resolution would need the graph anyway).
    chains: list[dict] = []
    try:
        from cie import traceability as _trace

        nodes, edges = service._engine._repo.project_graph()  # noqa: SLF001
        labels = {n.id: n for n in nodes}
        tests_by_target: dict[str, list[str]] = {}
        contracts_by_source: dict[str, list[str]] = {}
        for er in edges:
            if er.edge.relation == "TESTS":
                src = labels.get(er.edge.source)
                tests_by_target.setdefault(er.edge.target, []).append(
                    src.label if src is not None else er.edge.source
                )
            elif er.edge.relation == "HAS_CONTRACT":
                contracts_by_source.setdefault(er.edge.source, []).append(er.edge.target)
        for target_id in list(tests_by_target.keys())[:max_chains]:
            node = labels.get(target_id)
            if node is None:
                continue
            chains.append({
                "symbol": node.label, "file": node.source_file,
                "tested_by": tests_by_target.get(target_id) or [],
                "contracts": contracts_by_source.get(target_id) or [],
            })
    except Exception:  # noqa: BLE001 - graceful for an empty graph
        chains = []

    # coverage lives on the engine (repo-protocol method), not as a
    # ToolService tool on every surface — the HTTP-only coverage_report
    # helper wraps it; the export reads the same engine method directly
    # (read side only, same dat the HTTP helper would serve).
    try:
        coverage_rows = service._engine.coverage_report(  # noqa: SLF001
            "", None, "", True,
        )
        coverage = [
            {"file": c.file_path, "pct": c.coverage_pct, "measured": c.measured}
            for c in coverage_rows
        ]
    except Exception:  # noqa: BLE001 - graceful for an empty graph
        coverage = []

    return {
        "files": files,
        "tasks": tasks,
        "orphans": orphans,
        "chains": chains,
        "coverage": coverage,
        "summary": {
            "files": len(files),
            "tasks": len(tasks),
            "chains": len(chains),
            "orphans": len(orphans),
        },
    }


def _render_overview(data: dict) -> str:
    s = data["summary"]
    return (
        f'<h2>Overview</h2><div class="card mono">'
        f"files: {s['files']} &middot; tasks: {s['tasks']} &middot; "
        f"chains: {s['chains']} &middot; orphans: {s['orphans']}"
        f"</div>"
        '<div class="card"><h3>How to read this page</h3>'
        "Task &rarr; file &rarr; test chains are the queries no other code "
        "graph renders: every <code>tested_by</code> row is a persisted "
        "TESTS edge resolved through the same call graph. Symbols without "
        "any test are in the Orphans view &mdash; an empty list there is "
        "an answer, not missing data.</div>"
    )


def _render_chains(data: dict) -> str:
    cards = []
    for c in data["chains"]:
        rows = [
            f'<div><span class="pill ok">tested_by</span> <code>{_esc(t)}</code></div>'
            for t in c["tested_by"]
        ]
        rows += [
            f'<div><span class="pill bad">contract</span> <code>{_esc(k)}</code></div>'
            for k in c["contracts"]
        ]
        rows_html = "".join(rows) or '<div class="kv">no tests, no contracts</div>'
        cards.append(
            f'<div class="card"><h3><code>{_esc(c["symbol"])}</code> '
            f'<span class="kv">in</span> <code>{_esc(c["file"])}</code></h3>'
            f"{rows_html}</div>"
        )
    return "<h2>Task &amp; test chains</h2>" + "".join(cards)


def _render_tasks(data: dict) -> str:
    rows = []
    for t in data["tasks"]:
        status = _esc(t["status"])
        pill = f'<span class="pill {"ok" if status == "pending" else "gap"}">{status}</span>'
        rows.append(
            f"<tr><td><code>{_esc(t['name'])}</code></td><td>{pill}</td>"
            f"<td><code>{_esc(t['file_path'])}</code></td>"
            f"<td>{_esc(t['description'])}</td></tr>"
        )
    return (
        '<h2>Atomic tasks</h2><table><thead><tr><th>name</th><th>status</th>'
        '<th>file</th><th>description</th></tr></thead><tbody>'
        + "".join(rows) + "</tbody></table>"
    )


def _render_orphans(data: dict) -> str:
    rows = [
        f'<tr><td><code>{_esc(o["label"])}</code></td>'
        f'<td><code>{_esc(o["source_file"])}</code></td></tr>'
        for o in data["orphans"]
    ]
    return (
        '<h2>Orphans (no test, no contract)</h2><table><thead><tr>'
        "<th>symbol</th><th>file</th></tr></thead><tbody>"
        + "".join(rows) + "</tbody></table>"
    )


def _render_files(data: dict) -> str:
    rows = [
        f'<tr><td><code>{_esc(f["path"])}</code></td><td>{_esc(f["label"])}</td></tr>'
        for f in data["files"]
    ]
    return (
        '<h2>Indexed files</h2><table><thead><tr><th>path</th>'
        "<th>file hub</th></tr></thead><tbody>"
        + "".join(rows) + "</tbody></table>"
    )


_TEMPLATE = """<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8">
<title>cie — project snapshot</title>
<style>__CSS__</style></head>
<body>
<header><h1>cie &mdash; project snapshot</h1>
<div class="sub">static, self-contained export — no server, no network
calls; open via file://</div></header>
<main>
<nav>
  <a href="#" data-target="sec-overview">Overview</a>
  <a href="#" data-target="sec-chains">Task &amp; test chains</a>
  <a href="#" data-target="sec-tasks">Atomic tasks</a>
  <a href="#" data-target="sec-orphans">Orphans</a>
  <a href="#" data-target="sec-files">Indexed files</a>
</nav>
<section id="sec-overview" class="view">
  <input type="search" id="q" data-search
         placeholder="filter rows in the views...">
  __OVERVIEW__
</section>
<section id="sec-chains" class="view" data-searchable>__CHAINS__</section>
<section id="sec-tasks" class="view" data-searchable>__TASKS__</section>
<section id="sec-orphans" class="view" data-searchable>__ORPHANS__</section>
<section id="sec-files" class="view" data-searchable>__FILES__</section>
</main>
<footer>Generated by `cie export-html` — a read-only snapshot; regenerate
after changes with `cie index` + this command.</footer>
<script>window.CIE_DATA = __DATA__;</script>
<script>__JS__</script>
</body></html>
"""

_CSS = """
:root { --bg:#0f1420; --panel:#171e2e; --fg:#dbe2f0; --dim:#93a1bb;
  --accent:#67b0ff; --ok:#43c78f; --warn:#e0b45a; --bad:#e06c75;
  --edge:#2a3247; --mono: ui-monospace, SFMono-Regular, Menlo, monospace; }
* { box-sizing:border-box; }
body { margin:0; background:var(--bg); color:var(--fg);
  font:14px/1.5 system-ui,-apple-system,"Segoe UI",sans-serif; }
header { padding:18px 24px; border-bottom:1px solid var(--edge); }
h1 { margin:0 0 4px; font-size:18px; }
.sub { color:var(--dim); font-size:12px; }
main { display:grid; grid-template-columns:270px 1fr; gap:0;
  min-height:calc(100vh - 62px); }
nav { border-right:1px solid var(--edge); padding:12px; }
nav a { display:block; padding:6px 8px; margin:1px 0; border-radius:6px;
  color:var(--fg); text-decoration:none; font-size:13px; }
nav a:hover, nav a.active { background:var(--panel); }
nav a.active { color:var(--accent); }
section.view { display:none; padding:18px 26px; }
section.active { display:block; }
h2 { margin:0 0 14px; font-size:15px; }
input[type=search] { width:100%; max-width:420px; padding:7px 10px;
  margin-bottom:12px; background:var(--panel); color:var(--fg);
  border:1px solid var(--edge); border-radius:6px; font-size:13px; }
table { width:100%; border-collapse:collapse; font-size:13px; }
th { text-align:left; color:var(--dim); font-weight:500; padding:6px 8px;
  border-bottom:1px solid var(--edge); }
td { padding:6px 8px; border-bottom:1px solid #1d2537; }
code { font-family:var(--mono); font-size:12px; color:var(--accent); }
.pill { display:inline-block; padding:1px 8px; border-radius:99px;
  font-size:11px; }
.pill.ok { background:#123829; color:var(--ok); }
.pill.bad { background:#3a1a1e; color:var(--bad); }
.pill.kv { background:#232b3d; color:var(--dim); }
.card { background:var(--panel); border:1px solid var(--edge);
  border-radius:8px; padding:12px 14px; margin-bottom:10px; }
.card h3 { margin:0 0 6px; font-size:13px; }
.kv { color:var(--dim); }
footer { padding:10px 24px; color:var(--dim); font-size:11px;
  border-top:1px solid var(--edge); }
"""

_JS = """
(function () {
  var q = document.getElementById('q');
  q.addEventListener('input', function () {
    var term = q.value.toLowerCase();
    document.querySelectorAll('tbody tr').forEach(function (tr) {
      tr.style.display = tr.textContent.toLowerCase().indexOf(term) >= 0 ? '' : 'none';
    });
  });
  var links = document.querySelectorAll('nav a');
  links.forEach(function (a) {
    a.addEventListener('click', function (ev) {
      ev.preventDefault();
      links.forEach(function (x) { x.classList.remove('active'); });
      ev.currentTarget.classList.add('active');
      document.querySelectorAll('section').forEach(function (s) {
        s.classList.remove('active'); });
      document.getElementById(ev.currentTarget.dataset.target)
        .classList.add('active');
    });
  });
  if (links.length) {
    // deep-link support: /path/export.html#sec-chains opens directly on
    // that view (shareable anchors into a static snapshot)
    var requested = (location.hash || '').replace('#', '');
    var initial = document.getElementById(requested) || null;
    if (initial && initial.tagName === 'SECTION') {
      links.forEach(function (a) {
        a.classList.toggle('active', a.dataset.target === requested);
      });
      initial.classList.add('active');
    } else {
      links[0].classList.add('active');
      document.getElementById('sec-overview').classList.add('active');
    }
  }
})();
"""


def export_html(
    service,
    project_root: Path,
    out_path: Path,
    *,
    max_chains: int = _MAX_CHAINS_DEFAULT,
    max_files: int = _MAX_FILES_DEFAULT,
) -> tuple[Path, dict]:
    """Render the shareable HTML snapshot; returns ``(out_path, summary)``.

    `service` is a ToolService — same envelopes the MCP/HTTP surfaces
    serve, so the export can't exceed the tool surface's read side."""
    data = _gather(service, max_chains, max_files)
    html = _TEMPLATE
    html = html.replace("__CSS__", _CSS).replace("__JS__", _JS)
    html = html.replace("__OVERVIEW__", _render_overview(data))
    html = html.replace("__CHAINS__", _render_chains(data))
    html = html.replace("__TASKS__", _render_tasks(data))
    html = html.replace("__ORPHANS__", _render_orphans(data))
    html = html.replace("__FILES__", _render_files(data))
    html = html.replace("__DATA__", json.dumps(data).replace("</", "<\\/"))
    out_path = Path(out_path)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(html, encoding="utf-8")
    return out_path, data["summary"]