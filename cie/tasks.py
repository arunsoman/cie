"""Domain models for the forge task pipeline.

Mirrors the planner's pydantic schema exactly (GraphBaseModel, AtomicTask,
ApiSpec, TestTriad, AtomicTaskBatch) so both sides validate against one
definition. Status/attempts are graph-only write-back properties (B1) and are
NOT part of the pydantic schema the planner emits.

Node shapes in the unified Neo4j graph:

    (:AtomicTask {id, name, userstory_id, parent_feature, task_type, layer,
                  action, file_path, description, exact_imports_json,
                  function_signatures_json, step_by_step_json,
                  status, attempts, created, updated})
    (:ApiSpec     {endpoint, request_schema, response_schema, error_codes_json})
    (:TestTriad   {positive, negative, negative_to_positive})
    (:Artifact    {path, kind, commit_sha})
    (:RepairEvent {round, failures_before, failures_after, mode, lint_ok,
                   timestamp, failures?, ts?})            # legacy fields optional

Edges:

    (:AtomicTask)-[:DEPENDS_ON]->(:AtomicTask)           # task-to-task deps (via file_path)
    (:AtomicTask)-[:DEPENDS_ON]->(:Node {kind:'file'})   # file-path deps into the code graph
    (:AtomicTask)-[:OWNS_API]->(:ApiSpec)                # one task → its contract
    (:AtomicTask)-[:OWNS_TRIAD]->(:TestTriad)            # one task → its triad
    (:AtomicTask)-[:PRODUCED]->(:Artifact)
    (:AtomicTask)-[:HAS_EVENT]->(:RepairEvent)
    (:AtomicTask)-[:PARENT_OF]->(:AtomicTask)            # from parent_ids/child_ids

``GraphBaseModel``/``bind_link`` below are cie's own standalone shim
(once a soft import of a host project's ``core.graph.base``, retained now
only as a cie-local base class) — this package installs and runs with no
host project on the path.
"""

from __future__ import annotations

import uuid
from enum import Enum
from typing import Dict, List, Literal, Optional

from pydantic import BaseModel, Field

#: Version of the atomic-task JSON Schema this package enforces at ingest.
#: Published alongside ``schemas/atomic_task.schema.json``; forge compares it
#: at startup and warns on drift.
SCHEMA_VERSION = "1.0.0"


class GraphBaseModel(BaseModel):
    """cie's own graph-node base class: an id plus a bidirectional
    parent/child id list, the shape AtomicTask nodes store in Neo4j."""

    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    parent_ids: List[str] = Field(default_factory=list)
    child_ids: List[str] = Field(default_factory=list)


def bind_link(parent: "GraphBaseModel", child: "GraphBaseModel") -> None:
    """Create a bi-directional link between a parent and a child object."""
    if parent.id not in child.parent_ids:
        child.parent_ids.append(parent.id)
    if child.id not in parent.child_ids:
        parent.child_ids.append(child.id)


class ApiSpec(BaseModel):
    """Contract between Frontend and Backend."""

    endpoint: str = Field(description="e.g., POST /api/v1/import/parse-csv")
    request_schema: str = Field(description="JSON schema or exact TypeScript/Pydantic interface of the request body")
    response_schema: str = Field(description="JSON schema or exact interface of the response body")
    error_codes: List[str] = Field(default_factory=list, description="e.g., ['400 Bad Request', '500 Parse Error']")


class TestTriad(BaseModel):
    """Ensures robust testing for every dev task."""

    positive: str = Field(description="Happy path test case.")
    negative: str = Field(description="Error path test case.")
    negative_to_positive: str = Field(description="Recovery/Fallback test case.")


