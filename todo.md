# todo.md — executable plan for every roadmap item

> Created 2026-08-30. Companion to `roadmap.md` (what + why) and `goal.md`
> (process state). Every item below was planned **against the actual code
> on this commit** — files, line references, and ripple-effects are from
> a real read of the tree, not guesses. Survey inputs: `cie/routes.py`,
> `cie/tools/__init__.py`, `cie/mcp_server.py`, `cie/tool_policy.py`,
> `cie/cli.py`, `cie/factory.py`, `cie/embedded_repository.py`,
> `cie/hierarchy.py`, `cie/extract.py`, `cie/callgraph.py`,
> `cie/tools/heuristic.py`, `tests/*`, `tool-test-lab/*`,
> `.github/workflows/ci.yml`, README/CHANGELOG/docs.
>
> Baseline when planned: 171/171 tests; live conformance 126 tools —
> 85 verified / 19 graceful / 18 unavailable-by-design / 4
> backend-gated / 0 crashes; `ToolService` introspects to exactly **126
> public methods** (verified live via `vars(ToolService)`).

**Ground rules (inherited from roadmap, non-negotiable):**
1. DoD = *verified against the real environment*, never written-only.
2. Every item that changes a tool count or a behavior re-measures and
   updates every citable number **in the same pass** (the no-stale-docs
   rule) — see the Count Contract below.
3. P2 items stay gated on R6. No launch mechanics first.

---

## 0. Execution order and dependency graph

```
R4 (doc archaeology, S) ──────────┐
R3 (count sweep — see note) ──────┤
R5 (shrink unavailable, M) ───────┼──► R6 (cut 0.1.0 stable — the C1 gate)
R2 (CLI↔SQLite parity, M) ────────┤        │
R1 (MCP write-side parity, M) ────┘        │
                                           ▼
R14 (hierarchy→SQLite, M) ◄── chore: fix hierarchy.py stale docstring
     │ (do before/with R1's hierarchy tier, see below)
     ▼
P1 in trust order: R7 → R8 → R9 → R10 → R11 → R12 → R13
                                           │
P2 (gated on R6):            R15 → R16 → R17 → R18 → R19 → R20
```

Recommended working order within P0: **R4 → R2 → R1(+R14 seam) → R5 →
R3-final → R6**. Rationale: R4 is pure docs and removes a broken link
everything else cites; R2 is required for R3's "every cited number
matches pytest and the live surface" verify to even make sense on the
zero-config path; R1 grows the surface (count changes once, here); R5
shrinks the unavailable bucket (count changes again); R3's *final* sweep
runs last-before-R6 so the release ships with counts measured on the
release candidate commit, not a pre-R1 desktop.

**The Count Contract (cross-cutting, applies to R1/R2/R5/R6/R12/R13):**
the single source of truth for "how many tools" is introspection, never
prose: `len([m for m in vars(ToolService) if not m.startswith('_') and m
!= 'describe' and callable(...)])` (measured 126 today), plus the
surface-label convention from CHANGELOG L123–127 (126 ToolService /
read-only subsets per policy). Any PR that adds/removes/moves a tool
must, in the same diff: update README's "~N tools" + badge claims,
`docs/tool-selection-accuracy.md`, `docs/competitive-landscape.md`,
`docs/language-agnostic-design.md`, CHANGELOG (new dated entry — history
entries are never retro-edited), and re-run `tool-test-lab/
surface_conformance.py`, committing the fresh `surface_results.json` as
an artifact of record.

---

## Cross-cutting impact map (who touches what)

| Shared file/table | Touched by | Why it matters |
|---|---|---|
| `cie/routes.py` (`TOOLS`, `HTTP_WRITE_ALIASES`, `_authorize_http_tool`) | R1, R14, R16 | alias handlers → ToolService moves authorization path from alias-set to `WRITE_TOOLS` |
| `cie/tool_policy.py` (`WRITE_TOOLS`) | R1, R14, R16 | promoted write-alias handlers MUST land in `WRITE_TOOLS` or the read-only HTTP default silently allows writes — the one genuinely dangerous ripple in the whole plan |
| `tests/test_tool_surface_invariants.py` (`HTTP_ONLY_HELPERS`, policy tests) | R1, R14, R17 | these tests pin the three-front-end parity contract; they are the gate that must be *updated*, not dodged |
| `cie/cli.py` (`_open_engine`, `_open_task_repo`, `_open_hierarchy_repo`, `_open_tool_service`, group docstring "query a knowledge graph stored in Neo4j" L419) | R2, R8, R14, R15 | one backend-dispatch seam, built once in R2, reused by R8/R14/R15 |
| `cie/factory.py` (`build_tool_service_embedded`) | R1, R2, R14 | the single construction path embedded mode already funnels through |
| `cie/extract.py` (`_LANG_LOADERS`, `_FUNCTION_TYPES`, `_CALL_TYPES`, name/param/heritage dispatch) | R12, R13 | per-language tables + one new declarator-unwrap helper |
| `pyproject.toml` (deps, version, classifiers) | R6, R12, R13 | tree-sitter-c/cpp/c-sharp deps; 0.1.0 bump |
| `README.md`, `docs/*.md`, `CHANGELOG.md` | everything | Count Contract + per-item doc anchors listed inline below |
| `tool-test-lab/surface_conformance.py` + `surface_results.json` | R1, R5, R6, R17 | classification buckets + reason assertions + the hardcoded `PYTHONPATH` pin (​L107) must be parameterized before CI can run it |
| `.github/workflows/ci.yml` | R6, R17 | conformance job + release job |
| `scripts/record_demo*.sh` pattern | R8, R15, R20 | record-then-commit, not one-off screenshots |

---

## P0 — before 0.1.0 stable can honestly cut

### [x] R1 · Embedded MCP write-side parity (M) — DONE 2026-08-30

*(Implementation summary: six `cie.routes._tool_*` handlers ported
verbatim into ToolService methods; routes dispatch via
`_service_tool`; `WRITE_TOOLS` +6 in the same commit with the overlap
pinned by `test_http_write_aliases_and_write_tools_never_overlap`;
`HTTP_ONLY_HELPERS` −6; policy 403-tests per promoted name; 19-test
`tests/test_task_qa_write_parity.py` on the embedded backend incl.
SQLite on-disk verification; live MCP stdio probe executed all six;
surface 126→132, conformance re-run committed.
`push_hierarchy` + read-side hierarchy helpers wait for R14. Full
detail in the commit and CHANGELOG [Unreleased].)*

**State today (verified).** `cie/routes.py` implements 7 task/QA write
tools as bespoke `_tool_*` HTTP handlers — `_tool_push_tasks` (L374),
`_tool_set_task_status` (L399), `_tool_link_artifact` (L431),
`_tool_append_repair_events` (L463), `_tool_record_coverage` (L519),
`_tool_record_coverage_snapshot` (L619), `_tool_push_hierarchy` (L691) —
registered in the `HTTP_WRITE_ALIASES` frozenset (L145) and authorized
via the alias-set branch of `_authorize_http_tool` (L167). The MCP server
(`cie/mcp_server.py::build_mcp_server`) only introspects `ToolService`
public methods, so these 7 never appear on the default `cie-mcp
--embedded` install — the headline task/QA layer is read-only there.
`tests/test_tool_surface_invariants.py::HTTP_ONLY_HELPERS` currently
documents this split as *allowed* (17 names). Embedded side already has
the backends these tools need: `EmbeddedTaskRepository` (task write
side) and `EmbeddedRepository`/`InMemoryRepository.record_coverage*`
(L1207–1312). The only seam missing for embedded is the hierarchy repo
— see R14.

**Plan.**
1. **Promote, do not re-implement.** Move each `_tool_*` handler's body
   (validation + partial-accept semantics + hint strings) into a real
   `ToolService` method with the **same kwargs and same envelope
   shape**, one commit per tool family: (a) `push_tasks`;
   (b) `set_task_status`, `link_artifact`, `append_repair_events`;
   (c) `record_coverage`, `record_coverage_snapshot`. In
   `cie/routes.py::TOOLS`, replace each bespoke entry with
   `_service_tool("<name>")` so the HTTP contract (URL, kwargs, envelope,
   "Field required" retry hints the conformance harness relies on) is
   bit-identical. Methods resolve their backends through
   `self._task_repo` / `self._engine` exactly the way the handlers use
   `factory.get_task_repo(project)` / `factory.get_engine(project)` —
   no new wiring.
2. **Authoritative trap — do this before anything else can ship:** add
   all 6 task/QA write names above to `cie/tool_policy.py::WRITE_TOOLS`.
   The moment they become ToolService methods, routes' `_authorize_http_tool`
   will classify them via `authorize()`/`WRITE_TOOLS`, NOT
   `HTTP_WRITE_ALIASES` — if they're not in `WRITE_TOOLS`, the default
   read-only HTTP policy (INSPECTOR) would silently start permitting
   writes. Then shrink `HTTP_WRITE_ALIASES` to empty (or only
   `push_hierarchy` pending R14) and add an invariants test pinning:
   `HTTP_WRITE_ALIASES ∩ WRITE_TOOLS == ∅` **and** every name removed
   from the alias set is present in `WRITE_TOOLS` — so the trap can't
   recur next time a handler is promoted.
