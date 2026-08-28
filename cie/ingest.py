"""Push externally-produced AtomicTask-shaped entities into cie's own
task store (Neo4jTaskRepository) — the single store forge reads from
(cie.factory.get_task_repo(...).list_pending(), used by both the
mounted /tools/list_pending_tasks route and forge's CieBackend).

Any feature that produces task objects field-compatible with
cie.tasks.AtomicTask (see that module's docstring: name,
userstory_id, parent_feature, task_type, layer, action, file_path,
description, exact_imports, function_signatures,
step_by_step_implementation, dependencies, api_spec, test_triad) can push
them here instead of inventing its own task storage — this is what closes
the PRD-extraction -> forge loop: features/mine's story_2_tasks produces
DevTask/QATask objects shaped exactly like this and pushes them through
here.
"""

from __future__ import annotations

from typing import Any, Iterable

from cie import factory
from cie.task_repository import _task_from_node
from cie.tasks import AtomicQaTask, AtomicTask, AtomicTaskBatch

#: cie/task_repository.py::_VALID_LAYERS' canonical casing. mine's
#: DevTask/QATask.layer (features/mine/story_2_tasks/*/model.py) is a
#: plain unconstrained `str` — the LLM producing it has no reason to match
#: this exact Title-Case convention (confirmed live: it emitted
#: "frontend"), so every task from that producer was silently rejected at
#: the push gate ("has invalid layer 'frontend'") despite generation
#: itself succeeding. Normalized here, at the shared conversion boundary
#: every producer passes through, rather than in each producer — a case
#: genuinely outside this set still fails validation downstream with its
#: existing precise error, unchanged.
_LAYER_CANONICAL = {"backend": "Backend", "frontend": "Frontend", "api": "API", "data": "Data"}
#: Prefix fallbacks for producer drift the exact-match dict above misses
#: (confirmed live: an unconstrained LLM field emitted "frontend_state",
#: "frontend_ui", "test" in the same batch — see model.py/prompt.py fixes
#: in features/mine/story_2_tasks for the real fix; this stays as a
#: defense-in-depth net for whatever a prompt still doesn't fully pin down).
_LAYER_PREFIXES = (("frontend", "Frontend"), ("backend", "Backend"), ("api", "API"), ("data", "Data"))

#: cie/task_repository.py::_VALID_ACTIONS' canonical (lowercase)
#: casing — same failure mode/fix as _LAYER_CANONICAL above, for the
#: `action` field (confirmed live in the same batch: "Create", "Test",
#: "Setup"). Both TaskLayer and TaskAction are now real Enum fields on the
#: producer side (features/mine/story_2_tasks/model.py), which should
#: prevent this at the source going forward — this stays as a
#: defense-in-depth net for whatever other producer doesn't (yet) use
#: those enums. Unlike layer, there's no sensible prefix-fallback for a
#: genuinely unmapped action ("test" isn't a form of create/modify/delete)
#: — those still fail validation downstream with their existing precise
#: error, unchanged.
_ACTION_CANONICAL = {"create": "create", "modify": "modify", "delete": "delete"}


def _is_flattened_graph_node(data: dict) -> bool:
    """True for a raw Neo4j node property dict — nested fields (test_triad,
    api_spec, exact_imports, ...) are JSON-encoded into ``<field>_json``
    string properties by core/graph/repository.py::_flatten_props (Neo4j
    can't store nested objects), never under the plain field name. Any
    dict carrying such a key is that raw shape, not an already-flat
    AtomicTask-compatible dict."""
    return any(k.endswith("_json") for k in data)


def to_atomic_task(entity: Any) -> AtomicTask:
    """Re-validate any AtomicTask-shaped object (pydantic model or dict)
    into cie's own AtomicTask (or AtomicQaTask for task_type == 'qa').
    Extra fields the source model carries that the target model doesn't
    declare are dropped — pydantic's default extra="ignore" behavior (see
    e.g. `origin` and `dev_task_id` on AtomicTask, both declared there for
    exactly this reason).

    task_type == 'qa' validates into AtomicQaTask specifically (not plain
    AtomicTask) — confirmed live: features/mine/story_2_tasks/qa_tasks's
    QATask carries test_framework/run_command/test_type/test_origin/
    expected_assertions/requires_mocks/tested_file_path, none of which
    AtomicTask declares, so every QA task pushed through here used to lose
    all of them silently and come out shaped exactly like a dev task."""
    if isinstance(entity, AtomicTask):
        return entity
    if hasattr(entity, "model_dump"):
        data = entity.model_dump(mode="json", exclude={"parent_ids", "child_ids"})
    elif isinstance(entity, dict):
        if _is_flattened_graph_node(entity):
            # Raw Neo4j node dict (e.g. features/forge_repair/routes.py's
            # cached-story repush path, which reads already-saved DevTask/
            # QATask entities back via core.graph.repository.
            # list_entities_by_project) — go through the same node ->
            # AtomicTask reconstruction cie/task_repository.py's own reads
            # use (_task_from_node) so test_triad/api_spec/exact_imports/
            # etc. round-trip correctly, instead of `dict(entity)` leaving
            # every nested field at its pydantic default (None/[]) because
            # only the `<field>_json` key exists on the raw node. Confirmed
            # live 2026-08-13 on book-my-calender: a dev task's
            # test_triad_json held real content but validated to
            # test_triad=None here, tripping _task_rule_error's "missing a
            # test_triad" rejection and silently dropping the dev task
            # while its QA sibling (no equivalent gate) was written anyway.
            data = _task_from_node(entity).model_dump(mode="json", exclude={"parent_ids", "child_ids"})
        else:
            data = dict(entity)
    else:
        raise TypeError(f"cannot convert {type(entity)!r} to cie.tasks.AtomicTask")
    layer = data.get("layer")
    if isinstance(layer, str):
        normalized = layer.strip().lower()
        if normalized in _LAYER_CANONICAL:
            data["layer"] = _LAYER_CANONICAL[normalized]
        else:
            for prefix, canonical in _LAYER_PREFIXES:
                if normalized.startswith(prefix):
                    data["layer"] = canonical
                    break
    action = data.get("action")
    if isinstance(action, str):
        normalized_action = _ACTION_CANONICAL.get(action.strip().lower())
        if normalized_action:
            data["action"] = normalized_action
    target_model = AtomicQaTask if data.get("task_type") == "qa" else AtomicTask
    return target_model.model_validate(data)


def push_atomic_tasks(tasks: Iterable[Any], project: str = "") -> dict:
    """Convert + push a batch of AtomicTask-shaped entities via
    Neo4jTaskRepository.push_tasks — same store forge pulls pending tasks
    from. Returns the PushResult as a plain dict (accepted/rejected).

    ``project`` is stamped onto every written task node (see
    Neo4jTaskRepository.push_tasks/list_all_for_project) so a later
    project-scoped read doesn't pick up another project's tasks — it was
    previously only used to pick a (functionally-identical, shared-driver)
    cached repo instance, never actually written to the node."""
    batch = AtomicTaskBatch(tasks=[to_atomic_task(t) for t in tasks])
    result = factory.get_task_repo(project).push_tasks(batch, project=project)
    return result.model_dump(mode="json")