class AtomicTask(GraphBaseModel):
    """One Task = One File. The only model passed to the code generator."""

    name: str
    userstory_id: str = ""
    parent_feature: str = ""
    task_type: str = Field(default="dev", description="'dev' or 'qa'")
    layer: str = Field(default="", description="'Backend' | 'Frontend' | 'API' | 'Data' | 'Program' (nirdosha lane)")
    action: str = Field(default="create", description="'create' | 'modify' | 'delete'")
    file_path: str = Field(description="Exact path from project root.")
    description: str = Field(default="", description="1-2 sentences exactly detailing what this file does.")
    exact_imports: List[str] = Field(default_factory=list, description="List of exact import statements needed.")
    function_signatures: List[str] = Field(default_factory=list, description="Exact signatures.")
    step_by_step_implementation: List[str] = Field(default_factory=list, description="Numbered list of exact logic steps.")
    dependencies: List[str] = Field(default_factory=list, description="Other file paths this task depends on")
    api_spec: Optional[ApiSpec] = Field(default=None, description="Required if layer is API.")
    test_triad: Optional[TestTriad] = Field(default=None, description="Required for 'dev' tasks.")
    #: Which Module bucket (workflow/feature/usecase) or "manual" the
    #: parent UserStory came from — carried through from
    #: features/mine/story_2_tasks so forge_repair's dashboard/detail
    #: endpoints can surface it. Declared here (not just on the source
    #: model) so ingest.py::to_atomic_task's re-validation doesn't
    #: silently drop it (pydantic's default extra="ignore" behavior).
    origin: str = ""
    #: This DevTask's own file-level Hoare-triple state contract (what
    #: must hold before/after THIS file's change, mapped to that file's
    #: specific inputs/outputs) — LLM-generated per task by
    #: AtomicTaskExtractionPrompt (dev_tasks/prompt.py Section 2 Rule 8 /
    #: Section 4.4), NOT inherited from the parent UserStory's coarser
    #: story-level conditions. Declared here for the same silent-drop
    #: reason as `origin` above (ingest.py's re-validation would otherwise
    #: drop them). QA generation reads these off the matching DevTask to
    #: build acceptance assertions on top of the per-file test_triad.
    preconditions: List[str] = Field(default_factory=list)
    postconditions: List[str] = Field(default_factory=list)
    #: For a 'qa' task, the parent DevTask's `.id` (GraphBaseModel's uuid,
    #: not `.name`) it verifies — see features/mine/story_2_tasks/
    #: qa_tasks/model.py::QATask.dev_task_id and qa_tasks/service.py's
    #: `dev_by_id = {t.id: t for t in dev_tasks}`. Empty for 'dev' tasks
    #: and for the shared conftest_task (verifies no single DevTask).
    #: Declared here for the same reason as `origin` above — otherwise
    #: ingest.py::to_atomic_task's re-validation silently drops it.
    dev_task_id: str = ""
    #: Compliance/capability tracking tag (see forge.models.AtomicTask's
    #: origin_requirement docstring) — declared here for the same
    #: silent-drop reason as `origin`/`dev_task_id` above. Optional (not a
    #: plain `str = ""`): forge/adapt.py's mined-task adapter explicitly
    #: passes `None` for tasks that don't fulfill a tracked obligation
    #: (`getattr(mined_task, "origin_requirement", None) or None`) — the
    #: prompt's own instructions call this "null on every task that
    #: doesn't fulfill one" (dev_tasks/prompt.py), so `None` has to
    #: validate here too.
    origin_requirement: Optional[str] = None
    #: RBAC (rbac-native-plan-v10.md, logic ported not shape) — tree-
    #: inherited effective permission token gating this task's story, if
    #: any; mechanically derived by features/mine/story_2_tasks (never
    #: LLM-prompted), carried through cie's task-store dict
    #: round-trip so it survives into forge's own AtomicTask (see
    #: forge/models.py). Declared here for the same silent-drop reason as
    #: `origin`/`dev_task_id`/`origin_requirement` above — added in
    #: core-boundaries-migration-plan.md Phase 6 when the forge/models.py
    #: and dev_tasks/model.py duplicates (both of which already had this
    #: field) were collapsed onto this canonical model, which had been
    #: missing it.
    required_permission: Optional[str] = None
    #: be-v2/docs/plans/nirdosha-default-pipeline-plan.md Phase 3 — for a
    #: pipeline_mode="nirdosha" project, the `module "name" { }` /
    #: `fn`/`struct`/`enum`/`screen` block name this task owns inside the
    #: project's single shared `.nir` entrypoint (`file_path` is that same
    #: shared path for every task in the project, not a unique-per-task
    #: path the way the classic lane uses it). None for every classic-lane
    #: task, and for nirdosha-lane tasks until features/mine/
    #: story_2_tasks/dev_tasks (Phase 4, not yet built) actually populates
    #: it. Declared here for the same silent-drop reason as `origin`/
    #: `dev_task_id`/`required_permission` above — cie/ingest.py::
    #: to_atomic_task's re-validation would otherwise drop it.
    target_construct: Optional[str] = None

# --- Enums for Test Execution ---
class TestType(str, Enum):
    UNIT = "unit"
    INTEGRATION = "integration"
    END_TO_END = "end_to_end"
    REGRESSION = "regression"
    REPRODUCTION_SCRIPT = "reproduction_script"

class TestOrigin(str, Enum):
    EXTRACTED_FROM_ISSUE = "extracted_from_issue"
    EXTRACTED_FROM_COMMENT = "extracted_from_comment"
    GENERATED_BY_LLM = "generated_by_llm"

