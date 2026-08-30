"""SQLite-backed PRD-hierarchy store — the embedded twin of
`cie.hierarchy.Neo4jHierarchyRepository` (roadmap R14).

Completes the "everything works on the default backend" story started by
B1 (embedded task repository): the PRD decomposition tree
(Module/Feature/Workflow/UseCase/UserStory) can now also live in a local
SQLite file (default `<root>/.cie/hierarchy.db`) instead of Neo4j —
the last Neo4j-only feature, per goal.md's own leftover note.

Same `HierarchyRepository` protocol (see cie.hierarchy for the contract),
same validation semantics (shared `find_repeated_id`/`count_nodes`
helpers — imported, not duplicated), same read shapes
(`HierarchyNodeView`/`HierarchySubtree` from `cie.tasks`). Where the
backends INTENTIONALLY differ:

- **One parent direction.** The Neo4j implementation walks a mixed edge
  world (its own `HAS_CHILD` union plus the host wide-schema's
  `CHILD_OF`) because real host data carries both directions. This
  store is written ONLY by `push_hierarchy`, which writes
  `(parent)-[:HAS_CHILD]->(child)` — one `hierarchy_edges` table with a
  `kind` column replaces all of that.
- **Label unions collapse to a `node_type` column.** Neo4j needs
  TYPE_TO_LABEL/LABEL_TO_TYPE because Neo4j labels ARE the schema;
  SQLite stores the canonical `node_type` directly.
- **REALIZED_BY edges are written name-keyed and unconditionally.** The
  Neo4j write MERGEs them only when the matching `AtomicTask` node
  exists in the same project; this store's task layer lives in a
  separate file (`tasks.db`), so validating at write time would couple
  two files the hierarchy schema deliberately doesn't know about.
  `metadata['task_names']` still drives the edges; a read-side join can
  tighten this later if a consumer needs it. Documented difference,
  not a silent one.

Not multi-process-safe (same single-connection trade as
`EmbeddedRepository`/`EmbeddedTaskRepository`: thread-safe via the
shared `_ThreadSafeSQLite`, single writer). Reach for
`Neo4jHierarchyRepository` when that stops being fine.
"""

from __future__ import annotations

import json
from pathlib import Path

from cie.hierarchy import count_nodes, find_repeated_id
from cie.tasks import HierarchyNode, HierarchyNodeView, HierarchySubtree

_SCHEMA = """
CREATE TABLE IF NOT EXISTS hierarchy_nodes (
    id TEXT NOT NULL,
    project TEXT NOT NULL DEFAULT '',
    node_type TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    metadata_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (id, project)
);
CREATE TABLE IF NOT EXISTS hierarchy_edges (
    parent_id TEXT NOT NULL,
    child_id TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'HAS_CHILD',
    project TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (parent_id, child_id, kind, project)
);
CREATE INDEX IF NOT EXISTS idx_hier_edges_child ON hierarchy_edges(child_id, project);
CREATE INDEX IF NOT EXISTS idx_hier_edges_parent ON hierarchy_edges(parent_id, project);
"""

from cie.embedded_task_repository import _ThreadSafeSQLite  # noqa: E402


def _view_from(node_type: str, props: dict, depth: int = 0) -> HierarchyNodeView:
    """Local twin of cie.hierarchy._view_from_props (same shape; one
    function per module so the Neo4j module's JSON nuances stay its own)."""
    metadata: dict = {}
    raw = props.get("metadata_json")
    if raw:
        try:
            metadata = json.loads(raw)
        except (TypeError, ValueError):
            metadata = {}
    return HierarchyNodeView(
        node_type=node_type,
        id=props.get("id", ""),
        name=props.get("name", ""),
        description=props.get("description", ""),
        metadata=metadata,
        depth=depth,
    )


