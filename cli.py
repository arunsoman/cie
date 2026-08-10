"""cie CLI — human tables by default, SPEC §0 JSON envelopes with --json.

Every command honors ``--json``: the output is the standard response envelope
from :mod:`cie.envelope`, shape-identical to the HTTP ``/tools/*``
surface, so an agent can drive cie entirely over JSON:

    cie search "what connects attention to the optimizer"
    cie --json node SwinTransformer
    cie --json search-symbol parse_csv --kind function
    cie --json run "pytest -q" --timeout 300
    cie tasks:push batch.json
    cie hierarchy:children <node-id> --depth 2

``--json`` is a GROUP-level option (``@cli.command()``'s parent
``@click.group()``) — it must come BEFORE the subcommand name
(``cie --json search-symbol ...``), not after. Every command's own
``--project``, if it has one, comes after the subcommand as usual.

Connection settings come from ``CIE_NEO4J_*`` environment variables (see
:mod:`cie.config`). ``CIE_PROJECT`` (or the adopted alias
``FORGE_PROJECT_ID``) selects the project namespace; ``CIE_RUN_ROOT``
sets the jail root for the ``run`` tool (default: the current directory).
"""

from __future__ import annotations

import dataclasses
import json as _json
import os
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Optional

import click
from rich.console import Console
from rich.table import Table

from cie.config import Neo4jConfig
from cie.envelope import ToolTimer, envelope, err_envelope
from cie.models import (
    CoverageSnapshot,
    Node,
    NodeRecord,
    PathResult,
    TraversalMode,
    TraversalResult,
)
from cie.neo4j_repository import Neo4jRepository
from cie.query import QueryEngine
from cie.tasks import SCHEMA_VERSION, AtomicTaskBatch, HierarchyNode

console = Console()


# ---------------------------------------------------------------------------
# Backend factories (monkeypatched in tests; Neo4j in production)
# ---------------------------------------------------------------------------


def _project_from_env() -> str:
    """Project namespace from CIE_PROJECT (FORGE_PROJECT_ID alias)."""
    return os.environ.get("CIE_PROJECT") or os.environ.get(
        "FORGE_PROJECT_ID", ""
    )


def _open_engine(project: Optional[str] = None, ensure_indices: bool = False) -> QueryEngine:
    """Connect a QueryEngine to Neo4j, scoped to a project.

    Defaults to ``CIE_PROJECT``/``FORGE_PROJECT_ID`` (see module
    docstring) when the caller doesn't pass one explicitly — every command
    honors the env var this way, not just the handful that used to thread
    it through by hand. Pass ``project=""`` explicitly for the legacy
    unscoped (all-projects) view.

    ``ensure_indices=False`` by default (2026-08-07): `Neo4jRepository.
    connect()`'s own default flipped from always-on to opt-in — every CLI
    command used to pay a `CREATE INDEX IF NOT EXISTS` round trip (and, per
    the confirmed 2026-08-04 incident `cie.timeouts` documents, a real risk
    of queuing behind a stuck schema lock) on every single invocation, even
    read-only ones like `cie health`. Indices are bootstrapped once by the
    app's own startup lifespan (`app/api.py`) or explicitly via `cie
    bootstrap`; `load`/`watch` below still pass `ensure_indices=True`
    themselves since they're the natural first-touch commands for a
    project that may never have gone through the app at all.
    """
    if project is None:
        project = _project_from_env()
    cfg = Neo4jConfig.from_env()
    repo = Neo4jRepository.connect(
        cfg.uri, cfg.user, cfg.password, cfg.database, project=project,
        ensure_indices=ensure_indices,
        connect_timeout_s=cfg.connect_timeout_s,
        max_retry_time_s=cfg.max_retry_time_s,
        query_timeout_s=cfg.query_timeout_s,
        schema_timeout_s=cfg.schema_timeout_s,
        write_timeout_s=cfg.write_timeout_s,
    )
    return QueryEngine(repo)


def _close_engine(engine: QueryEngine) -> None:
    repo = engine._repo  # noqa: SLF001 - the CLI owns the lifecycle here
    close = getattr(repo, "close", None)
    if callable(close):
        close()


def _open_task_repo():
    """Connect the task repository; returns ``(repo, driver)``."""
    from neo4j import GraphDatabase

    from cie.task_repository import Neo4jTaskRepository

    cfg = Neo4jConfig.from_env()
    driver = GraphDatabase.driver(cfg.uri, auth=(cfg.user, cfg.password), **cfg.driver_kwargs())
    return Neo4jTaskRepository.from_driver(driver), driver


def _open_hierarchy_repo(project: str = ""):
    """Connect the hierarchy repository; returns ``(repo, driver)``."""
    from neo4j import GraphDatabase

    from cie.hierarchy import Neo4jHierarchyRepository

    cfg = Neo4jConfig.from_env()
    driver = GraphDatabase.driver(cfg.uri, auth=(cfg.user, cfg.password), **cfg.driver_kwargs())
    return Neo4jHierarchyRepository.from_driver(driver, project=project), driver


def _open_tool_service():
    """Build the ToolService facade over the real backends.

    File views, runs, and git history are jailed under the current working
    directory; ``CIE_RUN_ROOT`` can widen the ``run`` tool's jail.
    """
    from cie.tools import ToolService

    engine = _open_engine()
    task_repo, _driver = _open_task_repo()
    root = Path.cwd()
    allowed_root = Path(os.environ.get("CIE_RUN_ROOT") or str(root))
    return ToolService(engine, task_repo, root=root, allowed_root=allowed_root)


def _close_driver(driver: Any) -> None:
    close = getattr(driver, "close", None)
    if callable(close):
        close()


# ---------------------------------------------------------------------------
# Envelope emit helpers — the single --json path for every command
# ---------------------------------------------------------------------------


def _echo_json(payload: dict) -> None:
    click.echo(_json.dumps(payload, indent=2, default=str))


def _emit(
    ctx: click.Context,
    tool_name: str,
    results_or_dict: Any,
    *,
    hint: Optional[str] = None,
    truncated: bool = False,
    total: Optional[int] = None,
    timer: Optional[ToolTimer] = None,
) -> bool:
    """Emit the SPEC §0 success envelope when ``--json`` is active.

    Returns True when the envelope was echoed (the caller must skip its
    human rendering) and False in human mode (the caller renders rich
    output instead).
    """
    if not ctx.obj.get("json"):
        return False
    _echo_json(
        envelope(
            tool_name,
            results_or_dict,
            truncated=truncated,
            total=total,
            hint=hint,
            elapsed_ms=timer.elapsed_ms if timer else 0,
        )
    )
    return True


def _emit_err(
    ctx: click.Context,
    tool_name: str,
    kind: str,
    message: str,
    *,
    hint: Optional[str] = None,
    timer: Optional[ToolTimer] = None,
) -> None:
    """Emit an error envelope (--json) or red message (human); exits 1."""
    if ctx.obj.get("json"):
        _echo_json(
            err_envelope(
                tool_name,
                kind,
                message,
                hint=hint,
                elapsed_ms=timer.elapsed_ms if timer else 0,
            )
        )
    else:
        console.print(f"[red]{kind}: {message}[/red]")
        if hint:
            console.print(f"[yellow]hint: {hint}[/yellow]")
    raise SystemExit(1)


def _finish_service(ctx: click.Context, payload: dict) -> list:
    """Shared handling for ToolService envelope dicts.

    --json mode echoes the envelope verbatim and exits (0 when ``ok``, 1
    otherwise). Human mode prints error envelopes and exits 1; on success
    it returns ``payload['results']`` for the caller to render.
    """
    if ctx.obj.get("json"):
        _echo_json(payload)
        raise SystemExit(0 if payload.get("ok") else 1)
    if not payload.get("ok"):
        error = payload.get("error") or {}
        console.print(
            f"[red]{error.get('kind', 'error')}: "
            f"{error.get('message', 'unknown failure')}[/red]"
        )
        if payload.get("hint"):
            console.print(f"[yellow]hint: {payload['hint']}[/yellow]")
        raise SystemExit(1)
    return payload.get("results") or []


# ---------------------------------------------------------------------------
# Serializers + human renderers
# ---------------------------------------------------------------------------


def _node_dict(n: Node) -> dict:
    from cie.serialize import node_to_dict

    return node_to_dict(n)


def _edge_dicts(edges: Any) -> list[dict]:
    return [
        {
            "source": e.edge.source,
            "target": e.edge.target,
            "relation": e.edge.relation,
            "confidence": e.edge.confidence.value,
            "source_label": e.source_label,
            "target_label": e.target_label,
        }
        for e in edges
    ]