class TestFramework(str, Enum):
    JUNIT = "JUnit"
    PYTEST = "pytest"
    JEST = "Jest"
    MOCHA = "Mocha"
    RSPEC = "RSpec"
    GO_TESTING = "go testing"
    CTEST = "ctest"
    #: Spec §5b — one Playwright spec per screen-bearing story, executed
    #: against a live dev server by features/build_app/run_playwright_tests.py
    #: as the workflow's _stage_qa (run_workflow.py). Added alongside the
    #: existing in-process frameworks so QA tasks can declare their real
    #: runner without falling back to CUSTOM_OR_UNKNOWN (which would lose
    #: the framework-specific run_command wiring build_app expects).
    PLAYWRIGHT = "Playwright"
    CUSTOM_OR_UNKNOWN = "custom_or_unknown"

class AtomicQaTask(AtomicTask):
    """A 'qa'-typed AtomicTask carrying test-execution metadata. Subclasses
    AtomicTask (rather than duplicating its fields) so every existing
    consumer that already accepts an AtomicTask — generate_file_agentic,
    isinstance checks, List[AtomicTask] fields — keeps working unchanged for
    qa tasks too; only the 6 fields below are genuinely new. Constructed by
    forge/adapt.py::adapt_task for task_type == 'qa', mirroring how QATask
    (features/mine/story_2_tasks/qa_tasks/model.py) already carries the same
    6 fields on the mining side.

    # --- MERGED TEST METADATA (Applied when task_type == 'qa') ---
    """

    test_framework: Optional[TestFramework] = Field(
        default=None, 
        description="Required if task_type is 'qa'. The testing framework (e.g., Pytest, JUnit)."
    )
    run_command: Optional[str] = Field(
        default=None, 
        description="Required if task_type is 'qa'. Exact CLI command to execute ONLY this test file. "
                    "e.g., 'pytest tests/unit/test_bug.py', 'mvn test -Dtest=BugTest'"
    )
    test_type: Optional[TestType] = Field(
        default=None,
        description="Required if task_type is 'qa'. e.g., unit, integration, e2e."
    )
    test_origin: Optional[TestOrigin] = Field(
        default=None,
        description="Required if task_type is 'qa'. Was this test extracted from the GitHub issue or generated?"
    )
    expected_assertions: List[str] = Field(
        default_factory=list,
        description="For 'qa' tasks: Human-readable list of assertions the test makes."
    )
    requires_mocks: bool = Field(
        default=False,
        description="For 'qa' tasks: True if the test relies on mocking external services or DBs."
    )
    #: The exact `file_path` of the DEVELOPMENT task this QA task tests —
    #: see features/mine/story_2_tasks/qa_tasks/model.py::QATask's own
    #: field of the same name (that's the field's origin; mirrored here so
    #: it survives cie/ingest.py::to_atomic_task's re-validation instead of
    #: being silently dropped like the other 6 fields above used to be).
    #: "" for the shared conftest/setup task, which verifies no single file.
    tested_file_path: str = ""

class AtomicTaskBatch(BaseModel):
    """Wrapper for the LLM to output a list of atomic tasks."""

    tasks: List[AtomicTask]

    # -- convenience helpers used by the forge pipeline (forge/repair.py,
    # forge/qa.py, forge/cli.py, features/build_app, features/forge_repair)
    # — ported here from forge/models.py's AtomicTaskBatch (core-boundaries
    # -migration-plan.md §A.6) so the canonical batch type is a drop-in
    # replacement for every existing call site, not just a schema match. --
    def by_path(self) -> Dict[str, AtomicTask]:
        return {t.file_path: t for t in self.tasks}

    def dev_tasks(self) -> List[AtomicTask]:
        return [t for t in self.tasks if t.task_type == "dev"]

    def qa_tasks(self) -> List[AtomicTask]:
        # `self.tasks` is genuinely List[AtomicTask] — a batch built via
        # forge/adapt.py::adapt_task will have real AtomicQaTask instances
        # (a subclass) for qa-typed entries, but that's not guaranteed for
        # every producer of this batch (e.g. a raw JSON round-trip through
        # .model_validate() without a discriminated union yields plain
        # AtomicTask). Callers wanting the 6 qa-specific fields should
        # isinstance(t, AtomicQaTask)/getattr rather than assume every
        # element here has them.
        return [t for t in self.tasks if t.task_type == "qa"]


# --- Graph-only write-back state (NOT part of the planner's pydantic schema) ---


class TaskStatus(str, Enum):
    """Lifecycle of an atomic task in the forge pipeline (graph-only property)."""

    PENDING = "pending"
    GENERATED = "generated"
    TESTED_GREEN = "tested_green"
    REPAIR_EXHAUSTED = "repair_exhausted"


class ArtifactKind(str, Enum):
    """What kind of file an artifact is."""

    SOURCE = "source"
    TEST = "test"
    SPEC = "spec"