3. `push_hierarchy` (+ read-side `get_children`, `get_lineage`) needs a
   hierarchy backend on embedded — promote these **in R14's PR**, not
   here; R1's verify scope is the task/QA surface that stands on
   `tasks.db` alone.
4. After promotion, update `tests/test_tool_surface_invariants.py`:
   `HTTP_ONLY_HELPERS` shrinks by 6 (the remaining honest set becomes
   `health`, `schema_version`, `validate_api_contracts`,
   `validate_coverage`, `validate_cycles`, `get_coverage`,
   `coverage_report`, `coverage_trend` — all read-side or HTTP-surface
   helpers); re-run the whole file and *read the failure list before
   editing* — that list IS the parity contract.
5. Re-run and commit fresh artifacts: `tool-test-lab/
   surface_conformance.py` (new surface = 126 + 6 = **132** tools;
   expect assigned `verified`/`graceful` for the promoted writes against
   `.cie/tasks.db`) and `tool-test-lab/dogfood_mcp.py` (its probe list
   should now find push-side names).

**Impact / ripples elsewhere.**
- `cie/tool_policy.py` docstring says its classification was read
  2026-08-28 and "keep this set in sync by hand" — amend that comment
  and the `HTTP_WRITE_ALIASES` comment block on routes L139–146, which
  will otherwise lie after promotion.
- Count Contract: README "~126", `docs/tool-selection-accuracy.md` and
  `docs/competitive-landscape.md` numbers → measured 132; note
  `docs/tool-selection-accuracy.md`'s 14/14 claim was gathered on a
  *different* surface — re-run its harness or re-label the claim's
  surface as the 08-30 one (decide in PR).
- `CHANGELOG.md`: new Added/Fixed entry; historical entries untouched.
- No `mcp_server.py` code change (introspection does the right thing
  automatically) — that's the point of the fix — but its module
  docstring's "~121 methods" comment needs the new number.
- `http_policy` tests (`tests/test_http_policy.py`): write-refusal cases
  for the 6 promoted names must still 403 under default policy — add
  explicitly, one per tool.

**Verify (from roadmap, made concrete).** `python tool-test-lab/
dogfood_mcp.py <indexed-root>` sees and executes `push_tasks`,
`set_task_status`, `link_artifact`, `append_repair_events`,
`record_coverage*` against `<root>/.cie/tasks.db`; invariants tests
green with the *smaller* `HTTP_ONLY_HELPERS`; conformance JSON has 0
crashes. Done ≠ merged; done = fresh `surface_results.json` committed.

---

### [x] R2 · CLI↔SQLite parity for the quickstart (M) — DONE 2026-08-30

*(Implementation summary: one backend-selection seam in cie/cli.py —
`--backend`/`--db`/`CIE_BACKEND`/`CIE_DB`/auto-probe; embedded branches
in `_open_engine`/`_open_task_repo`/`_open_hierarchy_repo`/
`_open_tool_service`; honest `unavailable`/`not_found` envelopes for
hierarchy-on-embedded, Neo4j-only ingest commands, and explicitly
missing dbs; group-level fail-fast probe; 13-test `tests/test_cli.py`
+ clean-venv verify vs psf/requests, callers("close")=3 matching the
published benchmark. Details in the commit message and CHANGELOG
[Unreleased].)*

