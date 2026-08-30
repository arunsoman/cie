"""PRD hierarchy store: Module/Feature/Workflow/UseCase/UserStory.

The hierarchy is created by an external planning system; this module only
stores, links, and traverses it. Graph shapes (integration spec + SPEC §5.3):

    (:Module|:Feature|:Workflow|:UseCase|:UserStory
        {id, name, description, metadata_json, project})
    (parent)-[:HAS_CHILD]->(child)
    (:UserStory)-[:REALIZED_BY]->(:AtomicTask)   # via metadata['task_names']

All Cypher is APOC-free. The embedded twin (`SQLiteHierarchyRepository`,
R14) lives in `cie/embedded_hierarchy_repository.py` and implements the
same protocol over one local SQLite file. (An earlier version of this
docstring pointed at an in-memory fake at
`tests/in_memory_hierarchy_repo.py` — that file never existed in this
repository, found and corrected during R14.)
"""

from __future__ import annotations

import json
from typing import Optional, Protocol, runtime_checkable

from cie.tasks import HierarchyNode, HierarchyNodeView, HierarchySubtree

try:  # be-v2's real typed-relationship union, when cie runs embedded in be-v2
    from core.graph.repository import rel_type_union as _bev2_rel_type_union  # type: ignore
except Exception:  # standalone shim — cie installs without be-v2 on the path
    def _bev2_rel_type_union() -> str:
        """Mirrors core.graph.repository.rel_type_union()'s value — kept as
        a literal duplicate (not imported) so this package still installs
        and runs standalone without be-v2 on the path, same shim style as
        this module's own GraphBaseModel/bind_link soft import in
        cie/tasks.py. Update alongside core.graph.repository.
        LABEL_TO_REL_TYPE if that mapping ever changes."""
        return "HAS_CHILD|HAS_MODULE|HAS_CAPABILITY|HAS_USE_CASE|HAS_USER_STORY|HAS_TASK|OUTPUTS"

#: The relationship-type union this module's own HAS_CHILD reads now match
#: (see _project_graph below) — core.graph.repository.save_entity (used by
#: features/discover's profile links, features/enrich_graph's add_a_child,
#: and every features/mine.* service via LlmRepository.persist) no longer
#: writes every parent_ids/child_ids edge as plain HAS_CHILD; some are now
#: typed (HAS_MODULE/HAS_USER_STORY/...) by child label. Without this,
#: e.g. a UserStory created via POST /api/mine {action:"mine_add_node"}
#: (which links ONLY through core.graph.repository, no CHILD_OF dual-write)
#: would silently vanish from this module's tree traversal the moment its
#: edge type changed from HAS_CHILD to HAS_USER_STORY.
_HAS_CHILD_UNION = _bev2_rel_type_union()

#: node_type -> Neo4j label (and its inverse for reads).
#:
#: "project"/"actor"/"capability"/"task" were added so this module can read
#: the REAL PRD-extraction entities features/extract_shim writes (via
#: core.graph.backend_entity/core.graph.client) — those share the exact
#: same bare label names (Neo4j labels accumulate: a node written as
#: :Entity:UserStory by the wide schema also carries plain :UserStory, the
#: label this module already matched on). "feature"/"workflow" are left
#: alone — they belong to the external planning system this module's
#: original docstring describes, not to extract_shim's output, and nothing
#: here should stop matching them.
#: The 10 discover-module profile types (features/discover/routes.py's
#: PROFILE_REGISTRY, same keys) — added alongside a fix to that module's
#: assess/assess-all handlers, which stamped a `project` property but never
#: an actual HAS_CHILD edge to the Project node (confirmed gap, found
#: 2026-08-02 running a real extraction + discovery pass through this
#: endpoint and seeing profiles never appear no matter how the tree query
#: was extended). Without an entry here, even a correctly-linked profile
#: node would still be invisible to this module's label filter.
_DISCOVER_PROFILE_LABELS = {
    "application_type_and_paradigm": "ApplicationArchitecture",
    "compliance": "ComplianceAndAuditingProfile",
    "data_architecture_and_state_management": "DataArchitectureProfile",
    "infrastructure_and_deployment": "InfrastructureAndDevOpsProfile",
    "licensing": "BusinessAndLicensingProfile",
    "privacy": "DataPrivacyProfile",
    "scale_users_and_performance": "ScaleAndPerformanceProfile",
    "security": "SecurityArchitectureProfile",
    "thirdparty_modules": "ThirdPartyIntegrationsProfile",
    "tech_stack": "TechStack",
}

