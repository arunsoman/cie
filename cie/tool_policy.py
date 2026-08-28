"""Per-agent-type authorization over cie's LLM tool surface (ToolService).

Every `ToolService` method has always been uniformly callable by whatever
holds a reference to the instance — fine for a trusted in-process caller,
not fine once cie is handed to a less-trusted external caller (docs/plans/
cie-standalone-any-project-plan.md, Pillar A gap 2). This module adds a
thin policy layer OVER `ToolService` — it does not change `ToolService` or
any of its ~123 methods, and nothing calls into this module yet (adopting
it at a real call site — a trusted driver's tool dispatch, or an external
HTTP/MCP caller — is separate follow-up work, not part of this file).

`WRITE_TOOLS` below is a hand-curated classification — every method's
docstring (and body, where the docstring alone was ambiguous) was read
2026-08-28, not inferred from a naming heuristic alone, though it does
line up with this codebase's own `*_run` convention: a `*_run` tool
executes a pass and persists its results to the graph, while the
plain-named counterpart reads back what was persisted (e.g.
`clone_detect_run` — "Run the full clone-detection pass and write
fresh..." — vs. read-only `clone_clusters`/`clone_find`). Keep this set in
sync by hand when a new mutating method is added to `ToolService` — there
is no automatic way to derive "does this write" from a signature the way
`cie.tool_schema` derives types from one; `test_write_tools_are_real_tool_names`
in `tests/cie/test_tool_policy.py` only catches a stale/typo'd entry, not
a missing one.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import Optional


class AgentType(str, Enum):
    """Which kind of caller is holding a `ToolService` handle. Mirrors the
    idea from `src/new/cie`'s rejected facade design (see this plan's
    provenance note) trimmed to this codebase's actual callers today —
    FORGE/REQUIREMENT_MINER for the in-process pipeline, INSPECTOR for a
    read-only/external/less-trusted caller, ORCHESTRATOR for whatever
    drives the pipeline itself."""

    FORGE = "forge"
    REQUIREMENT_MINER = "miner"
    INSPECTOR = "inspector"
    ORCHESTRATOR = "orchestrator"


# Tools that mutate state outside their own return value: the graph
# (writes/persists nodes, edges, or properties), the filesystem, or an
# external process/service. Read-only queries over already-computed state
# are NOT in this set, even when their docstring says "compute" — only
# tools that store or change something are.
WRITE_TOOLS: frozenset[str] = frozenset({
    # filesystem
    "write_file", "write_files_atomic", "edit_file", "delete_file",
    # execution / process side effects
    "run", "run_tests", "start_watch", "stop_watch",
    "start_mock_server", "stop_mock_server", "install_git_hook",
    # graph-mutating "*_run" analysis passes (see module docstring)
    "clone_detect_run", "community_detect_run", "community_summarize_run",
    "contracts_run", "dependency_graph_run", "doc_graph_run",
    "drift_detect_run", "implied_pages_run", "mock_registry_run",
    "performance_analyze_run", "state_machine_run",
    "subsystem_dependency_graph_run", "test_skeletons_run", "type_flow_run",
    # explicit record/persist/promote/reindex operations
    "record_verdict", "record_apm_metric", "record_test_result",
    "promote_hint_to_task", "reindex", "reindex_file",
    "nook_and_corner_test", "configure_layer_rules", "performance_baseline",
    "decompose_page",
    # sync/versioning graph mutation
    "sync_promote", "sync_revert", "sync_evict_speculative",
    "sync_load_commit", "sync_quality_gate",
})


@dataclass(frozen=True)
class ToolPolicy:
    """What one caller may do against a `ToolService`.

    `allowed_tools=None` means "every tool this ToolService exposes"
    (still subject to `allow_write`); pass an explicit set to also
    restrict by name — e.g. an external caller scoped to read-only graph
    queries and nothing else.
    """

    agent_type: AgentType
    allow_write: bool = False
    allowed_tools: Optional[frozenset[str]] = None

    def permits(self, tool_name: str) -> bool:
        if self.allowed_tools is not None and tool_name not in self.allowed_tools:
            return False
        if tool_name in WRITE_TOOLS and not self.allow_write:
            return False
        return True


# Ready-made policies for this codebase's actual current callers — forge
# and requirement-miner already run in-process and trusted (see
# docs/plans/cie-forge-cre-migration-plan.md's CieClient), so these stay
# permissive; INSPECTOR is the one meant for a genuinely untrusted/
# external caller.
FORGE_POLICY = ToolPolicy(AgentType.FORGE, allow_write=True)
REQUIREMENT_MINER_POLICY = ToolPolicy(AgentType.REQUIREMENT_MINER, allow_write=False)
INSPECTOR_POLICY = ToolPolicy(AgentType.INSPECTOR, allow_write=False)
ORCHESTRATOR_POLICY = ToolPolicy(AgentType.ORCHESTRATOR, allow_write=True)


class ToolNotPermitted(PermissionError):
    def __init__(self, tool_name: str, policy: ToolPolicy):
        super().__init__(
            f"tool {tool_name!r} not permitted for agent_type="
            f"{policy.agent_type.value} (allow_write={policy.allow_write})"
        )
        self.tool_name = tool_name
        self.policy = policy


def authorize(policy: ToolPolicy, tool_name: str) -> None:
    """Raise `ToolNotPermitted` if `policy` does not permit `tool_name`."""
    if not policy.permits(tool_name):
        raise ToolNotPermitted(tool_name, policy)


def filter_tool_schemas(schemas: list[dict], policy: ToolPolicy) -> list[dict]:
    """The subset of `schemas` (`cie.tool_schema.tool_schemas()` output)
    `policy` permits — what to actually hand an LLM as its tool list, so
    it never sees, let alone calls, a tool it isn't authorized for."""
    return [s for s in schemas if policy.permits(s["name"])]


def execute(service: object, policy: ToolPolicy, tool_name: str, kwargs: dict) -> dict:
    """Policy-enforced dispatch: authorize, then call
    ``service.<tool_name>(**kwargs)``.

    The recommended call site for anything other than a fully trusted
    in-process caller — see docs/plans/cie-standalone-any-project-plan.md
    Pillar A gap 2. Not yet wired into any real call site (forge still
    calls `ToolService` methods directly); this is the mechanism, adopting
    it is separate follow-up work.
    """
    authorize(policy, tool_name)
    method = getattr(service, tool_name, None)
    if tool_name.startswith("_") or not callable(method):
        raise ToolNotPermitted(tool_name, policy)
    return method(**kwargs)
