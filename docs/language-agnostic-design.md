# cie — language-agnostic boundary & `ProjectDescriptor` design

Status: design (react to this before implementing Phase 2). Phase 0 scrub
already applied to the code — see "Phase 0 — applied" below.

## 1. What cie is (the boundary this design preserves)

cie = **Code Insight Engine**. Its essential job is one thing:

> **Turn source into a queryable graph and answer structural/semantic
> questions about it.**

That is: extraction → store (Neo4j or embedded SQLite) → traverse/query →
serve to agents (MCP / HTTP / CLI). The on-demand analysis passes
(clone / perf / drift / communities / confidence / traceability / GraphRAG
Q&A) are all **read-side analysis over already-extracted structure** — they
do not build or run the project.

**cie is *not* a build system, a compiler, a test runner, or a coverage
tool.** Those are the *project's* concerns, owned by the project's own
toolchain. cie's value is being a thin, ignorant intelligence layer *over*
whatever toolchain already exists.

This is why the earlier "LanguageProvider with `compile`/`build`/`init`"
idea was rejected: it would make cie *own* the project's toolchain, which
is outside cie's essence and the source of the language-coupling we are
trying to remove.

## 2. Where "language" genuinely lives in cie — only two places

1. **Extraction** — parsing a file into nodes/edges. Already pluggable via
   `cie.lang_adapter.LanguageAdapter` (tree-sitter Python/JS/TS/Java +
   register-your-own / entry-point group). **This is the only place
   "language" is *code cie ships*.** Keep extending it.

2. **A declarative project descriptor** — for the execution-adjacent tools,
   cie should *invoke the project's own commands* and *parse declared
   formats*, never *own* them. This is config supplied by the project or
   the calling agent, run through the existing generic `run` oracle. cie
   stays ignorant of what those commands do — exactly as `run` already is.

**There is no `compile()`/`build()` in cie. No active provider.** The
"language construct" collapses from *a provider that builds* to *(an
extraction adapter) + (a declarative descriptor the agent/project fills
in)*.

## 3. `ProjectDescriptor` shape

A small, declarative dataclass (or Pydantic model) — **config, not code
cie ships per language**. Lives in a new `cie/project_descriptor.py`.
Loaded from a `cie.toml` / `cie.json` at the project root, or passed
explicitly by the calling agent / `CieConfig`.

```python
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional, Literal

@dataclass(frozen=True)
class ProjectDescriptor:
    # --- extraction (the one place "language" is real code) ---
    language: str = "auto"          # LanguageAdapter key, or "auto" = by suffix
    source_roots: tuple[Path, ...] = ()   # what to index; () = project_root
    ignore: tuple[str, ...] = ()    # fnmatch globs: dist/, target/, .venv, node_modules

    # --- execution-adjacent: cie INVOKES these, never owns them ---
    # All run through the existing jailed `run` oracle. None required.
    test_command: Optional[str] = None        # "pytest -q" | "cargo test" | "npm test"
    coverage_command: Optional[str] = None    # "pytest --cov --cov-report=json:cov.json"
    coverage_format: Optional[Literal[
        "coverage.py-json", "lcov", "cobertura-xml", "jacoco-xml", "gtest-json"
    ]] = None
    build_command: Optional[str] = None       # only if the project has a build step

    # --- capabilities: declare what this language/project supports ---
    # Gates the capability-gated tools (§4). Absent capability => tool no-ops
    # with a clear hint, never assumes. This is NOT "cie builds it" — it is
    # "cie knows whether to even attempt a language-specific heuristic."
    capabilities: dict = field(default_factory=dict)
    # examples:
    #   {"mocking": "python_patch"}            # testlink @patch heuristic applies
    #   {"contracts": "python_assert"}         # contracts.py assertion injection applies
    #   {"type_info": "mypy"}                  # type_flow can use real type info
    #   {}                                     # none of the above -> those tools no-op
```

**Semantics, stated plainly:**

- `language` + `source_roots` + `ignore` feed `extract` / `index` / `load`.
  This is the existing `LanguageAdapter` path — no new code, just wiring.
- `test_command` / `coverage_command` / `build_command` are **strings cie
  shells out via the existing `run` tool** (cwd-jailed, timeout-bounded).
  cie does not interpret them. `build_command` is optional and absent for
  interpreted languages — *no build step is a first-class case, not an
  error*.