TYPE_TO_LABEL = {
    "module": "Module",
    "feature": "Feature",
    "workflow": "Workflow",
    "usecase": "UseCase",
    "userstory": "UserStory",
    "project": "Project",
    "actor": "Actor",
    "capability": "Capability",
    "task": "Task",
    **_DISCOVER_PROFILE_LABELS,
}
LABEL_TO_TYPE = {v: k for k, v in TYPE_TO_LABEL.items()}

_HIERARCHY_LABELS = list(TYPE_TO_LABEL.values())


def _view_from_props(node_type: str, props: dict, depth: int = 0) -> HierarchyNodeView:
    """Build a HierarchyNodeView from stored node properties."""
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


def _type_of_labels(labels: list[str]) -> Optional[str]:
    """Return the hierarchy node_type for a node's labels, or None."""
    for label in labels:
        if label in LABEL_TO_TYPE:
            return LABEL_TO_TYPE[label]
    return None


def find_repeated_id(root: HierarchyNode) -> Optional[str]:
    """Return the first id repeated on any root-to-leaf path, or None.

    The hierarchy must be a tree: ids are the MERGE key, so a repeated id on
    a path would create a cycle (or a diamond) in the stored graph.
    """
    stack: list[tuple[HierarchyNode, frozenset[str]]] = [(root, frozenset())]
    while stack:
        node, ancestors = stack.pop()
        if node.id in ancestors:
            return node.id
        path = ancestors | {node.id}
        for child in node.children:
            stack.append((child, path))
    return None


def count_nodes(root: HierarchyNode) -> int:
    """Count unique node ids in the tree."""
    seen: set[str] = set()
    stack = [root]
    while stack:
        node = stack.pop()
        seen.add(node.id)
        stack.extend(node.children)
    return len(seen)


@runtime_checkable
class HierarchyRepository(Protocol):
    """Storage + traversal contract for the PRD hierarchy."""

    def push_hierarchy(self, root: HierarchyNode, project: str = "") -> int:
        """MERGE the whole tree (idempotent by node id).

        Writes one labeled node per HierarchyNode, HAS_CHILD edges per tree
        structure, and REALIZED_BY edges from userstory nodes whose
        ``metadata['task_names']`` match stored AtomicTask names. Returns the
        number of unique nodes written. Raises ``ValueError`` naming the
        duplicated id when an id repeats on any root-to-leaf path.
        """

    def get_children(
        self, node_id: str, depth: int = 0, type_filter: str = "", limit: int = 200
    ) -> HierarchySubtree:
        """THE 'all children' API.

        Breadth-first over HAS_CHILD, ordered by depth then name. ``depth=0``
        means unlimited; otherwise only nodes up to that distance from the
        root are returned. ``type_filter`` restricts the RESULTS to one
        node_type (traversal still passes through other types). At most
        ``limit`` children are returned; when more exist, ``truncated=True``.
        """

    def get_lineage(self, node_id: str) -> list[HierarchyNodeView]:
        """Return the ancestor path root-first (node itself last).

        Empty list when the id is unknown.
        """

    def get_hierarchy_node(self, node_id: str) -> Optional[HierarchyNodeView]:
        """Return a single node by id, or None when unknown."""

    def get_project_tree(self, project: str) -> list[HierarchyNodeView]:
        """Every node scoped to ``project``, each with ``parent_id`` and
        ``children_ids`` resolved from a live edge traversal (see
        ``_project_graph``) — the flat "get all nodes of a project" API.
        Empty list when the project has no hierarchy nodes.
        """