class SQLiteHierarchyRepository:
    """HierarchyRepository over one local SQLite file. See the module
    docstring for the intentional differences from the Neo4j backend."""

    def __init__(self, db_path: Path | str, project: str = ""):
        self._db_path = Path(db_path)
        self._db_path.parent.mkdir(parents=True, exist_ok=True)
        self._conn = _ThreadSafeSQLite(str(self._db_path))
        self._conn.executescript(_SCHEMA)
        self._conn.commit()
        self._project = project

    # -- write ------------------------------------------------------------

    def push_hierarchy(self, root: HierarchyNode, project: str = "") -> int:
        """MERGE the whole tree, idempotent by (node id, project); see the
        protocol. Same root-to-leaf repeated-id ValueError the Neo4j repo
        raises (shared validator, shared wording)."""
        repeated = find_repeated_id(root)
        if repeated is not None:
            raise ValueError(
                f"hierarchy tree repeats id '{repeated}' on a root-to-leaf path"
            )
        effective_project = project or self._project
        total = count_nodes(root)
        stack: list[tuple[object, HierarchyNode]] = [(None, root)]
        while stack:
            parent, node = stack.pop()
            self._conn.execute(
                "INSERT INTO hierarchy_nodes (id, project, node_type, name, "
                "description, metadata_json) VALUES (?, ?, ?, ?, ?, ?) "
                "ON CONFLICT (id, project) DO UPDATE SET "
                "node_type = excluded.node_type, name = excluded.name, "
                "description = excluded.description, "
                "metadata_json = excluded.metadata_json",
                (
                    node.id, effective_project, node.node_type,
                    node.name, node.description, json.dumps(node.metadata),
                ),
            )
            if parent is not None:
                self._conn.execute(
                    "INSERT INTO hierarchy_edges (parent_id, child_id, kind, project) "
                    "VALUES (?, ?, 'HAS_CHILD', ?) "
                    "ON CONFLICT (parent_id, child_id, kind, project) DO NOTHING",
                    (parent.id, node.id, effective_project),
                )
            if node.node_type == "userstory":
                for task_name in node.metadata.get("task_names") or []:
                    if not isinstance(task_name, str):
                        continue
                    self._conn.execute(
                        "INSERT OR IGNORE INTO hierarchy_edges "
                        "(parent_id, child_id, kind, project) VALUES (?, ?, 'REALIZED_BY', ?)",
                        (node.id, task_name, effective_project),
                    )
            for child in node.children:
                stack.append((node, child))
        self._conn.commit()
        return total

    # -- reads ------------------------------------------------------------

    def _project_graph(
        self, project: str,
    ) -> tuple[dict[str, tuple[dict, str]], dict[str, list[str]]]:
        """One bulk fetch of the project's hierarchy rows (see the Neo4j
        twin's `_project_graph` for the shape this mirrors). Returns
        ``(nodes_by_id, children_of)`` where each node entry is
        ``(props_dict, node_type)``."""
        nodes_by_id: dict[str, tuple[dict, str]] = {}
        for node_id, node_type, name, description, metadata_json in self._conn.execute(
            "SELECT id, node_type, name, description, metadata_json "
            "FROM hierarchy_nodes WHERE project = ?",
            (project,),
        ):
            nodes_by_id[node_id] = (
                {
                    "id": node_id, "name": name, "description": description,
                    "metadata_json": metadata_json,
                },
                node_type,
            )
        children_of: dict[str, list[str]] = {}
        for parent_id, child_id in self._conn.execute(
            "SELECT parent_id, child_id FROM hierarchy_edges "
            "WHERE project = ? AND kind = 'HAS_CHILD'",
            (project,),
        ):
            children_of.setdefault(parent_id, []).append(child_id)
        return nodes_by_id, children_of

    def get_children(
        self, node_id: str, depth: int = 0, type_filter: str = "", limit: int = 200
    ) -> HierarchySubtree:
        """BFS 'all children' traversal; see the protocol for the contract
        (same ordering/limit/type_filter semantics as the Neo4j repo)."""
        nodes_by_id, children_of = self._project_graph(self._project)
        if node_id not in nodes_by_id:
            raise ValueError(f"unknown hierarchy node '{node_id}'")
        root_props, root_type = nodes_by_id[node_id]
        root_view = _view_from(root_type, root_props, depth=0)

        max_hops = depth if depth > 0 else 100
        children: list[HierarchyNodeView] = []
        truncated = False
        frontier = [node_id]
        seen = {node_id}
        hop = 0
        while frontier and hop < max_hops:
            hop += 1
            next_frontier: list[str] = []
            level_nodes: list[tuple[str, dict, str]] = []
            for pid in frontier:
                for cid in children_of.get(pid, []):
                    if cid in seen:
                        continue
                    seen.add(cid)
                    next_frontier.append(cid)
                    if cid in nodes_by_id:
                        props, ntype = nodes_by_id[cid]
                        level_nodes.append((cid, props, ntype))
            level_nodes.sort(key=lambda t: t[1].get("name", ""))
            for _cid, props, ntype in level_nodes:
                if type_filter and ntype != type_filter:
                    continue
                if len(children) >= limit:
                    truncated = True
                    frontier = []
                    break
                children.append(_view_from(ntype, props, depth=hop))
            if truncated:
                break
            frontier = next_frontier
        return HierarchySubtree(root=root_view, children=children, truncated=truncated)

    def get_lineage(self, node_id: str) -> list[HierarchyNodeView]:
        """Ancestor path root-first, node itself last; [] for unknown ids
        (same shape and ordering as the Neo4j repo)."""
        nodes_by_id, children_of = self._project_graph(self._project)
        if node_id not in nodes_by_id:
            return []
        parent_of: dict[str, str] = {}
        for parent_id, kids in children_of.items():
            for cid in kids:
                parent_of[cid] = parent_id
        chain: list[str] = [node_id]
        current = node_id
        while current in parent_of:
            current = parent_of[current]
            if current in chain:  # defensive: never loop on a malformed graph
                break
            chain.append(current)
        chain.reverse()  # root-first, node itself last
        lineage: list[HierarchyNodeView] = []
        for depth, nid in enumerate(chain):
            if nid not in nodes_by_id:
                continue
            props, ntype = nodes_by_id[nid]
            lineage.append(_view_from(ntype, props, depth=depth))
        return lineage

    def get_hierarchy_node(self, node_id: str) -> HierarchyNodeView | None:
        """One node by id (project-scoped), or None when unknown."""
        nodes_by_id, _children = self._project_graph(self._project)
        if node_id not in nodes_by_id:
            return None
        props, ntype = nodes_by_id[node_id]
        return _view_from(ntype, props, depth=0)

    def get_project_tree(self, project: str) -> list[HierarchyNodeView]:
        """Every node scoped to ``project``, each with ``parent_id`` and
        ``children_ids`` resolved from the HAS_CHILD edges (see the
        protocol for the contract)."""
        nodes_by_id, children_of = self._project_graph(project)
        parent_of: dict[str, str] = {}
        for parent_id, kids in children_of.items():
            for cid in kids:
                parent_of[cid] = parent_id
        tree: list[HierarchyNodeView] = []
        for node_id, (props, ntype) in nodes_by_id.items():
            view = _view_from(ntype, props, depth=0)
            view.parent_id = parent_of.get(node_id)
            view.children_ids = sorted(children_of.get(node_id, []))
            tree.append(view)
        tree.sort(key=lambda v: (v.node_type, v.id))
        return tree