def _render_node(n: Node) -> str:
    parts = [n.label]
    if n.source_file:
        parts.append(f" ({n.source_file}")
        if n.source_location:
            parts.append(f" {n.source_location}")
        parts.append(")")
    if n.community is not None:
        parts.append(f" [community {n.community}]")
    return "".join(parts)


def _render_traversal(result: TraversalResult) -> None:
    if not result.nodes:
        console.print("[yellow]No matching nodes found.[/yellow]")
        return
    header = (
        f"Traversal: {result.mode.value.upper()} depth={result.depth} | "
        f"Start: {list(result.start_labels)} | {len(result.nodes)} nodes found"
    )
    console.print(f"[bold]{header}[/bold]\n")
    table = Table(show_header=True, header_style="bold cyan")
    table.add_column("Node")
    table.add_column("Source")
    table.add_column("Community", justify="right")
    for n in result.nodes:
        table.add_row(
            n.label,
            n.source_file,
            "" if n.community is None else str(n.community),
        )
    console.print(table)
    if result.edges:
        console.print("\n[bold]Edges[/bold]")
        et = Table(show_header=True, header_style="bold cyan")
        et.add_column("From")
        et.add_column("Relation")
        et.add_column("To")
        et.add_column("Confidence")
        for e in result.edges:
            et.add_row(
                e.source_label,
                e.edge.relation,
                e.target_label,
                e.edge.confidence.value,
            )
        console.print(et)


def _render_node_record(nr: NodeRecord) -> None:
    n = nr.node
    console.print(f"[bold]Node: {n.label}[/bold]")
    console.print(f"  ID:        {n.id}")
    console.print(f"  Source:    {n.source_file} {n.source_location}")
    console.print(f"  Type:      {n.file_type}")
    console.print(f"  Community: {n.community if n.community is not None else '-'}")
    console.print(f"  Degree:    {nr.degree}")


def _render_neighbors(label: str, neighbors: Optional[list]) -> None:
    if neighbors is None:
        console.print(f"[yellow]No node matching '{label}' found.[/yellow]")
        return
    if not neighbors:
        console.print(f"[bold]Neighbors of {label}:[/bold] (none)")
        return
    console.print(f"[bold]Neighbors of {label}:[/bold]")
    table = Table(show_header=True, header_style="bold cyan")
    table.add_column("Direction")
    table.add_column("Neighbor")
    table.add_column("Relation")
    table.add_column("Confidence")
    for e in neighbors:
        table.add_row(
            "-->" if e.edge.source == label else "<--",
            e.target_label if e.edge.source == label else e.source_label,
            e.edge.relation,
            e.edge.confidence.value,
        )
    console.print(table)


def _render_path(result: Optional[PathResult], src: str, tgt: str) -> None:
    if result is None:
        console.print(
            f"[yellow]No path found between '{src}' and '{tgt}'.[/yellow]"
        )
        return
    console.print(f"[bold]Shortest path ({result.hops} hops):[/bold]")
    segments = []
    for i, n in enumerate(result.nodes):
        segments.append(n.label)
        if i < len(result.edges):
            er = result.edges[i]
            conf = f" [{er.edge.confidence.value}]" if er.edge.confidence else ""
            segments.append(f"--{er.edge.relation}{conf}-->")
    console.print(" ".join(segments))


def _format_signature(sig) -> str:
    params = ", ".join(sig.parameters) if sig.parameters else ""
    out = f"{sig.name}({params})"
    if sig.return_type:
        out = f"{out} -> {sig.return_type}"
    return out


def _render_signature(sig) -> None:
    console.print(f"[bold]Signature: {_format_signature(sig)}[/bold]")
    console.print(f"  Name:           {sig.name}")
    console.print(f"  Parameters:     {', '.join(sig.parameters) if sig.parameters else '(none)'}")
    console.print(f"  Return type:    {sig.return_type or '-'}")
    console.print(f"  Enclosing class: {sig.enclosing_class or '-'}")
    console.print(f"  Source:         {sig.node.source_file} {sig.node.source_location}")


# ---------------------------------------------------------------------------
# CLI group
# ---------------------------------------------------------------------------


@click.group()
@click.version_option(package_name="protobox-be-v2")
@click.option(
    "--json/--no-json", default=False,
    help="Output the SPEC §0 JSON envelope instead of human-formatted tables.",
)
@click.pass_context
def cli(ctx, json: bool) -> None:
    """cie: query a knowledge graph stored in Neo4j."""
    ctx.ensure_object(dict)
    ctx.obj["json"] = json


# ---------------------------------------------------------------------------
# Legacy query commands (engine-backed)
# ---------------------------------------------------------------------------


@cli.command()
@click.argument("question")
@click.option(
    "--mode",
    type=click.Choice([m.value for m in TraversalMode]),
    default="bfs",
    help="bfs=broad context, dfs=trace a specific path.",
)
@click.option(
    "--depth", type=click.IntRange(1, 6), default=3, help="Traversal depth (1-6)."
)
@click.pass_context
def search(ctx, question: str, mode: str, depth: int) -> None:
    """Search the graph by natural-language question or keywords."""
    engine = _open_engine()
    try:
        with ToolTimer() as timer:
            result = engine.search(question, TraversalMode(mode), depth)
        payload = {
            "mode": result.mode.value,
            "depth": result.depth,
            "start_labels": list(result.start_labels),
            "nodes": [_node_dict(n) for n in result.nodes],
            "edges": _edge_dicts(result.edges),
        }
        hint = None if result.nodes else (
            "no nodes matched; try fewer keywords or run `cie files` "
            "to see what is loaded"
        )
        if _emit(ctx, "search", payload, hint=hint, timer=timer):
            return
        _render_traversal(result)
    finally:
        _close_engine(engine)


@cli.command()
@click.argument("label")
@click.pass_context
def node(ctx, label: str) -> None:
    """Get full details for a specific node by label or ID."""
    engine = _open_engine()
    try:
        with ToolTimer() as timer:
            result = engine.get_node(label)
        if result is None:
            _emit_err(
                ctx, "node", "not_found", f"no node matching '{label}'",
                hint="try `cie search-symbol <name>` for a substring "
                     "search, or `cie files` to list loaded files",
                timer=timer,
            )
        payload = {**_node_dict(result.node), "degree": result.degree}
        if _emit(ctx, "node", payload, timer=timer):
            return
        _render_node_record(result)
    finally:
        _close_engine(engine)


@cli.command()
@click.argument("label")
@click.option(
    "--relation", "relation_filter", default="", help="Filter by relation type."
)
@click.pass_context
def neighbors(ctx, label: str, relation_filter: str) -> None:
    """Get all direct neighbors of a node with edge details."""
    engine = _open_engine()
    try:
        with ToolTimer() as timer:
            result = engine.get_neighbors(label, relation_filter)
        if result is None:
            _emit_err(
                ctx, "neighbors", "not_found", f"no node matching '{label}'",
                hint="check the label with `cie search-symbol <name>`",
                timer=timer,
            )
        hint = None if result else (
            f"node '{label}' has no neighbors"
            + (f" with relation containing '{relation_filter}'" if relation_filter else "")
        )
        if _emit(ctx, "neighbors", _edge_dicts(result), hint=hint, timer=timer):
            return
        _render_neighbors(label, result)
    finally:
        _close_engine(engine)


@cli.command()
@click.argument("community_id", type=int)
@click.pass_context
def community(ctx, community_id: int) -> None:
    """Get all nodes in a community by community ID."""
    engine = _open_engine()
    try:
        with ToolTimer() as timer:
            nodes = engine.get_community(community_id)
        hint = None if nodes else (
            f"community {community_id} not found or empty; list them with "
            "`cie communities`"
        )
        if _emit(ctx, "community", [_node_dict(n) for n in nodes],
                 hint=hint, timer=timer):
            return
        if not nodes:
            console.print(
                f"[yellow]Community {community_id} not found or empty.[/yellow]"
            )
            return
        console.print(f"[bold]Community {community_id} ({len(nodes)} nodes):[/bold]")
        for n in nodes:
            console.print(f"  {_render_node(n)}")
    finally:
        _close_engine(engine)


@cli.command(name="communities")
@click.pass_context
def list_communities(ctx) -> None:
    """List all communities ordered by size."""
    engine = _open_engine()
    try:
        with ToolTimer() as timer:
            rows = engine.list_communities()
        payload = [{"community": cid, "size": size} for cid, size in rows]
        hint = None if rows else "no communities found; load a graph first"
        if _emit(ctx, "communities", payload, hint=hint, timer=timer):
            return
        if not rows:
            console.print("[yellow]No communities found.[/yellow]")
            return
        table = Table(show_header=True, header_style="bold cyan")
        table.add_column("Community", justify="right")
        table.add_column("Size", justify="right")
        for cid, size in rows:
            table.add_row(str(cid), str(size))
        console.print(table)
    finally:
        _close_engine(engine)