- `coverage_format` tells cie **which parser** to use on the file the
  coverage command produced. cie ships one parser per format; adding a
  language's coverage is "add a parser + declare the format," not "teach
  cie to run coverage."
- `capabilities` is a **declaration**, not an implementation. It tells the
  capability-gated tools whether to attempt their language-specific
  heuristic at all. The heuristic itself still lives in cie (it is
  understanding-side), but it no longer *assumes* the language.

**What this deliberately does NOT add:**

- No `compile()` / `build()` *method cie implements*. `build_command` is a
  string the project supplies; cie just runs it.
- No per-language provider package with lifecycle code.
- No new file-jail surface beyond what `run` already has (`CIE_RUN_ROOT`).
  Provider-driven writes are not a concern because there is no provider —
  only the project's own commands, run through the existing jail.

## 4. Tool triage — what happens to each existing tool

Five dispositions. "KEEP" = essence, already/soon language-agnostic via
extraction adapter. "DESCRIPTOR-DRIVEN" = stop hardcoding Python, read the
command/format from `ProjectDescriptor`. "CAPABILITY-GATED" = language-
specific source heuristic, activate only when `capabilities` declares it,
else no-op with a hint. "CUT-TO-ADAPTER" = app-specific, not generic cie.
"PRODUCT-DECISION" = depends on whether cie keeps a task/hierarchy layer.

### KEEP (essence — understand + answer; extraction-adapter-driven)
Core graph nav: `search_symbol`, `resolve_import`, `semantic_search`,
`callers`, `callees`, `file_skeleton`, `path_between`, `failing_context`,
`affected_by`, `class_hierarchy`, `test_map`, `actual_callers`,
`dead_code_confirm`, `hybrid_search`, `entity_context`, `view_file`, `qa`
(GraphRAG), `blame_history`.
Section 13 read-side analysis: `clone_detect_run`/`clone_clusters`/
`clone_find`, `performance_profile`, `antipattern_scan`, `drift_*`,
`architecture_check`, `metrics`, `tech_debt_report`, `metric_trend`,
`community_*`, `accuracy_check`, `freshness_report`,
`comprehensiveness_report`, `salience_report`, `graph_diff`.
Section 0 sync (graph versioning): `sync_*`, `configure_layer_rules`,
`install_git_hook`, `get_layer_rules`.
Section 1 data model: `export_rdf` (⚠ ties to the `urn:protobox:`→`urn:cie:`
rename decision), `related_edges`, `validate_property_constraints`,
`type_flow*`, `dependency_graph*`, `doc_graph*`, `doc_search`.
Section 14 (understanding-side, language-agnostic parts): `test_skeletons*`,
`state_machine*`, `fsm_validate`, `traceability*`, `prd_traceability*`,
`semantic_diff`, `record_verdict`, `agent_verdicts`, `confidence_report`,
`justification`, `check_invariant`, `invariant_violations`,
`telemetry_to_spec`.
Section 15 (HTML decomposition, language-agnostic): `decompose_page`,
`page_tree`, `promote_hint_to_task`, `element_coverage`, `implied_pages*`.
Section 17: `subsystem_*`, `population_path`.
Mock server (generic HTTP): `mock_registry*`, `mock_coverage`,
`start/stop_mock_server`, `mock_violations`.
APM ingest (already-measured values, language-agnostic):
`record_apm_metric`, `apm_metrics`, `performance_baseline`,
`performance_regressions`.
Telemetry (OTLP/HTTP, language-agnostic): `POST /telemetry/otlp`.
Virtual filesystem & graph freshness: `write_file`, `write_files_atomic`,
`edit_file`, `delete_file`, `reindex`, `reindex_file`, `start/stop_watch`.
Generic execution oracle: `run` (already language-agnostic — keep as-is).

### DESCRIPTOR-DRIVEN (stop hardcoding Python; read from `ProjectDescriptor`)
- `run_tests`, `nook_and_corner_test`, `unified_coverage_report`,
  `test_results`, `record_test_result`, `coverage_gaps` → run
  `descriptor.test_command`; parse `descriptor.coverage_format`.