**State today (verified).** `cie index` writes the embedded SQLite graph
(`cie/cli.py` L438–488 via `cie.embedded_repository.EmbeddedRepository`),
but every query command routes through `_open_engine()` (L70), which
unconditionally builds `Neo4jRepository.connect(...)` — the session-7
log's four retries against `localhost:7687` are exactly this line
failing. The same hardwiring exists in `_open_task_repo()` (L112) for
`tasks:*` and `_open_hierarchy_repo()` (L134) for `hierarchy:*` (that
one blocked on R14), and `_open_tool_service()` (L145) composes them.
The good news, verified: `InMemoryRepository` (embedded's engine)
implements the *full* Repository protocol surface the CLI reads —
`get_community`/`list_communities`/`god_nodes`/`stats`/`shortest_path`/
`discover_features`/`list_files`/`failing_context`/`affected_by`/
`hybrid_search`/`semantic_search` all exist (L185–1312) — so this is a
construction-path fix, not a query-engine port. The trap to respect:
community *summary labels* and embeddings come from `core.llm`
(`cie/community_detect.py`, `cie/embed.py`) which won't exist embedded —
those commands must degrade with an envelope-honest hint, not pretend.

**Plan.**
1. **One dispatch seam, built once:** add `_open_backend(project)`
   returning `("embedded", db_path)` or `("neo4j", cfg)`:
   resolution order = `--db PATH` on the `cli` group → `CIE_DB` env →
   `.cie/graph.db` in the *project root being queried* → Neo4j env
   (`CIE_NEO4J_URI` etc.). Rewrite `_open_engine`,
   `_open_task_repo`, `_open_tool_service` to switch on that seam,
   calling `build_tool_service_embedded()` / `EmbeddedRepository` /
   `EmbeddedTaskRepository` on the embedded branch. Neo4j stays the
   explicit choice when URI env is set; no behavior change for existing
   Neo4j users (their env vars still win).
2. Every `_open_engine` call site (~25 commands) keeps working unchanged
   — that's the seam's job. Audit exceptions: `bootstrap` (explicitly
   Neo4j index-creation — becomes an error-with-hint on embedded, not a
   silent no-op), `load`/`watch`/`serve` (all remain honest about which
   backend they target), `schema:dump`/`schema-version`/`health`/
   `stats` (must answer on embedded).
3. Fix the lies in prose while there: group docstring L419 ("query a
   knowledge graph stored in Neo4j") and each help string that says
   Neo4j-only.
4. Fill the honest answers: commands whose underlying data cannot exist
   embedded (embeddings → `semantic-search`; LLM community labels →
   `community`) must return the standard error envelope with a
   machine-readable hint naming the missing optional backend — reuse
   the R5 reason-string convention below so R2 and R5 ship one
   convention, not two.
5. Tests: there is **no `tests/test_cli.py` today** (verified) — R2 adds
   one: CliRunner-based, monkeypatching nothing, running `index` → each
   documented query command against a tmp project; plus a
   `test_standalone_smoke.py`-style clean-venv check for the exact
   quickstart sequence.

**Impact / ripples elsewhere.**
- `README.md` Quickstart gains 2 lines ("query it from the CLI: `cie
  files`, `cie path` … — same `.cie/graph.db`"); `docs/language-
  agnostic-design.md` backend-matrix section updated.
- `docs/competitive-landscape.md` honest-gaps: "CLI requires Neo4j" is a
  known caveat — update the entry in the same PR (no-stale-docs rule).
- `_close_engine`/`_close_driver` teardown paths must handle the
  embedded branch (SQLite conn has no driver; `EmbeddedRepository` wants
  an explicit `close`/flush — it flushes per-call today, fine).
- R8 and R14 depend on this seam; R15's `cie init` command schemas too.

**Verify (from roadmap).** Fresh venv: `pip install -e .` → `cie index
.` → `cie files .`, `cie search-symbol`, `cie path`, `cie callers`,
`cie callees`, `cie skeleton`, `cie view-file`, `cie stats`, `cie
communities` each answer against `.cie/graph.db` (or return an honest
envelope-error naming the missing optional backend — no bolt traffic at
all, proven by `--db` pointing into an empty dir and zero sockets).

---

### [x] R3 · Stale-surface sync, repo-wide (S) — DONE 2026-08-30

*(Done as the post-R1/R2/R5 final counts pass: badge 155→213; live
citations re-measured (132/83, introspection + conformance artifact);
dated snapshots labeled per the convention; durable tool-count-label
block added to README; competitive-landscape and language-agnostic-
design counts updated; CLI command count corrected 49→47, verified by
walking the real click tree; grep sweep clean — remaining "81" mentions
are explicitly dated snapshot labels; dated CHANGELOG/competitive-delta
records stay frozen per convention.)*

**State today (verified).** Live stale spots: README L10 badge
`tests-155 passing`; README L66 `81-tool surface` hook paragraph;
`docs/competitive-landscape.md` L85 (81-tool); `docs/tool-selection-
accuracy.md` L19 + L56 (81 tools); `docs/language-agnostic-design.md`
L194 (~121). CHANGELOG L123–127 already states the correct convention
(126 MCP tools = 127 public methods − `describe`; 83 read-only under
`inspector`). Historical CHANGELOG entries (L115, L243, L270, L320) are
*record*, not claims — do not retro-edit them.

**Plan.** Sections follow. This is deliberately the LAST P0 item before
R6, after R1/R5 have moved the numbers, and it re-measures everything
from the live code:
1. Measure once, cite everywhere: test count from `pytest -q` (171 as
   of planning, re-run); surface counts from introspection + a fresh
   `surface_conformance.py` JSON; policy subsets from
   `count: 126 − len(WRITE_TOOLS ∩ surface)` per policy.
2. Apply the **surface-label convention** (CHANGELOG's) as a short
   labeled table pinned in README next to the tools section, and make
   every doc citation use a label ("N ToolService tools / M over MCP
   under `full` / K read-only under `inspector`") instead of a bare
   number, so the drift class dies structurally, not just this once.
3. Update the README badge to a number, and — to kill the drift
   permanently — note in `README` that the badge is re-cut each release
   (R6 checklist step), not CI-live.

**Impact / ripples.** None code-wise. Everything count-citing elsewhere
(roadmap.md header baseline, goal.md log lines) is historical record and
stays. R6's verify re-asserts all of it on the release commit.

**Verify (from roadmap).** `grep -rn "155 passing\|81-tool\|~121"`
returns only historical/change-record contexts (dated CHANGELOG entries,
session logs); every live citation matches that commit's `pytest -q`
count and `tool-test-lab/surface_results.json`.

---

### [x] R4 · Reconstruct the provenance of the plan docs (S) — DONE 2026-08-30

**State today (verified).** `docs/growth-plan.md` does not exist and has
**no git history** (checked `git log --all --diff-filter=A`).
`goal.md` referenced it at 11 places and `roadmap.md` L8 cites it too.
Conclusion: **fold, don't restore** — there's nothing to restore from
history. *DONE 2026-08-30:* tombstone written; goal.md gained a
Provenance section; every reference rewritten with folded language.
(Plan body below retained for the record.)

**Plan.**
1. Deep-read the remaining live content goal.md actually inherited (its
   workstreams A–F already carry the substance). Anything growth-plan
   had that goal.md doesn't (the "Open question" section C1 cites at
   L61) gets lifted verbatim into a `## Appendix — folded from
   docs/growth-plan.md` section of goal.md, quoted and labeled.
2. Rewrite each reference from "see docs/growth-plan.md Phase 0.5" to
   "folded into this doc's <section> (formerly docs/growth-plan.md
   Phase 0.5)". Add a one-line provenance note at goal.md's top.
3. Add a permanent tombstone `docs/growth-plan.md` stub (5 lines:
   "folded into ../goal.md on 2026-08-30; do not recreate; history has
   no copy") so external links/deep links from old sessions land
   somewhere honest instead of 404.

**Impact / ripples.** `roadmap.md` L8's provenance line gets a pointer
to the appendix. `README.md`/docs do not reference growth-plan
(verified grep) — no other links to repoint.

**Verify (from roadmap).** `grep -rn "growth-plan"` → only
folded/restored-convention text; the tombstone exists; every goal.md
internal link resolves.

---

### [x] R5 · Shrink the unavailable-by-design surface (M) — DONE 2026-08-30

*(Implementation summary: lazy `core.llm` in 4 modules → 13 tools
un-503'd; `err` envelope gains optional `error.reason` slug; `_guard`
classifies ModuleNotFoundError/DetectorUnavailable into
`OPTIONAL_BACKEND_MISSING:*` / `HOST_PLUGIN_MISSING:*` slugs; registry
pinned in `tests/test_unavailable_reasons.py` incl. a regrowth gate;
conformance harness carries reason into `surface_results.json` — live
run: 132 tools / 100 verified / 23 graceful / 5 unavailable. Real crash
found+fixed en route: `failing_context("")` IsADirectoryError via the
heuristic fallback.)*

**State today (verified).** The 18-tool unavailable bucket decomposes,
by module, to exactly six root causes — all module-level imports of the
host-project `core.llm` (or plugin contract) executed when a tool lazily
imports its feature module: `cie/community_detect.py` L45 (→
`community_detect_run`, `community_search`, `community_summarize_run`),
`cie/contracts.py` L30 (→ `contracts`, `contracts_run`, `validate_types`
— shares the module), `cie/graphrag.py` L32 (→ `qa`),
`cie/state_machine.py` L20 (→ `state_machine_run`, `fsm_validate`),
`cie/test_orchestration.py` L40 (→ `run_tests`, `test_plan`,
`test_results`, `coverage_gaps`, `unified_coverage_report`,
`nook_and_corner_test`, `inject_assertions`, `strip_assertions`),
plus `decompose_page` (`DetectorUnavailable` plugin contract). The 4
backend-gated tools (`implied_pages_run`, `prd_traceability_*`) are a
different bucket: their engine methods have no InMemoryRepository
implementation yet (stretch: porting them to embedded, InMemoryRepository
already has `record_coverage` family at L1207+, collapses them into
verified — good R5 side-quest, not the gate).
`ToolService._guard` (L~245) already maps `ModuleNotFoundError` →
`kind=unavailable` with a prose hint; conformance classifies on
`kind=unavailable` (surface_conformance.py `classify`, L82–84).

**Plan.** Take option (a) *decouple*, not option (b) plugin-surface, for
these six — they're data/analysis routines whose LLM use is one call
inside a pipeline, and the plugin surface is already the decompose
pattern. Concretely, per module:
1. **Defer imports, gate capabilities.** Module-level `from core.llm
   import ...` moves into the single function that actually calls the
   agent; the module's *pure* layers (community graph algorithms,
   contract parsing, FSM text parsing, test-plan heuristics) run and
   return real results with `llm_summary: null` + a reason instead of
   the whole tool 503-ing. Each ToolService method then returns
   `ok=true` with a results-shaped "degraded" payload — verified/graceful
   in conformance — or, for tools that are *only* an LLM call
   (`qa`), a crisp `unavailable` with a machine-readable reason.
2. **Machine-readable reasons.** Add `reason` to the error envelope:
   `_err` grows an optional `reason: str` slug next to
   `kind`/`message` (SPEC §0 change — bump `envelope.py`'s version
   note). Slugs: `OPTIONAL_BACKEND_MISSING:core.llm`,
   `HOST_PLUGIN_MISSING:decompose`, `NEO4J_REQUIRED`, etc. Pick the
   slug per call site; `_guard` fills it automatically from the
   exception context.
3. **Pinned registry.** New `tests/test_unavailable_reasons.py`:
   a hand-curated dict `{tool_name → reason_slug}` asserted per tool
   against a live embedded ToolService — the "each with its reason
   asserted in the harness" requirement, in pytest form, so
   `surface_conformance.py` (which should also carry the reason through
   to `surface_results.json`) can diff against it.
4. Re-run conformance → commit new `surface_results.json`; expected
   end-state: unavailable ≤ 6, and each remaining one's `reason` field
   is the asserted contract.

**Impact / ripples elsewhere.**
- `ToolService._guard` + `cie/envelope.py`: error envelope shape grows a
  field — check consumers: routes `_service_tool` passthrough (fine),
  MCP server content serialization (dict passthrough, fine),
  conformance `classify()` gets a `reason` extraction line.
- `tests/test_optional_dependency_envelope.py` — extend with reason
  expectations; it's the pre-existing test of this navigation.
- Count Contract: 88→~95 verified/graceful shifts every surface citation
  (README, docs) *again* — R3-final runs after this.
- `docs/competitive-landscape.md` "unavailable-by-design" honesty
  footnote updates in the same PR.
- Careful scope line: do NOT start re-implementing `core.llm` (an LLM
  client abstraction) inside cie here — that's a product decision
  (candidate for the P1 semantic work in R10), keep R5 to
  degrade-honestly + reason-strings.

**Verify (from roadmap).** Re-run `tool-test-lab/surface_conformance.py`:
unavailable 18 → ≤6; every unavailable row carries `reason` and the
registry test pins it; README's "~N tools" carries the honest footnote
naming what remains 503 and why.

---

### [x] R6 · Cut 0.1.0 stable — the C1 gate (S once R1–R5 close) — DONE 2026-08-31 (GitHub release live; PyPI upload pending a distribution-name decision)

*(Implementation: release commit `a22b4bf` — version house 0.1.0a3 →
0.1.0, classifier → Beta only, README badges re-cut (status-beta,
tests-279), CHANGELOG [Unreleased] → [0.1.0] with a fresh Unreleased
stub; suite 279/279; `uv build` → sdist+wheel, twine check PASSED, entry
points verified; clean-venv rehearsal from the wheel on a fresh
psf/requests clone @ 5460f467 — index 858/1822, `cie callers close` = 3,
real MCP stdio handshake from an independent client: 85 read-only tools,
callers() executes with resolution gap visible. Tag `v0.1.0` pushed,
GH release live with the caveats block kept; C1 hand-closed in goal.md
with the tag id. **Open follow-up:** PyPI upload under `cie` is
impossible — pypi `cie` is cluster311/cie10 (ICD-10 codes, latest
0.208); needs a rename-or-GitHub-only decision, README + release notes
already state it with a date.)*

**State today (verified).** `pyproject.toml` version `0.1.0a3`,
classifier `Development Status :: 3 - Alpha`; CHANGELOG's latest dated
section is `[0.1.0a3] - 2026-08-30`; repo has 10 commits; `goal.md` C1
open and explicitly "cannot be closed by editing".

**Plan (the release itself is a checklist run, not a coding task).**
1. **Gate check:** R1–R5 checkboxes verified `[x]` with committed
   artifacts; suite green on the release commit; conformance green with
   0 crashes; every count in README/docs re-measured same-commit (this
   is R3's final sweep data).
2. Version house: `0.1.0a3` → `0.1.0`; classifiers: Alpha → Beta is the
   *predatory-reading* risk — a 0.1.0 stable at first-repo-maturity
   still ships caveats, so classifier moves to `4 - Beta` **only**.
   Status badge in README L36 (`status-alpha`) moves to beta in the
   same diff; `pyproject.toml` classifiers + version; CHANGELOG `## 
   [0.1.0] - <date>` section assembled from the unreleased P0 entries
   (Keep-a-Changelog move, not rewrite).
3. Release notes on GH: rendered from the CHANGELOG section **plus**
   the alpha-era caveats block (contributor count, single-author,
   benchmark scope honesty); KEEP the honest-loss links
   (`docs/benchmarks-requests.md` 3-of-6 line) in the notes body.
4. `python -m build` → verify sdist/wheel contain `cie/` and entry
   points (`cie`, `cie-mcp`); upload to PyPI **after** the clean-venv
   rehearsal passes.
5. Clean-venv rehearsal, third-party repo (the roadmap's own bar):
   fresh `python -m venv`, `pip install "cie[mcp]"` from PyPI, `cie
   index <third-party-repo>`, `cie-mcp <repo> --embedded` handshake via
   a real MCP client, one query command via CLI, then (R18's listings
   only *after* the artifact is live).
6. Hand-close C1 in `goal.md` **by hand** with the tag id, and cut tag
   `v0.1.0` + GH release. Only now does P2 ungate.

**Impact / ripples everywhere.** This is the step the whole tree was
prepped for: post-release, README "status" prose, `docs/competitive-
landscape.md` "alpha caveats", and `goal.md` C1 all flip in one
documented pass.

**Verify (from roadmap).** Clean-venv `pip install cie[mcp]` quickstart
against a third-party repo end-to-end; tag + release live; C1 closed by
hand in goal.md.

---

## P1 — differentiator defense + credibility (after stable)

### [x] R7 · Edge provenance tagging (L) — DONE 2026-08-30

*(Implementation summary: `callgraph.resolve_call_edges` computes per-name
total/unresolved call-site tallies in the same pass (`resolution_stats`);
both loaders persist them as `CallResolutionStat` analysis nodes;
ToolService `callers`/`callees` shapes per-row `provenance`
(graph|heuristic-name-match, including on fallback legs) + envelope
`resolution`; 4 ground-truth tests (`tests/test_edge_provenance.py`);
live psf/requests check matches the published 3-resolved with the gap
exposed (`19/16/3`).)*

**State today (verified).** The *graph* half exists: `cie/callgraph.py`
already stamps every `calls`/`extends`/`implements` edge with
`Confidence` — EXTRACTED (same-file/import-resolved, L323–333),
INFERRED (receiver heuristic, L255–278), AMBIGUOUS (L344) — and
`ToolService._edge_results` (L~600) passes `edge.confidence` through.
The *unseen* part R7 names: (a) when `callers`/`callees` fall back to
`HeuristicToolSet` (heuristic.py), results also say `INFERRED` —
indistinguishable from the graph's INFERRED — and only the envelope
hint mentions degradation; (b) *unresolved* call sites are dropped
silently at `resolve_call_edges` (L346–348: `if target is None or
confidence is None: continue`), so "3 of 6 real call sites resolved"
is invisible in tool output — it lives only in
`docs/benchmarks-requests.md`.

**Plan.**
1. **Provenance on every edge-shaped result.** `_edge_results` gains
   `provenance: "graph" | "heuristic-name-match"`, set by the caller
   (`callers`/`callees` know which path served the envelope); heuristic
   toolset results get `"heuristic-name-match"` — a viewer can now
   discount correctly. `failing_context`/`affected_by`/`path_between`
   chain/hops get the same field. `entity_context`/`serialize.py`
   passthroughs updated so one shaping exists.
2. **Make resolution completeness visible.** At extraction, count call
   sites per unresolved-name (`resolve_call_edges` already has the
   candidates) and persist a tiny per-name tally — a `NodeKind.
   FUNC`-adjacent property or a dedicated stats edge written by both
   backends at load time (repository `load_extraction` change + Neo4j
   `merge_delta` + InMemory/Embedded equivalents, one protocol method).
   `callers()`/`callees()` then add
   `resolution: {"resolved_edges": N, "unresolved_call_sites": M}` so
   the requests-gap is *in the output*. (If persisting proves
   sprawling, fall back to computing on-demand from
   `_symbol_index`/extraction call-site records — decide at
   implementation, keep the tool contract identical either way.)
3. **Ground-truth fixture test**, modeled exactly on
   `tests/test_graph_semantics_ground_truth.py` (same
   known-by-inspection-oracle pattern, lines 1–64): a fixture with one
   statically-resolvable call, one receiver-inferred call, one
   unresolvable name referenced twice — asserting `provenance` labels
   AND `resolution` counts equal the human-derived truth, on BOTH
   backends (embedded factory + mocked Neo4j repo where the suite
   already has doubles).

**Impact / ripples elsewhere.**
- `docs/benchmarks-requests.md` gains one sentence: the documented
  3-of-6 gap is now visible in `callers` output (`resolution.
  unresolved_call_sites`), and README L60–62's footnote references the
  field instead of only the doc.
- `docs/tool-selection-accuracy.md`/competitive docs untouched (no
  surface-count change; this is output enrichment, not a new tool).
- `HeuristicToolSet` envelopes gain the provenance field — check its
  consumers (czy RepairAgent weighting code in be-v2 host relies on
  exact result shape? `heuristic.py` docstring says repair loops weight
  `confidence`; additive field is safe).
- CHANGELOG entry under the next version.

**Verify (from roadmap).** Ground-truth test asserts labels against
known-by-inspection truth, both backends; fresh standalone install
returns resolution counts on psf/requests matching the published
benchmark's 3-of-6 for `close()` — the claim and the tool agree.

---

### [x] R8 · `cie export-html` — the shareable artifact (M) — DONE 2026-08-30

*(Implementation summary: `cie/export_html.py` — read-only composition
through ToolService envelopes + one project_graph fetch for chains
(node-id-exact matching); CLI `cie export-html [PATH] --out`; XSS/blob
escape tests; zero-external-reference tests; real psf/requests export
(37 files/34 chains/680 orphans/215KB/0 external refs); screenshots
committed from `file://` via scripts/record_export_html.sh. Found+fixed
en route: the JS view-activation bug the first screenshot caught
(Overview never activated).)*

**State today (verified).** Nothing HTML-export exists; the pieces it
composes do: `traceability_chain`/`traceability_orphans` (ToolService,
verified bucket), `callers`/`file_skeleton`, `build_tool_service_
embedded` as the zero-config data path, and
`scripts/record_demo.sh` + `record_demo_client.py` as the
record-then-commit pattern for artifacts (`demo.svg` precedent).
`demo.cast` shows the cast format expected.

**Plan.**
1. New `cie/export_html.py`: pure stdlib `string.Template`-built
   single-file HTML (inline CSS/JS, **no CDN, no fetch** — `file://`
   hard requirement). Data via the read-only ToolService envelopes (one
   construction path, `build_tool_service_embedded` first-class), so
   export honors exactly what the tools expose and never opens a write
   path.
2. Views in v1, centered on what nobody else renders: the task→file→
   test **traceability chains** (from `traceability_chain`), orphan
   tasks/tests (from `traceability_orphans`), per-symbol **callers**
   pane, per-file **file_skeleton** listing. A single
   pre-rendered JSON blob embedded in the HTML keeps the page static
   and client-side-searchable; no server, no auth surface — state the
   "safe slice of a viewer" boundary in the command's docstring.
3. CLI: `cie export-html [PATH] -o out.html --project CIE_PROJECT`,
   routed through R2's backend seam so the embedded path is default.
4. Screenshot artifact: extend the record-then-commit pattern —
   `scripts/record_export_html.sh` (build page for a pinned repo clone
   + headless capture), committed output joined into README next to
   `demo.svg` *as a script output, not a one-off*.
   **Impact / ripples elsewhere:**
   - README new subsection under Quickstart ("Share it: one static
     file"), CHANGELOG entry, `docs/competitive-landscape.md`
     Graphify/GitNexus rows gain "cie: static HTML export" where the
     gap was listed.
   - R9's benchmark harness can reuse the export as a task input later;
     R15's `cie init` context-file suggested snippets mention the
     command.
   - No `ToolPolicy` surface change (read-only composition of existing
     tools) — note in the PR why the surface count doesn't move.

**Verify (from roadmap).** Export psf/requests; open via `file://` in a
plain browser with zero network/deps (verify by measuring zero external
requests in the capture); screenshot committed via the script.

---

### [x] R9 · Benchmark harness + third independent repo (M) — DONE 2026-08-30

*(DONE 2026-08-30: scripts/benchmark.sh + benchmark_tasks.py; third dataset
docs/benchmarks-urllib3.md — 12 receiver-attributed caller edges vs 28 raw
grep matches, 28/40 resolution miss published, 2.24× skeleton compression;
requests numbers regenerate matching the doc, verified twice.)*

**State today (verified).** `docs/benchmarks-requests.md` is the
methodology of record (94 lines: pinned commit `5460f467`, 3 task
shapes, honest-loss section); the two-column naive-vs-cie pattern and
the 3-of-6 miss. There is no script: the numbers were a session, not a
reproducible artifact. `scripts/` holds `record_demo.sh` (the
reproducibility pattern) and `smoke_patch_mcp.py`.

**Plan.**
1. `scripts/benchmark.sh` + `scripts/benchmark_tasks.py` (Python part:
   drive `ToolService` directly against an indexed clone — the same
   calls `record_demo_client.py` makes over MCP, minus transport
   ceremony): input = `(repo_url, pinned_commit, src_glob)`; steps =
   clone → `cie index` → run the 3 canonical task shapes (`easy
   definition`, `ambiguous callers`, `large-file skeleton`) → run the
   naive-side commands too → emit a **markdown table + raw JSON**.
   Add the new metric: **token-per-query** — chars of tool-result
   payload ÷ naive-side output chars for equivalent tasks, reported
   with the methodology caveat that token counting is chars-based
   unless a tokenizer pin is chosen; cite codebase-memory-mcp's
   "120× fewer tokens" strictly as vendor-claim vs ours-measured.
2. Third repo: pick one public, Python, ≥10k-line, status-quo-famous,
   and *not* `psf/requests` — candidate: `httpie/cli` (un-
   ambiguous entry points) or `keleshev/schema`; pick on "does it have
   the ambiguous-name property" after a dry `grep` probe. Run
   end-to-end, publish the doc as `docs/benchmarks-<repo>.md`, **keep
   the honest-loss section** — pre-commit to reporting whatever
   resolves badly.
3. `docs/benchmarks-requests.md` numbers: regenerate from the script
   against the pinned commit and reconcile (if the live numbers moved,
   the doc moves with a dated re-run note — never silently).

**Impact / ripples elsewhere.** README cites two measured numbers
(1-tool vs 3-tool; 3-of-6; 43% skeleton ratio) — after regen, update in
the same PR with dated footnotes; CHANGELOG; this feeds R20's
claim-audit table directly.

**Verify (from roadmap).** A fresh-clone reader reproduces all
published numbers end-to-end from the script alone; doc's tables are
script output, pasted with a run date + commit hash.

---

### [ ] R10 · GraphRAG/embedding first-party benchmark (M)

**State today (verified).** The semantic layer is exactly two modules:
`cie/embed.py` (wraps `core.llm.embed_text`, NVIDIA NIM; standalone
falls back to a raising stub — L21–47) and `cie/graphrag.py`
(module-level `from core.llm import ...` L32 → `qa` is
unavailable-by-design in standalone, which is also why it sits in the
R5 bucket). `docs/competitor-benchmarks.md` (146 lines) holds the
version-table rigor for the 08-28 run.

**Plan.**
1. Constraint stated first: a first-party GraphRAG benchmark must run
   in an environment *with* `core.llm` (R5 did not remove that
   dependency — it only stopped it from nuking unrelated tools). Either
   run in the be-v2 environment for this benchmark, or
   (preferred, small) give `cie.embed` a first-party OpenAI-compatible
   fallback path gated by `CIE_EMBED_DSN`/NVIDIA_API_KEY so the
   benchmark is reproducible from this repo alone. Decide before
   writing the harness; document the choice in the doc's methodology
   section.
2. `scripts/benchmark_semantic.py`: fixed question set (10–20
   questions spanning exact-symbol, conceptual, and cross-file questions
   on the same corpora used in R9 — requests + the third repo),
   competitors: **claude-context**, **grepai** (each pinned by version
   + index settings table), measurements: precision@k on
   hand-labeled relevance sets, answer grounding, tokens/context bytes
   returned per query, index time. Publish **wins and misses**.
3. Land numbers in `docs/competitor-benchmarks.md` as a new dated
   section with the same tool-version table rigor; update
   `docs/competitive-landscape.md`'s "where competitors are genuinely
   ahead" GraphRAG paragraph to cite measured results either way.

**Impact / ripples elsewhere.** Competitive-landscape + README claim
sentences that currently live only as "unmeasured" caveats; possibly
`pyproject.toml` gains an `embed` extra **only if** the first-party
fallback path is chosen (dependency: `openai` client — decide then);
R20's post cites the outcome table.

**Verify (from roadmap).** Numbers in `docs/competitor-benchmarks.md`
with the version-table rigor; competitor retrieval configs stated as
vendor-documented, ours stated as measured.

---

### [x] R11 · Streamable-HTTP transport for `cie-mcp` (M) — DONE 2026-08-30

*(More existed than the roadmap implied: the parser already accepted
the transport; implemented `--host/--port` pass-through, live
verification via the official streamable_http client, wiring tests,
README/docs/CHANGELOG.)*

**State today (verified).** More exists than the roadmap implies:
`cie/mcp_server.py::_build_arg_parser` already accepts `--transport
{stdio,sse,streamable-http}` (choice wired to
`server.run(transport=...)`), and the installed SDK (`mcp 2.x`
`MCPServer.run`) literally types
`transport: Literal['stdio','sse','streamable-http']` — so the remaining
work is host/port surfacing, verification, and honest docs, not
transport plumbing. What's *missing*: no `--host/--port` args (SDK
defaults hidden), no doc line anywhere, no test, and no verification
that the inspector policy surfaces the predicted schema set over HTTP.

**Plan.**
1. Add `--host` (default `127.0.0.1` — localhost-only by default is the
   safe choice; document that binding non-loopback turns cie into a
   network service subject to the ToolPolicy boundary) and `--port`
   (default 8000), passed through to `server.run(...)`.
2. Verification harness (extend `tool-test-lab/dogfood_mcp.py` or new
   `tool-test-lab/dogfood_mcp_http.py`): spawn `cie-mcp --embedded
   --policy readonly --transport streamable-http`, connect with the
   official MCP **Inspector in browser mode** (the roadmap's named
   client) *and* an SDK `streamablehttp_client`; assert: tools/list ==
   exactly `INSPECTOR_POLICY.permits()` predictions (server-side
   filtering, never client trust), and one write-tool call attempt is
   refused server-side with the standard envelope error.
3. Tests: `tests/test_mcp_server.py` gains an in-process transport test
   (ASGI-level through the SDK's http client where feasible) so CI
   covers the flag without needing a browser; keep the browser-verify
   as a manual, recorded step.
4. Docs: README quickstart gains a 6-line HTTP block (when to prefer it,
   policy note, loopback note); `docs/language-agnostic-design.md`
   transport matrix; CHANGELOG.

**Impact / ripples elsewhere.** R16's run-tool refusal test reuses this
harness (`run` refused over HTTP is *already* the default via read-only
policy — pin it here too); tool-test-lab README documents the new
harness; security posture sentence in README (a port is now openable).

**Verify (from roadmap).** A real browser-based MCP client (Inspector)
connects over HTTP and sees exactly the schema set inspector policy
predicts; write attempts are server-side-refused; both asserted by the
harness, screenshot/cast recorded.

---

### [x] R12 · Language #7: tree-sitter C (M) — DONE 2026-08-30

*(Implementation summary: `cie/extract.py` — `.c`/`.h` loaders,
`function_definition` (shared with Python's node type, per-file language
dispatch), `_DECLARATOR_TYPES` + `_c_declarator_name` (innermost
identifier, param_list excluded — the mirror bug pinned),
`_c_params_text`/`_c_return_type_text` through the declarator chain;
pyproject `tree-sitter-c>=0.23.0`; v1 scope: no struct-classes, no
header-prototype nodes — both documented + test-pinned. 9 tests in
tests/test_extract_c.py incl. the D1 never-silently-skips guard.)*

**State today (verified).** Same tables as R12 (`_LANG_LOADERS` L72–82, `_FUNCTION_TYPES` L94–103, `_PARAM_TYPES` L105, `_CALL_TYPES`
L114) cover 6 languages; the file's own comments document the
verified-per-grammar discipline to copy. The known trap is real: in
tree-sitter-c, a function's name lives inside nested
`function_declarator` → `parenthesized_declarator`/`pointer_declarator`
paths, while the current name-extraction helpers read
`child_by_field_name("name")` off the top node (L272, L580, L731) —
field-based lookup silently returns `None` and functions vanish, which
is exactly the "naive-skip" bug class the roadmap's verify demands a
guard against. Test suite to match:
`tests/test_extract_go_rust.py` (10 tests, real parses, documented-gap
assertions) is the bar.

**Plan.**
1. `pyproject.toml`: add `tree-sitter-c>=0.23.0` to core deps (C is
   common enough to ship in-core, matching the existing six).
2. `cie/extract.py`: `.c`/`.h` loaders; **build the declarator
   unwrapper** as a shared helper `_declarator_name(node)` (walks
   `declarator`/`function_declarator`/`pointer_declarator` chains until
   an `identifier` with a name-field, returning None → skipped
   symbol with a debug trace, never a silent false-empty); wire into
   `_walk_file`'s function branch and `_collect_call_sites`. Parameter
   extraction: C uses `parameter_list` (already in `_PARAM_TYPES` —
   verified present). Calls: `call_expression` with the `function`
   field → same-file/`includes`-based resolution; decide + document the
   C scope: `#include` → import-map analog (headers as file hubs) is
   in-scope if cheap; structs/classes: **no CLASS node in v1** (match
   the Go/Rust precedent of documented non-coverage, keep `struct` as
   plain symbols, not classes).
3. **The anti-naive-skip test** (the roadmap's real ask): a fixture C
   file with functions, prototypes, pointers-returning, and nested
   declarators must assert *exact* extracted symbol sets (positive
   assertions, not just non-crash), plus one test that would fail if
   extraction ever returned empty (e.g. `assert
   {f.name for f in ...} == {...}` with a non-empty expected set) —
   style-match `test_go_extracts_function_and_receiver_methods...`.
4. Docstrings/comments: extension docs
   (`docs/adding-a-language.md`) get a "C declarator" section naming
   the trap for the next language porter (it recurs in C++).

**Impact / ripples elsewhere.** README language list + badges,
`docs/competitive-landscape.md` language-count row (6 → 7, **do not
round up further**), `docs/language-agnostic-design.md` coverage table,
`pyproject.toml`, CHANGELOG, `tests/test_extract_c.py` (new, ~8–12
tests), and the R13 three-way declarator plan. `_docstring()` needs a
C branch (best-effort attach comment blocks or honestly None — same
graceful treatment as Go/Rust's empty docstrings, test included).

**Verify (from roadmap).** `test_extract_go_rust`-style suite against a
real C parse; naive-skip bug class asserted absent (a test that fails
if extraction silently returns nothing); a curl/mitlab-grade real C
file parses in conformance-style spot-check.

---

### [x] R13 · Languages #8–9: C++ and C# (M) — DONE 2026-08-30

*(Implementation summary: C++ — `_CPP_CLASS_TYPES` + body/name gate on
the walker (C's struct-invisibility stays pinned for `.c`),
`field_identifier`/`qualified_identifier` extensions to `_c_declarator_name`,
base_class_clause bases; C# — `_CSHARP_CLASS_TYPES`,
`_csharp_class_bases` (base_list first=extends/rest=implements,
documented), `invocation_expression`+`member_access_expression` in the
call pipeline; `.h` re-scoped to C headers with the mis-tree finding;
10 tests across both languages; counts 7→9 (README, landscape,
pyproject).)*

**State today (verified).** Same tables as R12; C++ and C# each reuse
R12's declarator path but diverge exactly where the roadmap says:
inheritance/impl resolution. Verified specifics to handle: C++
`class_specifier`/`struct_specifier` with `base_class_clause`
(virtual/visibility tokens inside), methods as
`field_declaration_list` entries with real `function_declarator`s
(tricky: names inside `pointer_declarator` for operators/
destructors); C# `class_declaration`/`interface_declaration` with
`base_list` (which carries contains both `extends`-and-`implements`
semantics in one clause — relation split by marker), `method_declaration`
with `name` field (friendlier, field-based).

**Plan.**
1. Two deps in one pass: `tree-sitter-cpp`, `tree-sitter-c-sharp` (pin
   versions; C# grammar package name is `tree-sitter-c-sharp` →
   module `tree_sitter_c_sharp`).
2. Reuse `_declarator_unwrap` from R12 for C++'s nested declarators; C#
   mostly rides the existing field-based name helper — assert that with
   a test, don't assume it.
3. Per-language inheritance: C++ `base_class_clause` → `extends`
   (access-specifier stripped); C# `base_list` → split
   first-identifier-`extends` / rest-`implements` (document the rule in
   the function's docstring, C#'s single-clause reality, verified
   against real parses — same verification standard as
   `_java_class_bases` at extract.py L642).
4. Update the three docs + pyproject + README count in the SAME PR as
   the code (no-stale-docs rule) and keep landscape's honest statement:
   this is 6 → 9, **not** 9 ≈ 21–40+; the gap sentence stays.

**Impact / ripples elsewhere.** Same set as R12 ×2 languages:
`tests/test_extract_cpp.py`, `tests/test_extract_csharp.py`;
`docs/adding-a-language.md` gains the "declarator languages" pattern
note; conformance surface unchanged (no new tools); benchmark docs
unaffected unless a C repo enters R9's set (note only).

**Verify (from roadmap).** R12's bar per language, plus: C++ multiple-
inheritance and C# interface-list parses assert exact base/relation
pairs; README/landscape/pyproject updated in the same pass.

---

### [x] R14 · PRD-hierarchy port to embedded SQLite (M) — DONE 2026-08-30

*(Implementation summary: `cie/embedded_hierarchy_repository.py` (same
protocol, shared validators, documented backend differences — single
HAS_CHILD direction, name-keyed unconditional REALIZED_BY);
`hierarchy_repo` param on ToolService + `hierarchy_tracking=False` /
`--no-hierarchy` opt-out with
`unavailable[HIERARCHY_STORE_NOT_CONFIGURED]`; three hierarchy tools
promoted (R1's playbook, incl. the WRITE_TOOLS trap); factory + cli
+ mcp wiring; 21 tests at B1's bar; conformance re-run: 135 tools,
100 verified / 26 graceful / 5 unavailable / 0 crashes; README#undefers
"still Neo4j only"; stale docstring fixed. push_hierarchy trip-wire
verified by the pinned invariant test firing exactly once when the
alias-set/WRITE_TOOLS swap was momentarily half-done.)*

**State today (verified).** `cie/hierarchy.py` holds the full
`HierarchyRepository` protocol (L151–188: `push_hierarchy`,
`get_children`, `get_lineage`, `get_hierarchy_node`, `get_project_tree`)
+ `Neo4jHierarchyRepository` (Cypher, APOC-free). The docstring claims
an in-memory fake at `tests/in_memory_hierarchy_repo.py` — **that file
does not exist** (verified `find`), a stale reference to fix here.
Embedded has *no* hierarchy story; `factory.build_tool_service_embedded`
doesn't thread one; `mcp_server --embedded` therefore can't serve
`push_hierarchy`/`get_children`/`get_lineage` even after R1's
promotion. CLI `hierarchy:*` commands route through `_open_hierarchy_repo`
(hardwired Neo4j) — R2's seam covers them once the repo exists.

**Plan.**
1. `cie/embedded_hierarchy_repository.py`: `SQLiteHierarchyRepository`
   implementing the protocol, backed by the same two-table style as
   `EmbeddedRepository` (a `hierarchy_nodes` table with
   id/parent_id/type/props JSON + `project` column; the
   `_bev2_rel_type_union()` shim's edge-type nuance collapses here —
   SQLite stores parent_id, but preserve the label-union semantics in
   `get_children`'s filtering so `get_project_tree` returns the same
   discover-profile views as `TYPE_TO_LABEL` maps, L60–80).
   Validation lives in `cie/hierarchy.py`'s shared helpers
   (`find_repeated_id`, `count_nodes`) — import, don't duplicate.
2. Wire: `build_tool_service_embedded` gains the hierarchy repo
   (respecting `--no-task-tracking`'s precedent for a
   `--no-hierarchy` fail-fast mode); mcp_server's `--embedded` path
   inherits it automatically via the factory; CLI `_open_hierarchy_repo`
   switches via R2's seam.
3. **Complete R1's hierarchy promotion here:** `push_hierarchy`,
   `get_children`, `get_lineage` become ToolService methods (routes'
   alias handlers → `_service_tool`), `push_hierarchy` lands in
   `WRITE_TOOLS`, `HTTP_WRITE_ALIASES` empties, invariant-sets updated —
   reusing R1's playbook exactly, including the WRITE_TOOLS trap.
4. Tests at the B1 depth: `tests/test_embedded_hierarchy_repository.py`
   mirroring `test_embedded_task_repository.py`'s 17-test bar (CRUD +
   lineage + project scoping + repeated-id rejection + empty-tree
   envelopes).
5. Docs drop the "Neo4j-only" caveat: README task/QA section,
   `docs/language-agnostic-design.md` backend matrix, CHANGELOG;
   fix `cie/hierarchy.py`'s stale fake-pointer docstring in the same
   PR.

**Impact / ripples elsewhere.** `mcp_server --embedded`'s help text
(task-tracking flag text); R1's invariant updates; conformance re-run
(surface grows by the 3 new ToolService methods — Count Contract);
roadmap cross-ref: this closes goal.md's own "last Neo4j-only feature"
note.

**Verify (from roadmap).** Hierarchy CRUD + lineage tests at the
17-test bar against SQLite; `push_hierarchy` over `--embedded` MCP
round-trips a real tree; docs carry no "Neo4j-only" hierarchy caveat
(grep-clean).

---

## P2 — adoption & launch mechanics (gated on R6)

### [x] R15 · `cie init` one-command onboarding (M) — DONE 2026-08-30

**State today (verified).** No `init` command exists; README's
quickstart (L85–107) ends with "add it to Claude Code / Cursor / Codex
the way you'd add any other local MCP server" — i.e., all-manual.
GitNexus's AGENTS.md/CLAUDE.md files and CodeGraph's installer are the
named exemplars (delta S2).

**Plan.**
1. `cie init [PATH]` (new command in `cie/cli.py`, after R2's seam):
   detect installed clients by config-file presence — Claude Code
   (`~/.claude.json` / project `.mcp.json`), Cursor
   (`~/.cursor/mcp.json`), Codex (`~/.codex/config.toml`) — `--client`
   override flag, `--list` printout; register the stdio server entry
   (`command: "cie-mcp"`, `args: [<abs path>, "--embedded"]`,
   `--policy readonly` **default** with `--policy` override, print the
   exact diff before writing, idempotent re-runs).
   Safety default note: readonly-until-chosen avoids surprising write
   powers in a client the user didn't intend to grant them — surface
   the choice, default safe (this is the fallback-differentiator d4
   story in product form).
2. Write context files: `<root>/AGENTS.md` + `CLAUDE.md` (append-or-
   create with a marked managed block, never clobber user content):
   what cie tools exist, when to use `callers` vs `grep`, the
   export-html hint, and the task/QA traceability one-liner — sourced
   from a template const in `cie/init_templates.py` so tests can pin
   it.
3. Demo: extend `scripts/record_demo.sh` pattern — a `record_init.sh`
   capturing `cie init` → client listing the tools (the demo cast
   helps, keep it ≤30s).

**Impact / ripples elsewhere.** README quickstart slims to `pip
install → cie init` with the manual path demoted to an appendix;
`docs/competitive-landscape.md` handgun the "no installer" honest-gap
row; CHANGELOG; CONTRIBUTING unchanged; watch W2 (policy-proxy) not
affected.

**Verify (from roadmap).** Fresh clone → `cie init` in a container/VM
with one real installed client → client lists cie tools with zero
manual config; captured for the demo cast; `--policy` default honored
by what the client lists.

---

### [x] R16 · `run`-tool isolation story made explicit (M) — DONE 2026-08-30

**State today (verified).** The honest core exists:
`cie/tools/runner.py` L3–8 states "v0 isolation is subprocess only —
cwd jail plus hard timeout", `routes.py` carries the "no container
isolation yet" comment, and the policy side is real (WRITE_TOOLS
gates `run`/`run_tests`; HTTP default-inspector refuses, verified this
session per goal.md). Missing: a written threat model, a per-surface
refusal test, and the optional container mode.

**Plan.**
1. Docs first: `docs/security.md` — the `run` boundary precisely:
   what the jail is (cwd via `allowed_root`, resolve-confined, hard
   timeout + process-group kill, bounded output — cite runner.py
   L96–122), what it is NOT (no fs sandbox beyond cwd for child
   processes that have their own permissions, no network restriction,
   no container), the threat model (trusted local user; untrusted
   HTTP/MCP callers get policy refusal, not jail security), and the
   escape hatch `CIE_RUN_ROOT` exists and what widening it means.
2. Optional container mode: a documented, *minimal* seam —
   `CIE_RUN_WRAPPER="docker run --rm -v {root}:{root} -w {root}"`-style
   prefix executed through the same runner (no new dependency, no
   docker SDK) + docs stating it's convenience, not enforcement.
3. The pinned test: extend `tests/test_http_policy.py` with a one-test
   loop asserting every networked surface (HTTP `POST /tools/run`,
   MCP under readonly policy, CLI server path) refuses `run` by
   default with `ToolNotPermitted` semantics — the machine-checkable
   half of the claim.

**Impact / ripples elsewhere.** README security footnote links the
doc; `runner.py` docstring cross-references; CHANGELOG; R11's harness
reuses the refusal assertions; mcpservers listing (R18) can safely
state the boundary.

**Verify (from roadmap).** `docs/security.md` states the threat model;
policy test pins read-only refusal of `run` on every surface;
container mode demonstrated once in a cast (optional, doc'd).

---

### [x] R17 · Conformance in CI (S) — DONE 2026-08-30

**State today (verified).** `.github/workflows/ci.yml` runs pytest on
py3.10–3.13 + an MCP stdio smoke step (initialize + tools/list) — good
floor, but conformance (`tool-test-lab/surface_conformance.py`) runs
only when a human remembers. Known blocker inside the harness itself:
L107 hardcodes `PYTHONPATH=/home/arun/Downloads/cie` (its README warns
to edit it).

**Plan.**
1. Fix the pin: derive from `Path(__file__).resolve().parents[1]` (repo
   root), keeping an env override for sandbox use — resolves the
   tool-test-lab README's ⚠ note.
2. Add a CI `conformance` job: `pip install -e ".[mcp,http]"`, index a
   committed micro-fixture repo (`tests/fixtures/sandbox/` — 5 files,
   deterministic content; matches the harness's `app.py`-guessing
   `arg_value` heuristics), run
   `python tool-test-lab/surface_conformance.py <sandbox>
   surface_results_ci.json`, then a small comparator:
   - **0 CRASH** always;
   - surface count == `ToolService` introspection count read in-process
     (never a magic constant forever — store an approved-counts file,
     `tool-test-lab/approved_surface.json`, updated *only* in PRs whose
     point was a surface change, so silent drift is red and deliberate
     drift is a reviewed diff);
   - every `unavailable-by-design` entry must carry an approved `reason`
     (ties into R5's registry).
   pytest should invoke this where feasible (a
   `tests/test_surface_conformance.py` marked slow, run in the same
   job) so the invariant is enforced by the test runner, not only yml.
3. Prove the red/green path once on a branch (intentionally break one
   tool → CI red → restore → green) and record the run links in
   goal.md.

**Impact / ripples elsewhere.** `tool-test-lab/README.md` (un-warn the
path pin, document the CI contract); `approved_surface.json` joins the
artifacts-of-record list; R3's count convention becomes CI-enforced —
after this lands, the no-stale-docs rule's tool-count half is
machine-checked.

**Verify (from roadmap).** Intentionally break one tool → CI red;
restore → green; invariant machine-checked from then on.

---

### [ ] R18 · Directory listings (S, human action — gated on explicit go-ahead) — drafts DONE 2026-08-31

*(Step 1 complete: `docs/directory-listings-drafts.md` holds the
mcpservers.org field set and the awesome-mcp-servers PR text, every
string sourced from the shipped README, R6 gate satisfied. Submission
awaits the owner's explicit go-ahead; README badge row stays un-added
until listings are live.)*

**State today (verified).** goal.md E3 names the two targets
(mcpservers.org/submit, awesome-mcp-servers PR) and the rule: external
action under this account needs explicit go-ahead, per goal.md's "needs
your go-ahead" framing. Draft complete 2026-08-31; README has no links
to add yet (correctly — no live listing to link).

**Plan.**
1. Draft (no submission): mcpservers.org entry fields (name, tagline
   from README L22's differentiator sentence, install command from
   quickstart, stdio transport note from R11 work, policy note);
   awesome-mcp-servers PR text (one line, section placement per repo
   convention, link to README + benchmark docs).
2. Wait for explicit go-ahead (the gate is the point), then submit both;
   capture PR URLs into goal.md E3.
3. After listing is live: README "Listed on" badge row + links back.

**Impact / ripples elsewhere.** README badge row only after live
(never link-to-404); R6 must have shipped first (C1 gate) since
listings point at the stable artifact.

**Verify (from roadmap).** Entries live; README links them; goal.md E3
checked by hand.

---

### [x] R19 · Repo trust signals + second-maintainer activation (S) — DONE (release page rides R6) 2026-08-31

*(Implementation: description + 10 topics applied via `gh repo edit`
2026-08-31; three `good-first-issue` issues created — CLI-tests GFI →
#17, failing_context ground truth → #18, urllib3 reproducibility → #19;
README label link landed in 5b527c4; drafts + stamps in
`docs/repo-trust-signals.md`. The release page is R6 step 3's own
checklist item (notes rendered from CHANGELOG at cut time). R6's PyPI
upload is blocked by a real finding: pypi `cie` is an unrelated
project.)*

**State today (verified).** CONTRIBUTING.md has a real
second-maintainer path (README L93 links "Becoming a second maintainer"
— the C4 work). Missing: GitHub repo description/topics, a rendered
release-notes page, labeled starter issues.

**Plan.**
1. Repo settings: description (differentiator one-liner, stable-version
   note), topics (`mcp`, `code-graph`, `tree-sitter`, `static-
   analysis`, `llm-tools`, `neo4j`, `sqlite`).
2. Release page: rendered from CHANGELOG 0.1.0 section (R6's notes
   body) — do once at R6, keep in step per release (R6 checklist
   notes it; this item adds the *issues* half).
3. 2–3 `good-first-issue` issues chosen against CONTRIBUTING's named
   safe-entry modules (pick from R12/R13's test-suite adjacencies or
   R17's approved-surface workflow — genuinely small, well-scoped,
   verifiable items), each with acceptance criteria + pointer to
   CONTRIBUTING's review bar.
4. README: link the issues from the contributing section.

**Impact / ripples elsewhere.** CONTRIBUTING may need a one-line
"current starter issues" pointer (dated — or link the label instead of
hardcoding numbers, so it never rots).

**Verify (from roadmap).** A newcomer reaches a labeled starter issue
from README in ≤2 clicks; C4's path verified as true by walking it.

---

### [ ] R20 · Launch post, drafted but gated (S + timing)

**State today (verified).** All raw material exists and is measured:
demo (recorded), benchmarks (2 docs + R9's third), tool-selection
accuracy (14/14 runs), surface conformance numbers, the honest-loss
sections. The declined critique (#10, session 3) and the claim-audit
demand come from goal.md.

**Plan.**
1. `docs/launch-post-draft.md`: Show HN + r/LocalLLaMA variants, both
   built from R8/R9 artifacts (the HTML export screenshot and the
   reproducible benchmark script are the post's spine — claims the
   reader can reproduce in 3 commands).
2. The claim-by-claim table: every sentence that asserts something
   footnoted to its measured source (file + section + date): benchmarks
   docs, `surface_results.json`, `docs/tool-selection-accuracy.md`,
   competitor claims labeled vendor-claims only. **Losses stay in the
   post** (the 3-of-6 miss, the tie in Task 1, R9's reported misses) —
   that's the floor of the honesty stance, non-negotiable per
   session-3 critique #10.
3. Publish gate: R6 shipped (not merely drafted), R9's numbers public,
   a re-read for rate-limits/timing (Show HN best practice), and the
   user's explicit go-ahead. Draft everything; publish nothing from
   here.

**Impact / ripples elsewhere.** If R9/R10 moved numbers, the post cites
the newest dated values; the audit table double-checks R3's counts —
it is the last walk of the Count Contract before an audience sees it.

**Verify (from roadmap).** Claim-by-claim table in the draft, each row
footnoted to its measured source; nothing published from vibes.

---

## Session log

- **2026-08-30 (planning pass):** read roadmap + goal + CONTRIBUTING;
  walked `cie/routes.py`, `cie/mcp_server.py`, `cie/tool_policy.py`,
  `cie/tools/__init__.py`, `cie/tools/heuristic.py`, `cie/cli.py`,
  `cie/factory.py`, `cie/embedded_repository.py`,
  `cie/embedded_task_repository.py`, `cie/hierarchy.py`,
  `cie/extract.py`, `cie/callgraph.py`, `tests/*` (invariants,
  ground-truth, smoke), `tool-test-lab/*` (harness + results),
  `.github/workflows/ci.yml`, README/CHANGELOG/docs counts. Findings
  folded into per-item "State today" sections. Notable: R11's transport
  choice partially exists in the CLI parser already; R14's referenced
  in-memory fake is missing on disk; surface_conformance.py has a
  machine-local PYTHONPATH hardcode (blocks R17 until fixed);
  `ToolService` measures exactly 126 public methods on this commit.
- **2026-08-30 (implementation pass 1)** — R4 done: fold executed
  (tombstone + goal.md Provenance section + 11 reference rewrites),
  verified clean. Committed.
- **2026-08-30 (implementation pass 2)** — R2 done: CLI↔SQLite parity.
  Also fixed en route: the venv's cie was a stale non-editable
  site-packages copy (the conformance-README trap) → now editable;
  suite 171 → 184; quickstart verified against psf/requests in a clean
  venv (858 nodes/1,822 edges; callers("close") = 3, matching
  docs/benchmarks-requests.md). Committed.
- **2026-08-30 (implementation pass 3)** — R1 done minus `push_hierarchy`
  (blocked on R14's SQLite hierarchy store by design, alias stays).
  Found en route (honest note, not fixed here — pre-existing semantics
  preserved by the promotion): the "push is idempotent per task name"
  hint is only true idempotence when the batch re-pushes the same task
  id; a fresh-id re-push with the same name creates a second row (both
  backends; plan_push rejects duplicate names within one batch only).
  Documented by test. Suite 186 → 205. Committed.
- **2026-08-30 (implementation pass 4)** — R5 done: 18 → 5 unavailable,
  all with reason slugs; 13 tools now run purely standalone; bucket
  scan caught and fixed `failing_context("")` IsADirectoryError;
  conformance 100/23/5/4, 0 crashes; suite 205 → 212. Committed.
- **2026-08-30 (implementation pass 5)** — R14 done: SQLite hierarchy
  store + the last three HTTP-only tools promoted; HTTP_WRITE_ALIASES
  empty; surface 132 → 135 (conformance 100/26/5/4, 0 crashes); suite
  213 → 235. The R3-era "hierarchy still Neo4j-only" caveat flipped in
  README; hierarchy.py's nonexistent-fake docstring pointer fixed.
  Committed.
- **2026-08-30 (implementation pass 6)** — R7 done: provenance labels +
  call-resolution tallies visible in callers/callees output; live psf/
  requests check (resolution 19/16/3, name-keyed reconciled against the
  doc's 6-site framing); suite 235 → 239. Committed.
- **2026-08-30 (implementation pass 7)** — R11 done: streamable-http
  end to end (wiring + harness + docs); suite 239 → 241. Committed.
- **2026-08-30 (implementation pass 8)** — R8 done: export-html feature
  (CLI + module + tests), psf/requests artifact + screenshots recorded
  via the script; suite 241 → 246. Committed.
- **2026-08-30 (implementation pass 9)** — R9 done: harness + third
  dataset (urllib3), README benchmark paragraph links the third repo;
  suite 246 still green (scripts-only + doc). Committed.
- **2026-08-30 (implementation pass 10)** — R12 done: C language #7
  (declarator unwrap + mirror-bug pin + guard suite); suite 246 → 254.
  Committed.
- **2026-08-30 (implementation pass 11)** — R13 done: C++ (#8) + C# (#9);
  two live-found fixes (`.h` mis-tree under C grammar for C++ —
  re-scoped with docs; `cs`-vs-`csharp` language-key mismatch); suite
  254 → 264. Committed.
- **2026-08-30 (implementation pass 12)** — R17 done (CI conformance
  gate + approved_surface.json + fixture; one collection leak fixed);
  R16 done (docs/security.md + CIE_RUN_WRAPPER seam + spawn-guard test);
  suite 264 → 266. Committed in two commits.
- **2026-08-30 (implementation pass 13)** — R15 done: cie init
  (detection + guarded merges + managed context blocks + readonly-by-
  default), verified via a real stdio handshake on the registered entry;
  suite 266 → 279. Committed.
- **2026-08-31 (implementation pass 14)** — R19 applied end-to-end:
  repo description + 10 topics live (gh repo edit), starter issues #17/
  #18/#19 created against CONTRIBUTING's safe-entry list; draft doc
  committed with apply-stamps. En-route finding recorded under R6:
  pypi.org/project/cie is cluster311/cie10 — the name-availability
  assumption in R6's step 4 is false; needs a package-name decision.
  README contributing link verified already in (5b527c4).
- **2026-08-31 (implementation pass 15)** — R6 executed end-to-end
  (minus PyPI upload, blocked on a package-name decision): version
  house, artifacts built + twine-passed, clean-venv rehearsal from the
  wheel on the third-party repo (CLI + real MCP client, all matching
  published numbers), tag `v0.1.0` + GH release with caveats kept, C1
  hand-closed at tag `v0.1.0` on `a22b4bf`. Committed in two commits
  (release cut; goal/todo close-out). PyPI finding: pypi `cie` =
  cluster311/cie10; README quickstart switched to GitHub installs with
  a dated note until the distribution name is decided. Next update
  appends here with date + what moved.