@cli.command()
@click.option("--top-n", type=int, default=10, help="Number of top nodes to show.")
@click.pass_context
def god(ctx, top_n: int) -> None:
    """Return the most connected nodes - the core abstractions."""
    engine = _open_engine()
    try:
        with ToolTimer() as timer:
            nodes = engine.god_nodes(top_n)
        payload = [
            {"label": nr.node.label, "id": nr.node.id, "degree": nr.degree}
            for nr in nodes
        ]
        hint = None if nodes else "no god nodes found; load a graph first"
        if _emit(ctx, "god", payload, hint=hint, timer=timer):
            return
        if not nodes:
            console.print("[yellow]No god nodes found.[/yellow]")
            return
        console.print("[bold]God nodes (most connected):[/bold]")
        for i, nr in enumerate(nodes, 1):
            console.print(f"  {i}. {nr.node.label} - {nr.degree} edges")
    finally:
        _close_engine(engine)


@cli.command()
@click.pass_context
def stats(ctx) -> None:
    """Return summary statistics for the graph."""
    engine = _open_engine()
    try:
        with ToolTimer() as timer:
            s = engine.stats()
        payload = {
            "nodes": s.nodes,
            "edges": s.edges,
            "communities": s.communities,
            "confidence_counts": s.confidence_counts,
            "confidence_percent": s.confidence_percent,
        }
        if _emit(ctx, "stats", payload, timer=timer):
            return
        console.print(f"Nodes:       {s.nodes}")
        console.print(f"Edges:       {s.edges}")
        console.print(f"Communities: {s.communities}")
        percents = s.confidence_percent
        for conf in ("EXTRACTED", "INFERRED", "AMBIGUOUS"):
            console.print(f"{conf}: {percents.get(conf, 0)}%")
    finally:
        _close_engine(engine)


@cli.command()
@click.pass_context
def bootstrap(ctx) -> None:
    """Create every cie index/constraint if it doesn't exist yet.

    Explicit, deliberate index creation — the counterpart to
    `Neo4jRepository.connect()`'s `ensure_indices=False` default (see
    `_open_engine`'s docstring for why that flipped): every OTHER command
    now assumes indices already exist rather than paying a `CREATE INDEX
    IF NOT EXISTS` round trip (and the schema-lock contention risk
    `cie.timeouts` documents) on every invocation. The app's own startup
    lifespan (`app/api.py`) already calls this once per process for the
    live service; run this by hand for a standalone CLI-only workflow
    against a brand-new Neo4j instance the app has never booted against.
    Safe to re-run — every statement is `IF NOT EXISTS`.
    """
    engine = _open_engine()
    try:
        engine._repo.ensure_indices()  # noqa: SLF001 - the CLI owns this call
        console.print("[green]Indices ensured.[/green]")
    finally:
        _close_engine(engine)


@cli.command(name="semantic-search")
@click.argument("query")
@click.option(
    "--top-k", type=int, default=10, help="Max results (capped at 50)."
)
@click.pass_context
def semantic_search(ctx, query: str, top_k: int) -> None:
    """Rank nodes by embedding similarity to a natural-language query.

    Complements `search`'s lexical/substring matching by finding
    conceptually related code even without shared keywords. Requires nodes
    were loaded/reindexed with NVIDIA_API_KEY set so embeddings were
    computed at write time — empty results usually mean that didn't happen
    yet, not that nothing matches.
    """
    engine = _open_engine()
    try:
        with ToolTimer() as timer:
            matches = engine.semantic_search(query, top_k=top_k)
        payload = [{**_node_dict(m.node), "score": m.score} for m in matches]
        hint = None if matches else (
            "no embeddings matched; nodes must be loaded/reindexed with "
            "NVIDIA_API_KEY set before semantic-search has anything to "
            "rank - try `cie search` for lexical matching instead"
        )
        if _emit(ctx, "semantic_search", payload, hint=hint, timer=timer):
            return
        if not matches:
            console.print(f"[yellow]{hint}[/yellow]")
            return
        console.print(f"[bold]Semantic matches for '{query}' ({len(matches)}):[/bold]")
        table = Table(show_header=True, header_style="bold cyan")
        table.add_column("Score", justify="right")
        table.add_column("Label")
        table.add_column("Kind")
        table.add_column("Source")
        for m in matches:
            table.add_row(
                f"{m.score:.3f}", m.node.label, m.node.kind, m.node.source_file,
            )
        console.print(table)
    finally:
        _close_engine(engine)


@cli.command()
@click.argument("source")
@click.argument("target")
@click.option(
    "--max-hops", type=int, default=8, help="Maximum hops to consider."
)
@click.pass_context
def path(ctx, source: str, target: str, max_hops: int) -> None:
    """Find the shortest path between two concepts (tool: path_between)."""
    service = _open_tool_service()
    payload = service.path_between(source, target, max_hops=max_hops)
    results = _finish_service(ctx, payload)
    if not results:
        console.print(f"[yellow]{payload.get('hint')}[/yellow]")
        return
    chain = results[0]["chain"]
    console.print(f"[bold]Shortest path ({results[0]['hops']} hops):[/bold]")
    segments = []
    for hop in chain:
        segments.append(hop["symbol"])
        if hop.get("relation"):
            segments.append(f"--{hop['relation']} [{hop['confidence']}]-->")
    console.print(" ".join(segments))


@cli.command()
@click.argument(
    "paths", nargs=-1, required=True,
    type=click.Path(exists=True, file_okay=False, path_type=Path),
)
@click.option(
    "--project", "project", default="",
    help="Project namespace: only this project's nodes are replaced, and "
         "every loaded node is stamped with it.",
)
@click.pass_context
def load(ctx, paths, project: str) -> None:
    """Parse one or more directories of source files with tree-sitter and
    load them into Neo4j, replacing this project's existing nodes.

    Runs the two-pass loader: pass 1 extracts files/classes/functions/methods,
    pass 2 resolves call sites into confidence-tagged `calls` edges.

    Always fully replaces this project's existing nodes (a repeated load
    of the same tree must not accumulate duplicates — see
    `Repository.load_extraction`'s contract). To load disjoint directories
    into ONE project without one wiping the other's data (e.g. a backend
    and a frontend tree sharing a namespace), pass every directory in a
    SINGLE call: `cie load be-v2/src frontend/src --project protobox-self`
    — there used to be a `--clear/--no-clear` flag here that implied a
    second call could merge instead of replace; it never actually worked
    (the flag was accepted but silently ignored) and has been removed in
    favor of this — the only way multiple directories were ever going to
    coexist in one project without a merge write path.
    """
    from cie.callgraph import (
        resolve_call_edges,
        resolve_inheritance_edges,
        synthesize_external_class_nodes,
    )
    from cie.extract import extract_many
    from cie.testlink import resolve_test_edges

    per_file = [ext for path in paths for ext in extract_many(path)]
    engine = _open_engine(ensure_indices=True)
    try:
        with ToolTimer() as timer:
            nodes = [n for ext in per_file for n in ext.nodes]
            if not nodes:
                _emit_err(
                    ctx, "load", "not_found",
                    f"no supported source files found under {', '.join(str(p) for p in paths)}",
                    hint="the loader parses .py/.js/.ts/.tsx files; check the path(s)",
                )
            edges = [e for ext in per_file for e in ext.edges]
            call_edges = resolve_call_edges(per_file)
            inheritance_edges = resolve_inheritance_edges(per_file)
            known_ids = {n["id"] for n in nodes}
            # DM-08: an unresolved base class (external library, or a
            # reference this pass cannot place) still gets an edge — see
            # resolve_inheritance_edges — so it needs a real node to
            # attach to, or the edge-write MATCH below silently drops it.
            external_nodes = synthesize_external_class_nodes(inheritance_edges, known_ids)
            nodes = nodes + external_nodes
            test_edges = resolve_test_edges(per_file, call_edges)
            count = engine._repo.load_extraction(  # noqa: SLF001
                nodes, edges + call_edges + inheritance_edges + test_edges, project=project,
            )
        path_list = [str(p) for p in paths]
        total_edges = (
            len(edges) + len(call_edges) + len(inheritance_edges) + len(test_edges)
        )
        payload = {
            "nodes_loaded": count,
            "edges_loaded": total_edges,
            "calls_edges": len(call_edges),
            "inheritance_edges": len(inheritance_edges),
            "test_edges": len(test_edges),
            "paths": path_list,
            "project": project,
        }
        if _emit(ctx, "load", payload, timer=timer):
            return
        console.print(
            f"[green]Loaded {count} nodes and {total_edges} edges "
            f"({len(call_edges)} calls, {len(inheritance_edges)} inheritance, "
            f"{len(test_edges)} TESTS) from {', '.join(path_list)}.[/green]"
        )
    finally:
        _close_engine(engine)