- `collect_apm_from_pytest` (in `cie/apm.py`) → generalize to
  "run the declared test command with its JUnit/xUnit reporter and parse
  the result," or cut to an adapter if no generic xUnit shape suffices.
- Coverage recording (`record_coverage` / `get_coverage` /
  `coverage_report` / `record_coverage_snapshot` / `coverage_trend`) →
  parse `descriptor.coverage_format` instead of assuming coverage.py JSON.

### CAPABILITY-GATED (language-specific source heuristic; no-op without the capability)
- `contracts` / `contracts_run` / `validate_types` / `inject_assertions` /
  `strip_assertions` (`python_assert` only) → active only when
  `capabilities["contracts"] == "python_assert"`.
- `testlink` `@patch`/`@mock.patch` resolution → active only when
  `capabilities["mocking"] == "python_patch"`.
- `type_flow` real-type path → active only when
  `capabilities["type_info"]` is set (else falls back to the structural
  inference it already does).

### CUT-TO-ADAPTER (app-specific — not a generic cie feature)
- `resolve_api_route`, `api_call_sites` (`cie/api_routes.py`) → hardwired
  to one repo's route decorators + `frontend/vite.config.js` proxy rules.
  Either (a) move to a pluggable `APIBoundaryResolver` adapter (same
  registry pattern as `LanguageAdapter`), or (b) cut from generic cie and
  let the host project ship its own resolver. **Do not keep a be-v2 route
  resolver in a language-agnostic tool.**

### PRODUCT-DECISION — RESOLVED (Phase 0.5c): keep as the team/Neo4j tier
- The whole task/hierarchy/coverage-write-back surface: `AtomicTask`,
  `hierarchy`, `list_pending_tasks`, `get_task`, `task_dependency_closure`,
  and the 17 HTTP-only tools (`push_tasks`, `set_task_status`,
  `link_artifact`, `append_repair_events`, `validate_*`, hierarchy
  push/get, coverage record/get/report/snapshot/trend).
- **Decision: keep it, as the Neo4j/team *retention* tier — not a
  zero-config feature.** The acquisition tier (embedded SQLite, the code
  graph + ~121 tools over MCP/HTTP/CLI) is what a solo dev / first-time
  visitor tries; the task/QA traceability + quality-governance layer is
  the "when your team is on Neo4j" upgrade that makes cie worth keeping
  installed past week one (cie's retention differentiator vs. pure code
  graphs).
- This resolves the 17-MCP-missing-tools question by positioning, not by
  code: those write-back tools stay HTTP/CLI-only (driven by a trusted
  in-process orchestrator), and the MCP surface stays the code-graph +
  filesystem + analysis surface an external coding agent needs. The
  forge-decoupling of this layer (rename `AgentType`, genericize
  `AtomicTask` field naming) is Phase 2 polish, not launch-blocking.
- `cie.api_routes`' user-facing tools (`resolve_api_route` /
  `api_call_sites`) were a separate, app-specific wart: cut from
  ToolService / HTTP / CLI in Phase 0.5a. The `cie.api_routes` *module*
  stays as an internal helper for `drift_detect` (CI-11) and
  `test_orchestration` (API-endpoint test plans); full adapter-ization
  (a pluggable `APIBoundaryResolver`, gating those passes behind a
  web-app capability) is Phase 2.

## 5. Phasing

- **Phase 0 — applied (see §6).** Safe mechanical scrub + deprecation
  aliases. Non-breaking.
- **Phase 1 — product decision + genericize.** Decide the task-layer
  question (§4 PRODUCT-DECISION). Either genericize `AtomicTask`/hierarchy
  or cut them. Either way cie stops being forge-shaped. Includes the two
  deliberate renames below (URN, AgentType) and the `api_routes` cut/adapter
  choice.
- **Phase 2 — `ProjectDescriptor`.** Add `cie/project_descriptor.py`,
  wire `extract`/`index`/`load` to `language`/`source_roots`/`ignore`,
  convert the DESCRIPTOR-DRIVEN tools to read commands/formats from it,
  add capability-gating to the CAPABILITY-GATED tools with graceful no-ops.
  Ship one coverage parser beyond coverage.py (lcov is the highest-value
  second format — covers C/C++/JS/Rust toolchains). No new jail surface.