class Artifact(BaseModel):
    """A file produced by a task, linked to a git commit (graph-only).

    content_ref/verdict (docs/forge-rebuild-plan.md's Phase 1 / WS9): set
    by `forge/reporter.py::CieReporter.record()` — content_ref is a
    short content hash (never the full file content), verdict is one of
    `forge/checkpoint.py::Verdict`'s values. Both default to "" so every
    pre-WS9 caller of `add_artifact` (which never set them) round-trips
    unchanged."""

    path: str
    kind: ArtifactKind = ArtifactKind.SOURCE
    commit_sha: str = ""
    content_ref: str = ""
    verdict: str = ""


class RepairEvent(BaseModel):
    """One row from forge's trajectory.jsonl, stored as a graph node (graph-only).

    The canonical shape is ``round``/``failures_before``/``failures_after``/
    ``mode``/``lint_ok``/``timestamp`` (integration spec T3.2). The legacy
    ``failures``/``ts`` fields are kept optional so older constructor calls
    keep working; they are stored verbatim when provided.
    """

    round: int = 0
    failures_before: int = 0
    failures_after: int = 0
    mode: str = ""
    lint_ok: bool = True
    timestamp: str = ""
    # Legacy back-compat fields (superseded by failures_before/after + timestamp).
    failures: Optional[int] = None
    ts: Optional[str] = None


class CycleResult(BaseModel):
    """Result of cycle detection over the task dependency graph."""

    has_cycle: bool
    cycle: List[str] = Field(default_factory=list)


class CoverageGap(BaseModel):
    """A dev task with no linked QA task and no produced test artifact."""

    task_name: str
    file_path: str
    parent_feature: str
    has_test_artifact: bool = False
    has_qa_task: bool = False


class ContractViolation(BaseModel):
    """A pair of tasks that share an API endpoint but disagree on the spec."""

    task_a: str
    task_b: str
    endpoint: str
    parent_feature: str
    reason: str = ""


# --- Partial-accept push contract (integration spec T3.1) --------------------


class RejectedTask(BaseModel):
    """One task that failed validation during ``push_tasks`` (nothing written).

    userstory_id/task_type/file_path are best-effort context (blank when
    not known/applicable) — added so a rejection can be durably persisted
    and later cross-referenced back to "which story/file was this for"
    without needing the original in-memory batch. Before this, a
    rejection's only record was a `print()` in features/mine/service.py's
    caller — confirmed live 2026-08-13: book-my-calender's `select_time_slot`
    dev task was silently dropped this way (missing test_triad, most
    likely) while its sibling QA task passed validation and got written,
    leaving an orphaned test with no implementation and zero durable trace
    of why. See `Neo4jTaskRepository.push_tasks`'s own docstring for where
    this now gets written."""

    name: str
    reason: str
    userstory_id: str = ""
    task_type: str = ""
    file_path: str = ""


class PushResult(BaseModel):
    """Outcome of ``push_tasks``: valid tasks written, invalid ones reported."""

    accepted: int
    rejected: List[RejectedTask] = Field(default_factory=list)


# --- PRD hierarchy (created by the external planner; stored + traversed here) -


#: Allowed hierarchy node types, mapped to Neo4j labels
#: module->Module, feature->Feature, workflow->Workflow,
#: usecase->UseCase, userstory->UserStory.
HierarchyType = Literal["module", "feature", "workflow", "usecase", "userstory"]


class HierarchyNode(BaseModel):
    """Recursive tree ingest shape pushed by the external planning system.

    ``metadata`` is the planner's extra payload, stored losslessly (JSON). For
    ``userstory`` nodes, ``metadata["task_names"]`` (list of task names) drives
    ``REALIZED_BY`` edges to stored ``:AtomicTask`` nodes.
    """

    node_type: HierarchyType
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    metadata: Dict = Field(default_factory=dict)
    children: List["HierarchyNode"] = Field(default_factory=list)


class HierarchyNodeView(BaseModel):
    """Read shape for a stored hierarchy node (flat, no children)."""

    node_type: str
    id: str
    name: str
    description: str
    metadata: Dict
    depth: int = 0
    #: Resolved from a live edge traversal at query time (see
    #: hierarchy.py::_project_graph), never a stored property — parent_id
    #: fields stored on the node itself are known-unreliable elsewhere in
    #: this codebase (go stale/null), so this module never trusts one.
    #: Populated by ``get_project_tree`` on every returned node, and by
    #: routes.py's node-detail/relations routes on the single node they
    #: fetch — left at these defaults (None/[]) by the plain
    #: ``get_hierarchy_node``/``get_children``/``get_lineage`` repository
    #: methods themselves, which don't compute them.
    parent_id: Optional[str] = None
    children_ids: List[str] = Field(default_factory=list)


class HierarchySubtree(BaseModel):
    """Result of the 'get all children' traversal API."""

    root: HierarchyNodeView
    children: List[HierarchyNodeView] = Field(default_factory=list)
    truncated: bool = False