class Neo4jHierarchyRepository:
    """Neo4j-backed HierarchyRepository (APOC-free, project-aware).

    When constructed with ``project="x"``, every read restricts matches to
    nodes stamped with that project; nodes without a ``project`` property
    belong to the default ("") project only.
    """

    def __init__(self, driver, project: str = ""):
        self._driver = driver
        self._project = project

    @classmethod
    def from_driver(cls, driver, project: str = "") -> "Neo4jHierarchyRepository":
        """Build a repository from an existing neo4j Driver."""
        return cls(driver, project=project)

    def _run(self, query: str, params: Optional[dict] = None):
        with self._driver.session() as session:
            return list(session.run(query, params or {}))

    def _project_params(self, project: str) -> dict:
        return {"project": project}

    # -- writes -------------------------------------------------------------

    def push_hierarchy(self, root: HierarchyNode, project: str = "") -> int:
        """MERGE the whole tree (idempotent by (node id, project) — see
        the 2026-08-15 fix note below for how a node written before that
        key existed still resolves to one node, not two); see protocol."""
        repeated = find_repeated_id(root)
        if repeated is not None:
            raise ValueError(
                f"hierarchy tree repeats id '{repeated}' on a root-to-leaf path"
            )
        effective_project = project or self._project
        total = count_nodes(root)
        root_stack: list[tuple[Optional[HierarchyNode], HierarchyNode]] = [(None, root)]

        # 2026-08-14 fix (RF3): the node MERGE now keys on {id, project}
        # instead of bare {id} (a colliding hierarchy id from a different
        # project used to silently reparent/overwrite that node into the
        # new project), the REALIZED_BY match scopes AtomicTask by project
        # too (same bare-name cross-project mislink bug class as RF2), and
        # the whole tree write is wrapped in one execute_write transaction
        # instead of per-statement auto-commit.
        #
        # 2026-08-15 fix: RF3's new {id, project} MERGE key does not
        # match a node written before RF3 shipped (bare {id}, so it has
        # NO `project` property at all — property absence != `project:
        # ""` in Neo4j's exact-match MERGE semantics) — confirmed live:
        # 36/75 Module, 78/417 UserStory, and 4/11 Project nodes in the
        # shared Neo4j have no `project` property today. Re-pushing any
        # tree that touches one of those ids would silently fork a
        # second, id-colliding node instead of updating the existing one
        # in place — exactly what this method's own docstring claims
        # can't happen. The OPTIONAL MATCH below adopts such a legacy
        # node into `effective_project` the first time this method
        # touches its id (one-time, self-resolving migration-on-write);
        # a node that already has ANY project (including a different
        # one) is left untouched, so RF3's actual cross-project
        # collision protection is unaffected.
        def _tx(tx) -> None:
            stack = list(root_stack)
            while stack:
                parent, node = stack.pop()
                label = TYPE_TO_LABEL[node.node_type]
                tx.run(
                    f"OPTIONAL MATCH (legacy:{label} {{id: $id}}) "
                    "WHERE legacy.project IS NULL "
                    "SET legacy.project = $project "
                    "WITH legacy "
                    f"MERGE (n:{label} {{id: $id, project: $project}}) "
                    "SET n.name = $name, n.description = $description, "
                    "n.metadata_json = $metadata_json",
                    {
                        "id": node.id,
                        "name": node.name,
                        "description": node.description,
                        "metadata_json": json.dumps(node.metadata),
                        "project": effective_project,
                    },
                )
                if parent is not None:
                    parent_label = TYPE_TO_LABEL[parent.node_type]
                    tx.run(
                        f"MATCH (p:{parent_label} {{id: $pid, project: $project}}), "
                        f"(c:{label} {{id: $cid, project: $project}}) "
                        "MERGE (p)-[:HAS_CHILD]->(c)",
                        {"pid": parent.id, "cid": node.id, "project": effective_project},
                    )
                if node.node_type == "userstory":
                    task_names = node.metadata.get("task_names") or []
                    for task_name in task_names:
                        if not isinstance(task_name, str):
                            continue
                        tx.run(
                            "MATCH (u:UserStory {id: $uid, project: $project}), "
                            "(t:AtomicTask {name: $tname, project: $project}) "
                            "MERGE (u)-[:REALIZED_BY]->(t)",
                            {"uid": node.id, "tname": task_name, "project": effective_project},
                        )
                for child in node.children:
                    stack.append((node, child))

        with self._driver.session() as session:
            session.execute_write(_tx)
        return total

    # -- reads --------------------------------------------------------------

    def get_hierarchy_node(self, node_id: str) -> Optional[HierarchyNodeView]:
        """Return a single node by id (project-scoped), or None.

        Deliberately its own lightweight query, not routed through
        ``_project_graph`` — a single-node lookup shouldn't pay for a
        whole-project fetch."""
        rows = self._run(
            f"MATCH (n) WHERE n.id = $id "
            f"AND ({' OR '.join(f'n:{label}' for label in _HIERARCHY_LABELS)}) "
            f"AND (coalesce(n.project, '') = $project OR coalesce(n.project_id, '') = $project) "
            f"RETURN n, labels(n) AS lbls",
            {"id": node_id, "project": self._project},
        )
        if not rows:
            return None
        node_type = _type_of_labels(rows[0]["lbls"]) or ""
        return _view_from_props(node_type, dict(rows[0]["n"]), depth=0)

    def _project_graph(
        self, project: str,
    ) -> tuple[dict[str, tuple[dict, list[str]]], dict[str, str], dict[str, list[str]]]:
        """One bulk fetch of every hierarchy-labeled node + edge scoped to
        ``project`` (matched on EITHER the narrow schema's ``project``
        property or the wide schema's ``project_id`` — see
        core/graph/backend_entity.py vs core/graph/client.py), across both
        this module's original label set and the wider one added above.

        Returns ``(nodes_by_id, parent_of, children_of)``. ``parent_of``/
        ``children_of`` are normalized to one "parent owns child" direction
        regardless of which schema wrote the underlying edge — real data
        carries BOTH ``(parent)-[:HAS_CHILD]->(child)`` (this module's own
        convention, and extract_shim's narrow-schema dual-write) and
        ``(child)-[:CHILD_OF]->(parent)`` (extract_shim's wide-schema
        write); a single Cypher variable-length path can't mix edge
        directions per hop without APOC (this file stays APOC-free), so
        the walk happens in Python instead, once, after one bulk fetch —
        not per-node, and not per read call.
        """
        label_filter = " OR ".join(f"n:{label}" for label in _HIERARCHY_LABELS)
        node_rows = self._run(
            f"MATCH (n) WHERE ({label_filter}) "
            "AND (coalesce(n.project, '') = $project OR coalesce(n.project_id, '') = $project) "
            "RETURN n, labels(n) AS lbls",
            {"project": project},
        )
        nodes_by_id: dict[str, tuple[dict, list[str]]] = {}
        for r in node_rows:
            props = dict(r["n"])
            node_id = props.get("id")
            if not node_id:
                continue
            nodes_by_id[node_id] = (props, list(r["lbls"]))
        ids = list(nodes_by_id.keys())
        parent_of: dict[str, str] = {}
        children_of: dict[str, list[str]] = {}
        if ids:
            edge_rows = self._run(
                "MATCH (c)-[:CHILD_OF]->(p) WHERE c.id IN $ids AND p.id IN $ids "
                "RETURN c.id AS child, p.id AS parent "
                "UNION "
                f"MATCH (p)-[:{_HAS_CHILD_UNION}]->(c) WHERE p.id IN $ids AND c.id IN $ids "
                "RETURN c.id AS child, p.id AS parent",
                {"ids": ids},
            )
            for r in edge_rows:
                parent_of.setdefault(r["child"], r["parent"])
                children_of.setdefault(r["parent"], []).append(r["child"])
        return nodes_by_id, parent_of, children_of

    def get_children(
        self, node_id: str, depth: int = 0, type_filter: str = "", limit: int = 200
    ) -> HierarchySubtree:
        """BFS 'all children' traversal; see protocol for the contract."""
        nodes_by_id, _parent_of, children_of = self._project_graph(self._project)
        if node_id not in nodes_by_id:
            raise ValueError(f"unknown hierarchy node '{node_id}'")
        root_props, root_labels = nodes_by_id[node_id]
        root_view = _view_from_props(_type_of_labels(root_labels) or "", root_props, depth=0)

        max_hops = depth if depth > 0 else 100  # trees: 100 hops is unbounded
        children: list[HierarchyNodeView] = []
        truncated = False
        frontier = [node_id]
        seen = {node_id}
        hop = 0
        # BFS level-by-level so results stay ordered by depth, matching the
        # original Cypher's `ORDER BY depth, desc.name` contract.
        while frontier and hop < max_hops:
            hop += 1
            next_frontier: list[str] = []
            level_nodes: list[tuple[str, dict, list[str]]] = []
            for pid in frontier:
                for cid in children_of.get(pid, []):
                    if cid in seen:
                        continue
                    seen.add(cid)
                    next_frontier.append(cid)
                    if cid in nodes_by_id:
                        props, labels = nodes_by_id[cid]
                        level_nodes.append((cid, props, labels))
            level_nodes.sort(key=lambda t: t[1].get("name", ""))
            for _cid, props, labels in level_nodes:
                node_type = _type_of_labels(labels)
                if node_type is None:
                    continue
                if type_filter and node_type != type_filter:
                    continue
                if len(children) >= limit:
                    truncated = True
                    frontier = []  # stop expanding further levels once truncated
                    break
                children.append(_view_from_props(node_type, props, depth=hop))
            if truncated:
                break
            frontier = next_frontier
        return HierarchySubtree(root=root_view, children=children, truncated=truncated)

    def get_lineage(self, node_id: str) -> list[HierarchyNodeView]:
        """Return the ancestor path root-first, node itself last."""
        nodes_by_id, parent_of, _children_of = self._project_graph(self._project)
        if node_id not in nodes_by_id:
            return []
        chain: list[str] = [node_id]
        cur = node_id
        while cur in parent_of:
            cur = parent_of[cur]
            if cur in chain:  # defensive: never loop on a malformed graph
                break
            chain.append(cur)
        chain.reverse()  # root-first, node itself last
        lineage: list[HierarchyNodeView] = []
        for depth, nid in enumerate(chain):
            if nid not in nodes_by_id:
                continue
            props, labels = nodes_by_id[nid]
            node_type = _type_of_labels(labels)
            if node_type is None:
                continue
            lineage.append(_view_from_props(node_type, props, depth=depth))
        return lineage

    def get_project_tree(self, project: str) -> list[HierarchyNodeView]:
        """Every node scoped to ``project``, each annotated with
        ``parent_id``/``children_ids`` — see protocol for the contract."""
        nodes_by_id, parent_of, children_of = self._project_graph(project)
        tree: list[HierarchyNodeView] = []
        for node_id, (props, labels) in nodes_by_id.items():
            node_type = _type_of_labels(labels)
            if node_type is None:
                continue
            view = _view_from_props(node_type, props, depth=0)
            view.parent_id = parent_of.get(node_id)
            view.children_ids = list(children_of.get(node_id, []))
            tree.append(view)
        return tree