## 6. Phase 0 — applied (this commit, in `/tmp/cie`)

Safe, mechanical, non-breaking. Deliberate decisions deliberately *not*
done here (see §7).

- **`cie/graph_cache.py`** — workspace path moved from
  `forge_workspaces/<p>/.forge/graph_cache.db` to a cie-owned
  `<workspace>/<p>/.cie/graph_cache.db`. `CIE_WORKSPACE_ROOT` is the new
  env var; `FORGE_WORKSPACE_ROOT` still read as a deprecated back-compat
  alias. Module docstring + `default_cache_db_path` updated.
- **`cie/tasks.py`** — dropped the soft `try: from core.graph.base import
  GraphBaseModel, bind_link` import that flipped the cie/be-v2 dependency
  direction. `GraphBaseModel`/`bind_link` are now cie's own standalone
  definitions (the shim that already existed as the fallback). Docstring
  updated. Package now installs/runs with no host project on the path.
- **`cie/mcp_server.py`** — added canonical policy names `full`
  (read+write) and `readonly` (read-only). `--policy` default changed
  `forge` → `full` (same permissions; renamed default). The historical
  names `forge`/`orchestrator`/`miner`/`inspector` kept as deprecated
  back-compat aliases (same permission level), so existing
  `--policy forge` scripts and the test suite (`POLICIES_BY_NAME["forge"]`,
  `resolve_policy("inspector")`) keep working.
- **`cie/tool_policy.py`** — module docstring rewritten generically
  (removed forge/be-v2 framing). `AgentType` enum and the `*_POLICY`
  constants left intact for now (see §7).

Verified: all four edited files parse; `POLICIES_BY_NAME` keys and
`resolve_policy` symbols referenced by `tests/test_mcp_server.py` are
preserved.

## 7. Deliberate decisions — status after Phase 0.5

(Phase 0 did the safe mechanical scrub; Phase 0.5 closed the two launch-
blocking identity warts and the product decision. What remains is Phase 2
polish, not launch-blocking.)

1. **`urn:protobox:` → `urn:cie:` URN scheme** — **DONE (Phase 0.5a).**
   `cie/data_model.py` now emits `urn:cie:` and the `cie:` RDF prefix.
   One-time identity migration (cie only emits URNs, nothing parses them
   back, so no read-side shim); external consumers keyed on the old form
   reindex. Guarded by `tests/test_tool_surface_invariants.py`.
2. **`AgentType` enum rename** (`FORGE`/`REQUIREMENT_MINER`/`INSPECTOR`/
   `ORCHESTRATOR` in `cie/tool_policy.py`). The enum *value strings*
   ("forge", "requirement_miner") appear in `ToolNotPermitted` error
   messages — user-visible. **Deferred to Phase 2** polish: the `--policy`
   *names* clients pass are already decoupled via the canonical
   `full`/`readonly` aliases + deprecated back-compat aliases shipped in
   Phase 0, so the enum is now internal identity only.
3. **`FORGE_PROJECT_ID` env alias** (`cie/cli.py`, `cie/routes.py`).
   Kept as a silent back-compat fallback (code still reads `CIE_PROJECT`
   first). Drop entirely only after forge consumers migrate to
   `CIE_PROJECT`.
4. **`cie/api_routes.py` user-facing tools** — **DONE (Phase 0.5a):** cut
   `resolve_api_route` / `api_call_sites` from ToolService, `routes.TOOLS`,
   and the CLI (`resolve-route` / `api-call-sites`). The module stays as
   an internal helper for `drift_detect` (CI-11) + `test_orchestration`
   (API-endpoint test plans). Full adapter-ization (a pluggable
   `APIBoundaryResolver` + capability-gating of those passes) is Phase 2.
   Guarded by `test_api_route_tools_are_not_on_the_generic_surface`.
5. **Cosmetic prose scrub** — ~100 remaining docstring/comment mentions of
  forge/mine/be-v2 across ~20 files (rationale prose like "pool sized to
  forge's AdaptiveConcurrencyLimiter ceiling 8"). Low value per edit,
  error-prone to hand-rewrite blindly. Track as a follow-up batch: rewrite
  each as generic rationale, don't just delete (the *why* is load-bearing).