@cli.command()
@click.argument("name")
@click.pass_context
def signature(ctx, name: str) -> None:
    """Return the signature of a function or method by name."""
    from cie.serialize import signature_to_dict

    engine = _open_engine()
    try:
        with ToolTimer() as timer:
            result = engine.get_signature(name)
        if result is None:
            _emit_err(
                ctx, "signature", "not_found",
                f"no function or method matching '{name}'",
                hint="try `cie search-symbol <name> --kind function`",
                timer=timer,
            )
        if _emit(ctx, "signature", signature_to_dict(result), timer=timer):
            return
        _render_signature(result)
    finally:
        _close_engine(engine)


@cli.command()
@click.argument("class_name")
@click.pass_context
def methods(ctx, class_name: str) -> None:
    """Return all methods declared in a given class."""
    from cie.serialize import signature_to_dict

    engine = _open_engine()
    try:
        with ToolTimer() as timer:
            results = engine.get_methods_of_class(class_name)
        hint = None if results else (
            f"no class matching '{class_name}' found, or it has no methods; "
            "check the name with `cie search-symbol <name> --kind class`"
        )
        if _emit(ctx, "methods", [signature_to_dict(s) for s in results],
                 hint=hint, timer=timer):
            return
        if not results:
            console.print(f"[yellow]No class matching '{class_name}' found, or it has no methods.[/yellow]")
            return
        console.print(f"[bold]Methods of {class_name} ({len(results)}):[/bold]")
        for i, sig in enumerate(results, 1):
            console.print(f"  {i}. {_format_signature(sig)}")
    finally:
        _close_engine(engine)


@cli.command()
@click.pass_context
def files(ctx) -> None:
    """List all source files loaded into the graph."""
    engine = _open_engine()
    try:
        with ToolTimer() as timer:
            nodes = engine.list_files()
        hint = None if nodes else "no files loaded; run `cie load <dir>` first"
        if _emit(ctx, "files", [_node_dict(n) for n in nodes],
                 hint=hint, timer=timer):
            return
        if not nodes:
            console.print("[yellow]No files loaded. Run `cie load <dir>` first.[/yellow]")
            return
        table = Table(show_header=True, header_style="bold cyan")
        table.add_column("File")
        table.add_column("Source path")
        for n in nodes:
            table.add_row(n.label, n.source_file)
        console.print(table)
    finally:
        _close_engine(engine)


@cli.command()
@click.pass_context
def discover(ctx) -> None:
    """Return all querying features this system provides (LLM-friendly)."""
    engine = _open_engine()
    try:
        with ToolTimer() as timer:
            features = engine.discover_features()
        if _emit(ctx, "discover", list(features), timer=timer):
            return
        console.print("[bold]cie querying features[/bold]\n")
        for i, f in enumerate(features, 1):
            console.print(f"{i}. {f}")
    finally:
        _close_engine(engine)


# ---------------------------------------------------------------------------
# Forge tool-layer commands (ToolService-backed; envelopes match /tools/*)
# ---------------------------------------------------------------------------


def _render_edge_peers(ctx, payload: dict, peer: str, symbol: str) -> None:
    """Human rendering for callers/callees EdgeRecord-shaped results."""
    results = _finish_service(ctx, payload)
    if not results:
        console.print(f"[yellow]{payload.get('hint')}[/yellow]")
        return
    plural = f"{peer}s"
    console.print(f"[bold]{plural.capitalize()} of {symbol} ({len(results)}):[/bold]")
    table = Table(show_header=True, header_style="bold cyan")
    table.add_column(peer.capitalize())
    table.add_column("File")
    table.add_column("Signature")
    table.add_column("Relation")
    table.add_column("Confidence")
    for r in results:
        table.add_row(
            str(r.get(peer, "")),
            str(r.get(f"{peer}_file", "")),
            str(r.get(f"{peer}_signature", "")),
            str(r.get("relation", "")),
            str(r.get("confidence", "")),
        )
    console.print(table)


@cli.command()
@click.argument("symbol")
@click.option("--limit", type=int, default=30, help="Max results (capped at 50).")
@click.pass_context
def callers(ctx, symbol: str, limit: int) -> None:
    """Return all callers of a symbol (blast radius for repair localization)."""
    service = _open_tool_service()
    payload = service.callers(symbol, limit=limit)
    _render_edge_peers(ctx, payload, "caller", symbol)


@cli.command()
@click.argument("symbol")
@click.option("--limit", type=int, default=30, help="Max results (capped at 50).")
@click.pass_context
def callees(ctx, symbol: str, limit: int) -> None:
    """Return everything a symbol calls (reverse localization)."""
    service = _open_tool_service()
    payload = service.callees(symbol, limit=limit)
    _render_edge_peers(ctx, payload, "callee", symbol)


@cli.command(name="search-symbol")
@click.argument("name")
@click.option("--kind", default="", help="Filter: class|function|method|file.")
@click.option("--file-glob", default="", help="fnmatch glob on source_file.")
@click.option("--limit", type=int, default=20, help="Max results (capped at 50).")
@click.pass_context
def search_symbol(ctx, name: str, kind: str, file_glob: str, limit: int) -> None:
    """Locate symbol definitions by name (the #1 localization question)."""
    service = _open_tool_service()
    payload = service.search_symbol(name, kind=kind, file_glob=file_glob, limit=limit)
    results = _finish_service(ctx, payload)
    if not results:
        console.print(f"[yellow]{payload.get('hint')}[/yellow]")
        return
    table = Table(show_header=True, header_style="bold cyan")
    table.add_column("Name")
    table.add_column("Kind")
    table.add_column("File")
    table.add_column("Lines")
    table.add_column("Confidence")
    table.add_column("Signature")
    for r in results:
        lines = r.get("line_range") or [0, 0]
        table.add_row(
            str(r.get("name", "")),
            str(r.get("kind", "")),
            str(r.get("source_file", "")),
            f"{lines[0]}-{lines[1]}",
            str(r.get("confidence", "")),
            str(r.get("signature", "")),
        )
    console.print(table)


@cli.command(name="view-file")
@click.argument("path")
@click.option("--start", type=int, default=1, help="First line (1-based).")
@click.option("--end", type=int, default=100, help="Last line (inclusive).")
@click.pass_context
def view_file(ctx, path: str, start: int, end: int) -> None:
    """Windowed, line-numbered file view joined with the graph symbol index."""
    service = _open_tool_service()
    payload = service.view_file(path, start=start, end=end)
    results = _finish_service(ctx, payload)
    view = results[0]
    console.print(view["content"])
    if view.get("symbol_index"):
        console.print("\n[bold]Symbols in window:[/bold]")
        for sym in view["symbol_index"]:
            lines = sym.get("lines") or [0, 0]
            console.print(
                f"  {sym.get('kind', '')} {sym.get('name', '')} "
                f"L{lines[0]}-{lines[1]}"
            )
    if view.get("hint"):
        console.print(f"[dim]{view['hint']}[/dim]")


@cli.command()
@click.argument("file_path")
@click.pass_context
def skeleton(ctx, file_path: str) -> None:
    """Signatures + line ranges for every symbol in a file (no bodies)."""
    service = _open_tool_service()
    payload = service.file_skeleton(file_path)
    results = _finish_service(ctx, payload)
    symbols = results[0].get("symbols", []) if results else []
    if not symbols:
        console.print(f"[yellow]{payload.get('hint')}[/yellow]")
        return
    console.print(f"[bold]Skeleton of {file_path} ({len(symbols)} symbols):[/bold]")
    for sym in symbols:
        lines = sym.get("lines") or [0, 0]
        sig = f"  {sym['signature']}" if sym.get("signature") else ""
        console.print(
            f"  L{lines[0]}-{lines[1]} {sym.get('kind', '')}: "
            f"{sym.get('name', '')}{sig}"
        )


@cli.command(name="failing-context")
@click.argument("test_identifier")
@click.option("--depth", type=int, default=3, help="BFS depth from the test.")
@click.option("--limit", type=int, default=30, help="Max results (capped at 50).")
@click.pass_context
def failing_context(ctx, test_identifier: str, depth: int, limit: int) -> None:
    """Symbols reachable from a failing test, ranked by graph distance."""
    service = _open_tool_service()
    payload = service.failing_context(test_identifier, depth=depth, limit=limit)
    results = _finish_service(ctx, payload)
    if not results:
        console.print(f"[yellow]{payload.get('hint')}[/yellow]")
        return
    console.print(f"[bold]Context for {test_identifier} ({len(results)} symbols):[/bold]")
    table = Table(show_header=True, header_style="bold cyan")
    table.add_column("Distance", justify="right")
    table.add_column("Symbol")
    table.add_column("File")
    table.add_column("Confidence")
    for r in results:
        table.add_row(
            str(r.get("distance", "")),
            str(r.get("symbol", "")),
            str(r.get("file", "")),
            str(r.get("confidence", "")),
        )
    console.print(table)
    if payload.get("hint"):
        console.print(f"[dim]{payload['hint']}[/dim]")


@cli.command(name="affected-by")
@click.argument("file_path")
@click.option(
    "--direction", type=click.Choice(["incoming", "outgoing"]), default="incoming",
    help="incoming (default): what depends on this file, i.e. what breaks "
         "if it changes. outgoing: what this file depends on.",
)
@click.option("--max-depth", type=int, default=3, help="BFS depth (capped at 6).")
@click.option("--max-results", type=int, default=30, help="Max results (capped at 50).")
@click.pass_context
def affected_by(
    ctx, file_path: str, direction: str, max_depth: int, max_results: int,
) -> None:
    """Blast radius: what depends on this file, or what it depends on.

    Was previously HTTP-only (`POST /tools/affected_by`), unreachable
    without the be-v2 server running — this command was missing even
    though every other query tool has one; added so agents driving `cie`
    purely over the CLI (no server needed) can actually use it, matching
    what every persona's cie-knowledge-graph design doc already assumed
    was possible."""
    service = _open_tool_service()
    payload = service.affected_by(
        file_path, max_depth=max_depth, direction=direction, max_results=max_results,
    )
    results = _finish_service(ctx, payload)
    if not results:
        console.print(f"[yellow]{payload.get('hint')}[/yellow]")
        return
    console.print(
        f"[bold]{'Depends on' if direction == 'outgoing' else 'Affected by'} "
        f"{file_path} ({len(results)} hits):[/bold]"
    )
    table = Table(show_header=True, header_style="bold cyan")
    table.add_column("Distance", justify="right")
    table.add_column("Symbol")
    table.add_column("File")
    table.add_column("Confidence")
    for r in results:
        table.add_row(
            str(r.get("distance", "")),
            str(r.get("symbol", "")),
            str(r.get("file", "")),
            str(r.get("confidence", "")),
        )
    console.print(table)
    if payload.get("hint"):
        console.print(f"[dim]{payload['hint']}[/dim]")


@cli.command(name="resolve-route")
@click.argument("path")
@click.pass_context
def resolve_route(ctx, path: str) -> None:
    """Frontend API call path -> backend route(s) + which service serves it.

    The one HTTP-API-boundary question AST extraction alone can't answer
    (see cie/api_routes.py's module docstring) — a small, self-contained
    regex-based resolver over be-v2/src + backend/src route decorators and
    frontend/src fetch()/request()/streamFetch() call sites, cross-checked
    against frontend/vite.config.js's dev-proxy rules. `path` may be a
    literal path or carry `${...}`/`{...}` placeholders, e.g.
    `cie resolve-route '/api/tasks/${taskId}/kill'`.
    """
    service = _open_tool_service()
    payload = service.resolve_api_route(path)
    results = _finish_service(ctx, payload)
    if not results:
        console.print(f"[yellow]{payload.get('hint')}[/yellow]")
        return
    table = Table(show_header=True, header_style="bold cyan")
    table.add_column("Method")
    table.add_column("Path")
    table.add_column("Service")
    table.add_column("File:Line")
    table.add_column("Handler")
    for r in results:
        service_label = r.get("service", "")
        if r.get("is_forwarding_shim"):
            service_label += " (shim)"
        table.add_row(
            str(r.get("method", "")),
            str(r.get("path", "")),
            service_label,
            f"{r.get('file', '')}:{r.get('line', '')}",
            str(r.get("handler", "")),
        )
        forwards = r.get("forwards_to")
        if forwards:
            table.add_row(
                "", forwards.get("path", ""), forwards.get("service", ""),
                f"{forwards.get('file', '')}:{forwards.get('line', '')}",
                f"-> real impl: {forwards.get('handler', '')}",
            )
    console.print(table)
    if payload.get("hint"):
        console.print(f"[dim]{payload['hint']}[/dim]")


@cli.command(name="api-call-sites")
@click.argument("route")
@click.pass_context
def api_call_sites(ctx, route: str) -> None:
    """Backend route (path template or handler name) -> frontend call sites.

    Reverse of `resolve-route`, e.g.
    `cie api-call-sites '/api/tasks/{task_id}/kill'` or
    `cie api-call-sites api_kill_task`.
    """
    service = _open_tool_service()
    payload = service.api_call_sites(route)
    results = _finish_service(ctx, payload)
    if not results:
        console.print(f"[yellow]{payload.get('hint')}[/yellow]")
        return
    table = Table(show_header=True, header_style="bold cyan")
    table.add_column("Path expression")
    table.add_column("File:Line")
    table.add_column("Enclosing")
    for r in results:
        table.add_row(
            str(r.get("path", "")),
            f"{r.get('file', '')}:{r.get('line', '')}",
            str(r.get("enclosing", "")),
        )
    console.print(table)


@cli.command()
@click.argument("path")
@click.option("--limit", type=int, default=20, help="Max commits returned.")
@click.pass_context
def blame(ctx, path: str, limit: int) -> None:
    """Git history for a path, joined with task-graph artifacts."""
    service = _open_tool_service()
    payload = service.blame_history(path, limit=limit)
    results = _finish_service(ctx, payload)
    if not results:
        console.print(f"[yellow]{payload.get('hint')}[/yellow]")
        return
    table = Table(show_header=True, header_style="bold cyan")
    table.add_column("Commit")
    table.add_column("Timestamp")
    table.add_column("Message")
    table.add_column("Task")
    for r in results:
        task = r.get("task") or {}
        table.add_row(
            str(r.get("commit_sha", ""))[:10],
            str(r.get("timestamp", "")),
            str(r.get("message", "")),
            str(task.get("name", "")),
        )
    console.print(table)


@cli.command()
@click.argument("cmd")
@click.option("--timeout", type=int, default=300, help="Hard timeout in seconds.")
@click.pass_context
def run(ctx, cmd: str, timeout: int) -> None:
    """Sandboxed execution oracle (subprocess + cwd jail + timeout)."""
    service = _open_tool_service()
    payload = service.run(cmd, timeout=timeout)
    if ctx.obj.get("json"):
        _echo_json(payload)
        raise SystemExit(0 if payload.get("ok") else 1)
    if not payload.get("ok") and payload.get("error"):
        console.print(f"[red]{payload['error'].get('message', '')}[/red]")
        if payload.get("hint"):
            console.print(f"[yellow]hint: {payload['hint']}[/yellow]")
        raise SystemExit(1)
    result = (payload.get("results") or [{}])[0]
    console.print(result.get("output", ""))
    if payload.get("hint"):
        console.print(f"[yellow]{payload['hint']}[/yellow]")
    style = "green" if payload.get("ok") else "red"
    console.print(f"[{style}]exit code {result.get('exit_code', '?')}[/{style}]")
    raise SystemExit(0 if payload.get("ok") else 1)


def _reindex_with_project(service, path: str, project: str) -> dict:
    """``reindex_file`` variant honoring an explicit ``--project`` namespace.

    Mirrors :meth:`ToolService.reindex_file` (same envelope and hints) while
    passing the project through to ``Repository.reindex_file``; the facade
    method itself always uses the default project.
    """
    from cie import extract
    from cie.tools import view as _view

    tool = "reindex_file"
    started = time.monotonic()
    try:
        resolved = _view._jail(service._root, path)  # noqa: SLF001
        if not resolved.is_file():
            raise FileNotFoundError(f"no such file under project root: {path}")
        extraction = extract.extract_file(resolved)
        written = service._engine._repo.reindex_file(  # noqa: SLF001
            str(resolved), extraction, project=project
        )
    except Exception as exc:  # noqa: BLE001 - converted to envelope
        return service._guard(tool, started, exc)  # noqa: SLF001
    return service._ok(  # noqa: SLF001
        tool,
        [{"path": str(resolved), "nodes_written": written, "project": project}],
        started,
        hint="graph is fresh for this file; callers of unchanged files "
             "were re-resolved in the same call",
    )


@cli.command()
@click.argument("path")
@click.option("--project", default="", help="Project namespace to reindex into.")
@click.pass_context
def reindex(ctx, path: str, project: str) -> None:
    """Incrementally re-index one file after a landed patch."""
    service = _open_tool_service()
    if project:
        payload = _reindex_with_project(service, path, project)
    else:
        payload = service.reindex_file(path)
    results = _finish_service(ctx, payload)
    entry = results[0]
    console.print(
        f"[green]Reindexed {entry['path']}: {entry['nodes_written']} nodes written.[/green]"
    )


def _build_reindex_handler(repo, project: str, debounce: float):
    """Console-reporting wrapper over the shared ``cie.tools.watch`` handler
    (see that module's docstring for why this used to be a second,
    independently-maintained copy of the same debounce/reindex logic).
    """
    from cie.tools.watch import build_reindex_handler

    def _on_reindexed(path: str, count, exc) -> None:
        if exc is not None:
            console.print(f"[red]failed to reindex {path}: {exc}[/red]")
        else:
            console.print(f"[cyan]reindexed[/cyan] {path}: {count} nodes")

    return build_reindex_handler(repo, project, debounce, on_reindexed=_on_reindexed)


@cli.command()
@click.argument(
    "paths", nargs=-1, required=True,
    type=click.Path(exists=True, file_okay=False, path_type=Path),
)
@click.option(
    "--project", default="",
    help="Project namespace to keep in sync (defaults to CIE_PROJECT/FORGE_PROJECT_ID).",
)
@click.option(
    "--debounce", type=float, default=0.5, show_default=True,
    help="Seconds to coalesce rapid-fire filesystem events per file.",
)
def watch(paths: tuple[Path, ...], project: str, debounce: float) -> None:
    """Watch directories and incrementally reindex Neo4j on every source change.

    Runs until interrupted (Ctrl+C). Each create/modify re-parses the file
    with tree-sitter and calls `Repository.reindex_file` (delete + reinsert +
    re-resolve callers, read-after-write consistent); deletes drop the
    file's nodes. Only supported source suffixes (.py/.js/.jsx/.mjs/.cjs/
    .ts/.tsx) trigger a reindex - everything else is ignored.
    """
    from watchdog.observers import Observer

    project = project or _project_from_env()
    cfg = Neo4jConfig.from_env()
    repo = Neo4jRepository.connect(
        cfg.uri, cfg.user, cfg.password, cfg.database, project=project,
        ensure_indices=True,
        connect_timeout_s=cfg.connect_timeout_s,
        max_retry_time_s=cfg.max_retry_time_s,
        query_timeout_s=cfg.query_timeout_s,
        schema_timeout_s=cfg.schema_timeout_s,
        write_timeout_s=cfg.write_timeout_s,
    )
    handler = _build_reindex_handler(repo, project, debounce)
    observer = Observer()
    for p in paths:
        observer.schedule(handler, str(p), recursive=True)
    observer.start()
    console.print(
        f"[green]Watching {', '.join(str(p) for p in paths)} "
        f"(project={project or '<default>'})... Ctrl+C to stop.[/green]"
    )
    try:
        while observer.is_alive():
            observer.join(1)
    except KeyboardInterrupt:
        observer.stop()
    observer.join()
    repo.close()


# ---------------------------------------------------------------------------
# Operational plumbing (T3.4)
# ---------------------------------------------------------------------------


def _health_payload(engine: QueryEngine) -> dict:
    """Build the extended health dict shared by CLI and HTTP surfaces."""
    details = engine.health_details()
    return {
        "status": "ok",
        "store": details.get("store", "reachable"),
        "nodes": details.get("nodes", 0),
        "edges": details.get("edges", 0),
        "communities": details.get("communities", 0),
        "isolation": "subprocess",
        "schema_version": SCHEMA_VERSION,
        "project": details.get("project", ""),
    }


@cli.command()
@click.pass_context
def health(ctx) -> None:
    """Backend connectivity + counts + tool-layer plumbing (fail fast)."""
    engine = _open_engine()
    timer = ToolTimer()
    try:
        with timer:
            payload = _health_payload(engine)
    except Exception as exc:  # noqa: BLE001 - connectivity failure
        _close_engine(engine)
        _emit_err(
            ctx, "health", "internal", f"graph store unreachable: {exc}",
            hint="check the graph store configuration and that it is "
                 "running",
            timer=timer,
        )
    finally:
        _close_engine(engine)
    if _emit(ctx, "health", payload, timer=timer):
        return
    console.print(f"status:         {payload['status']}")
    console.print(f"store:          {payload['store']}")
    console.print(f"nodes:          {payload['nodes']}")
    console.print(f"edges:          {payload['edges']}")
    console.print(f"communities:    {payload['communities']}")
    console.print(f"isolation:      {payload['isolation']}")
    console.print(f"schema_version: {payload['schema_version']}")
    console.print(f"project:        {payload['project'] or '(default)'}")


@cli.command(name="schema-version")
@click.pass_context
def schema_version(ctx) -> None:
    """Return the atomic-task JSON Schema version enforced at ingest."""
    with ToolTimer() as timer:
        payload = {"schema_version": SCHEMA_VERSION}
    if _emit(ctx, "schema_version", payload, timer=timer):
        return
    console.print(f"atomic-task schema version: {SCHEMA_VERSION}")


def _dump_schema(output: Optional[Path]) -> Path:
    """Regenerate the atomic-task JSON Schema via scripts/dump_schema.py logic.

    Imports ``dump_schema`` from the repo's ``scripts/`` directory when it is
    available (source checkout); falls back to the identical inline logic for
    installed wheels, where ``scripts/`` is not shipped.
    """
    scripts_dir = Path(__file__).resolve().parent.parent.parent / "scripts"
    dump_file = scripts_dir / "dump_schema.py"
    if dump_file.is_file():
        import importlib.util

        spec = importlib.util.spec_from_file_location("dump_schema", dump_file)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        dump_schema = module.dump_schema
    else:  # pragma: no cover - only in installed wheels
        from cie.tasks import AtomicTaskBatch as _Batch

        def dump_schema(output_path):
            path = Path(output_path)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(
                _json.dumps(_Batch.model_json_schema(), indent=2) + "\n",
                encoding="utf-8",
            )
            return path

    target = output or (
        Path(__file__).resolve().parent / "schemas" / "atomic_task.schema.json"
    )
    return Path(dump_schema(target))


@cli.command(name="schema:dump")
@click.option(
    "--output", "output", type=click.Path(path_type=Path), default=None,
    help="Destination file (default: the packaged schemas/atomic_task.schema.json).",
)
@click.pass_context
def schema_dump(ctx, output: Optional[Path]) -> None:
    """Regenerate schemas/atomic_task.schema.json from the pydantic models."""
    with ToolTimer() as timer:
        written = _dump_schema(output)
    payload = {"schema_path": str(written), "schema_version": SCHEMA_VERSION}
    if _emit(ctx, "schema_dump", payload, timer=timer):
        return
    console.print(f"[green]wrote {written}[/green]")


# ---------------------------------------------------------------------------
# Task store commands
# ---------------------------------------------------------------------------


@cli.command(name="tasks:pending")
@click.pass_context
def tasks_pending(ctx) -> None:
    """List pending atomic tasks (feed for the generate phase)."""
    from cie.serialize import task_to_dict

    repo, driver = _open_task_repo()
    try:
        with ToolTimer() as timer:
            tasks = repo.list_pending()
        hint = None if tasks else "no pending tasks; push a batch with tasks:push"
        if _emit(ctx, "list_pending_tasks", [task_to_dict(t) for t in tasks],
                 hint=hint, timer=timer):
            return
        if not tasks:
            console.print("[yellow]No pending tasks.[/yellow]")
            return
        for t in tasks:
            console.print(f"  {t.name} [{t.task_type}] {t.description}")
    finally:
        _close_driver(driver)


@cli.command(name="tasks:push")
@click.argument(
    "batch_file", type=click.Path(exists=True, dir_okay=False, path_type=Path)
)
@click.pass_context
def tasks_push(ctx, batch_file: Path) -> None:
    """Push a batch of atomic tasks (partial accept; rejections reported)."""
    tool = "push_tasks"
    try:
        raw = _json.loads(batch_file.read_text(encoding="utf-8"))
        batch = AtomicTaskBatch.model_validate(raw)
    except (ValueError, OSError) as exc:
        _emit_err(
            ctx, tool, "validation", f"invalid batch file: {exc}",
            hint="validate the batch against schemas/atomic_task.schema.json "
                 "(regenerate with `cie schema:dump`)",
        )
    repo, driver = _open_task_repo()
    try:
        with ToolTimer() as timer:
            result = repo.push_tasks(batch)
    finally:
        _close_driver(driver)
    payload = {
        "accepted": result.accepted,
        "rejected": [r.model_dump() for r in result.rejected],
    }
    hint = None
    if result.rejected:
        hint = (
            f"{len(result.rejected)} task(s) rejected; fix the reasons and "
            "re-push (push is idempotent per task name)"
        )
    if _emit(ctx, tool, payload, hint=hint, timer=timer):
        return
    console.print(f"[green]accepted: {result.accepted}[/green]")
    for r in result.rejected:
        console.print(f"  [red]rejected {r.name}: {r.reason}[/red]")


@cli.command(name="tasks:get")
@click.argument("name")
@click.pass_context
def tasks_get(ctx, name: str) -> None:
    """Return one atomic task by name."""
    service = _open_tool_service()
    payload = service.get_task(name)
    results = _finish_service(ctx, payload)
    task = results[0]
    console.print(f"[bold]Task: {task.get('name')}[/bold]")
    for key in (
        "id", "userstory_id", "parent_feature", "task_type", "layer",
        "action", "file_path", "description",
    ):
        console.print(f"  {key}: {task.get(key)}")
    if task.get("dependencies"):
        console.print(f"  dependencies: {', '.join(task['dependencies'])}")


@cli.command(name="tasks:closure")
@click.argument("name")
@click.pass_context
def tasks_closure(ctx, name: str) -> None:
    """Transitive task-to-task dependency closure (forge topo-sort feed)."""
    service = _open_tool_service()
    payload = service.task_dependency_closure(name)
    results = _finish_service(ctx, payload)
    if not results:
        console.print(f"[yellow]{payload.get('hint')}[/yellow]")
        return
    console.print(f"[bold]Dependency closure of {name} ({len(results)}):[/bold]")
    for task in results:
        console.print(f"  {task.get('name')} ({task.get('file_path', '')})")


@cli.command(name="tasks:deps")
@click.argument("name")
@click.pass_context
def tasks_deps(ctx, name: str) -> None:
    """Show the transitive file-path dependency closure of a task."""
    repo, driver = _open_task_repo()
    try:
        with ToolTimer() as timer:
            deps = repo.get_dependencies(name)
        hint = None if deps else (
            f"no dependencies found for '{name}'; the task may be unknown — "
            "check with `cie tasks:get <name>`"
        )
        if _emit(ctx, "task_dependencies", list(deps), hint=hint, timer=timer):
            return
        if not deps:
            console.print(f"[yellow]No dependencies found for '{name}'.[/yellow]")
            return
        for d in deps:
            console.print(f"  {d}")
    finally:
        _close_driver(driver)


# ---------------------------------------------------------------------------
# Consistency validation commands
# ---------------------------------------------------------------------------


@cli.command(name="validate:cycles")
@click.pass_context
def validate_cycles(ctx) -> None:
    """Detect cycles in the task dependency graph."""
    repo, driver = _open_task_repo()
    try:
        with ToolTimer() as timer:
            result = repo.validate_cycles()
        payload = {"has_cycle": result.has_cycle, "cycle": list(result.cycle)}
        hint = (
            "break the cycle or re-push the involved tasks; inspect with "
            "`cie tasks:closure <name>`"
            if result.has_cycle
            else None
        )
        if _emit(ctx, "validate_cycles", payload, hint=hint, timer=timer):
            return
        if result.has_cycle:
            console.print(f"[red]CYCLE DETECTED: {' -> '.join(result.cycle)}[/red]")
        else:
            console.print("[green]No cycles found.[/green]")
    finally:
        _close_driver(driver)


@cli.command(name="validate:coverage")
@click.pass_context
def validate_coverage(ctx) -> None:
    """Find dev tasks with no test artifact and no linked QA task."""
    repo, driver = _open_task_repo()
    try:
        with ToolTimer() as timer:
            gaps = repo.validate_coverage()
        hint = (
            "link a QA task or produce a test artifact for each gap"
            if gaps
            else "all dev tasks have a linked QA task or a test artifact"
        )
        if _emit(ctx, "validate_coverage", [g.model_dump() for g in gaps],
                 hint=hint, timer=timer):
            return
        if not gaps:
            console.print("[green]All tasks have test coverage.[/green]")
            return
        console.print(f"[yellow]Coverage gaps ({len(gaps)}):[/yellow]")
        for g in gaps:
            console.print(f"  {g.task_name} ({g.file_path})")
    finally:
        _close_driver(driver)


# ---------------------------------------------------------------------------
# QA coverage commands (be-v2/docs/design/qa-persona-cie-knowledge-graph.md)
# ---------------------------------------------------------------------------


def _parse_uncovered_lines(raw: str) -> list[int]:
    if not raw or not raw.strip():
        return []
    return [int(n.strip()) for n in raw.split(",") if n.strip()]


@cli.command(name="coverage:record")
@click.argument("file_path")
@click.option("--coverage-pct", type=float, required=True)
@click.option("--covered-lines", type=int, required=True)
@click.option(
    "--uncovered-lines", default="",
    help="Comma-separated line numbers, e.g. '12,13,47'.",
)
@click.option("--subtree", default="", help='e.g. "be-v2" or "frontend".')
@click.option("--project", default="", help="Project namespace.")
@click.pass_context
def coverage_record(
    ctx, file_path: str, coverage_pct: float, covered_lines: int,
    uncovered_lines: str, subtree: str, project: str,
) -> None:
    """Record one file's coverage measurement + derived per-function
    breakdown. Requires the file already indexed (`cie load`/`reindex`)."""
    tool = "record_coverage"
    try:
        uncovered = _parse_uncovered_lines(uncovered_lines)
    except ValueError as exc:
        _emit_err(
            ctx, tool, "validation", f"invalid --uncovered-lines: {exc}",
            hint="comma-separated integers, e.g. '12,13,47'",
        )
    engine = _open_engine(project)
    timer = ToolTimer()
    try:
        with timer:
            measured_at = datetime.now(timezone.utc).isoformat()
            result = engine.record_coverage(
                file_path, subtree, coverage_pct, covered_lines, uncovered,
                measured_at,
            )
    finally:
        _close_engine(engine)
    if result is None:
        _emit_err(
            ctx, tool, "not_found", f"no indexed FILE node for '{file_path}'",
            hint="index the file first (cie load/reindex) before recording "
                 "coverage",
            timer=timer,
        )
    payload = dataclasses.asdict(result)
    if _emit(ctx, tool, payload, timer=timer):
        return
    console.print(
        f"[green]{file_path}: {result.coverage_pct}% "
        f"({len(result.functions)} functions derived).[/green]"
    )


@cli.command(name="coverage:get")
@click.argument("file_path")
@click.option("--subtree", default="")
@click.option("--project", default="", help="Project namespace.")
@click.pass_context
def coverage_get(ctx, file_path: str, subtree: str, project: str) -> None:
    """Show one file's latest coverage + per-function breakdown."""
    tool = "get_coverage"
    engine = _open_engine(project)
    timer = ToolTimer()
    try:
        with timer:
            result = engine.get_coverage(file_path, subtree)
    finally:
        _close_engine(engine)
    if result is None:
        _emit_err(
            ctx, tool, "not_found", f"'{file_path}' has never been measured",
            hint="run coverage:record first, or check the path with "
                 "search-symbol",
            timer=timer,
        )
    payload = dataclasses.asdict(result)
    if _emit(ctx, tool, payload, timer=timer):
        return
    console.print(f"[bold]{file_path}: {result.coverage_pct}%[/bold]")
    for fn in result.functions:
        console.print(f"  {fn.label} ({fn.line_start}-{fn.line_end}): {fn.coverage_pct}%")


@cli.command(name="coverage:report")
@click.option("--subtree", default="")
@click.option("--below-pct", type=float, default=None)
@click.option("--file-glob", default="")
@click.option("--include-unmeasured/--no-include-unmeasured", default=True)
@click.option("--project", default="", help="Project namespace.")
@click.pass_context
def coverage_report_cmd(
    ctx, subtree: str, below_pct: Optional[float], file_glob: str,
    include_unmeasured: bool, project: str,
) -> None:
    """Every file's coverage, worst-first — "which modules need better
    coverage"."""
    tool = "coverage_report"
    engine = _open_engine(project)
    timer = ToolTimer()
    try:
        with timer:
            results = engine.coverage_report(
                subtree, below_pct, file_glob, include_unmeasured,
            )
    finally:
        _close_engine(engine)
    hint = (
        "no FILE nodes matched the filters; check subtree/file_glob or "
        "index the project first"
        if not results else None
    )
    payload = [dataclasses.asdict(r) for r in results]
    if _emit(ctx, tool, payload, hint=hint, timer=timer):
        return
    if not results:
        console.print(f"[yellow]{hint}[/yellow]")
        return
    for r in results:
        pct = f"{r.coverage_pct}%" if r.measured else "never measured"
        console.print(f"  {r.file_path}: {pct}")


@cli.command(name="coverage:snapshot")
@click.option("--subtree", default="", help='e.g. "be-v2" or "frontend".')
@click.option("--aggregate-pct", type=float, required=True)
@click.option("--files-measured", type=int, required=True)
@click.option("--total-lines", type=int, required=True)
@click.option("--covered-lines", type=int, required=True)
@click.option("--project", default="", help="Project namespace.")
@click.pass_context
def coverage_snapshot(
    ctx, subtree: str, aggregate_pct: float, files_measured: int,
    total_lines: int, covered_lines: int, project: str,
) -> None:
    """Append one aggregate coverage measurement for trend history.
    Always adds a new snapshot — never overwrites the last one."""
    tool = "record_coverage_snapshot"
    engine = _open_engine(project)
    timer = ToolTimer()
    try:
        with timer:
            snapshot = CoverageSnapshot(
                project=project, subtree=subtree, aggregate_pct=aggregate_pct,
                files_measured=files_measured, total_lines=total_lines,
                covered_lines=covered_lines,
                measured_at=datetime.now(timezone.utc).isoformat(),
            )
            engine.record_coverage_snapshot(snapshot)
    finally:
        _close_engine(engine)
    payload = dataclasses.asdict(snapshot)
    if _emit(ctx, tool, payload, timer=timer):
        return
    console.print(f"[green]Snapshot recorded: {aggregate_pct}% aggregate.[/green]")


@cli.command(name="coverage:trend")
@click.option("--subtree", default="")
@click.option("--limit", type=int, default=20)
@click.option("--project", default="", help="Project namespace.")
@click.pass_context
def coverage_trend_cmd(ctx, subtree: str, limit: int, project: str) -> None:
    """Most recent coverage snapshots, most recent first — "is coverage
    improving or regressing"."""
    tool = "coverage_trend"
    engine = _open_engine(project)
    timer = ToolTimer()
    try:
        with timer:
            results = engine.coverage_trend(subtree, limit)
    finally:
        _close_engine(engine)
    hint = (
        "no snapshots recorded yet; call coverage:snapshot after a "
        "coverage run"
        if not results else None
    )
    payload = [dataclasses.asdict(r) for r in results]
    if _emit(ctx, tool, payload, hint=hint, timer=timer):
        return
    if not results:
        console.print(f"[yellow]{hint}[/yellow]")
        return
    for s in results:
        console.print(f"  {s.measured_at}: {s.aggregate_pct}% ({s.subtree or 'all'})")


# ---------------------------------------------------------------------------
# PRD hierarchy commands
# ---------------------------------------------------------------------------


@cli.command(name="hierarchy:push")
@click.argument(
    "tree_file", type=click.Path(exists=True, dir_okay=False, path_type=Path)
)
@click.option("--project", default="", help="Project namespace for the tree.")
@click.pass_context
def hierarchy_push(ctx, tree_file: Path, project: str) -> None:
    """Ingest a planner hierarchy tree (Module->...->UserStory) from JSON."""
    tool = "push_hierarchy"
    try:
        root = HierarchyNode.model_validate(
            _json.loads(tree_file.read_text(encoding="utf-8"))
        )
    except (ValueError, OSError) as exc:
        _emit_err(
            ctx, tool, "validation", f"invalid hierarchy file: {exc}",
            hint="expected a HierarchyNode tree: {node_type, name, id?, "
                 "description?, metadata?, children?}",
        )
    repo, driver = _open_hierarchy_repo(project)
    timer = ToolTimer()
    try:
        with timer:
            written = repo.push_hierarchy(root, project=project)
    except ValueError as exc:
        _close_driver(driver)
        _emit_err(
            ctx, tool, "validation", str(exc),
            hint="hierarchy ids must be unique on every root-to-leaf path",
            timer=timer,
        )
    finally:
        _close_driver(driver)
    payload = {
        "nodes_written": written,
        "root_id": root.id,
        "root_name": root.name,
        "project": project,
    }
    if _emit(ctx, tool, payload, timer=timer):
        return
    console.print(
        f"[green]Wrote {written} hierarchy nodes "
        f"(root: {root.name}).[/green]"
    )


@cli.command(name="hierarchy:children")
@click.argument("node_id")
@click.option("--depth", type=int, default=0, help="Max depth (0 = unlimited).")
@click.option("--type", "type_filter", default="",
              help="Restrict results to one node_type (e.g. userstory).")
@click.option("--limit", type=int, default=200, help="Max children returned.")
@click.pass_context
def hierarchy_children(ctx, node_id: str, depth: int, type_filter: str,
                       limit: int) -> None:
    """BFS 'all children' traversal of the PRD hierarchy."""
    tool = "get_children"
    repo, driver = _open_hierarchy_repo()
    timer = ToolTimer()
    try:
        with timer:
            subtree = repo.get_children(
                node_id, depth=depth, type_filter=type_filter, limit=limit
            )
    except ValueError:
        _close_driver(driver)
        _emit_err(
            ctx, tool, "not_found", f"unknown hierarchy node '{node_id}'",
            hint="push the hierarchy first with `cie hierarchy:push`",
            timer=timer,
        )
    finally:
        _close_driver(driver)
    payload = {
        "root": subtree.root.model_dump(mode="json"),
        "children": [c.model_dump(mode="json") for c in subtree.children],
    }
    hint = (
        f"children capped at {limit}; narrow with --depth or --type"
        if subtree.truncated
        else None
    )
    if _emit(ctx, tool, payload, truncated=subtree.truncated, hint=hint,
             timer=timer):
        return
    root = subtree.root
    console.print(f"[bold]{root.node_type}: {root.name} ({root.id})[/bold]")
    for c in subtree.children:
        indent = "  " * c.depth
        console.print(f"{indent}{c.node_type}: {c.name} [{c.id}]")
    if subtree.truncated:
        console.print(f"[yellow]... truncated at {limit} children[/yellow]")


@cli.command(name="hierarchy:lineage")
@click.argument("node_id")
@click.pass_context
def hierarchy_lineage(ctx, node_id: str) -> None:
    """Ancestor path of a hierarchy node, root-first."""
    tool = "get_lineage"
    repo, driver = _open_hierarchy_repo()
    try:
        with ToolTimer() as timer:
            lineage = repo.get_lineage(node_id)
    finally:
        _close_driver(driver)
    hint = None if lineage else (
        f"unknown hierarchy node '{node_id}'; push the hierarchy first with "
        "`cie hierarchy:push`"
    )
    if _emit(ctx, tool, [v.model_dump(mode="json") for v in lineage],
             hint=hint, timer=timer):
        return
    if not lineage:
        console.print(f"[yellow]Unknown hierarchy node '{node_id}'.[/yellow]")
        return
    console.print(" -> ".join(f"{v.node_type}:{v.name}" for v in lineage))


# ---------------------------------------------------------------------------
# Server
# ---------------------------------------------------------------------------


@cli.command()
def serve() -> None:
    """No longer starts a standalone server.

    cie's HTTP surface (cie/routes.py) is mounted directly into
    the be-v2 app (see app/api.py) and runs in that process. Start the app
    instead:

        cd be-v2 && .venv/bin/uvicorn app.api:app --port 8001

    cie's endpoints are then available at /health, /tools/{tool},
    /code/*, /tasks/*, /validate/*, /query/* on that same port — no second
    process, no separate CIE_URL to point forge at.
    """
    console.print(
        "[yellow]`cie serve` is retired — cie is mounted into the "
        "be-v2 app now.[/yellow]\n"
        "Run: [bold]cd be-v2 && .venv/bin/uvicorn app.api:app --port 8001[/bold]\n"
        "cie's routes are then live on that same port "
        "(/health, /tools/{tool}, /code/*, /tasks/*, /validate/*, /query/*)."
    )
    sys.exit(1)


def main() -> None:
    """Entry point for the `cie` console script."""
    try:
        cli()
    except Exception as exc:  # pragma: no cover - network/DB errors only
        console.print(f"[red]error: {exc}[/red]")
        sys.exit(1)


if __name__ == "__main__":
    main()
