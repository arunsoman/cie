# PRD v2.0 — implementation record

**Spec:** `docs/prd-v2.0.md` (polyglot architecture: tree-sitter parsing ·
SQLite storage · G6/Mermaid/ReactFlow/JointJS visualization · Ollama LLM ·
OpenTelemetry runtime).
**Crate:** `cie-polyglot` — every slice below is one commit, pushed to `main`
as it landed. Test counts are crate-local (`cargo test -p cie-polyglot`).

## Already standing before this record (verified, not rebuilt)

| PRD area | Where it lives |
|---|---|
| tree-sitter parsing substrate (§13, §39) | the 12 grammar-pinned adapter crates (`cie-adapter-*`), workspace `tree-sitter = 0.25` + per-language grammars |
| ViewSpec + gate + multi-engine (§25–31) | `viz-adapter/` (negotiator, capability matrix, hash split, fisheye, encoding fidelity, 76-chart catalog) — see `docs/VIZ-MULTI.md` |
| Native LLM (§36–38) | `cie-llm` (offline default, content-addressed cache, budget downgrade) |
| Ingest budgets / parallel walk (§7) | `cie-ingest` (`budget_ms`, rayon walk) |
| Behavior/conformance/FCA/drift (§17, §21, §24) | `cie-analysis`, `cie-concepts`, `cie-drift` (TRACEABILITY §5: 84/84 PRD v1.0 tasks) |
| Multi-repo (§10) | `cli:ingest-org`, `cie-ingest::run_org_ingest` |

## The slices that landed (this record)

| Slice | PRD | What landed | Tests |
|---|---|---|---|
| 9 | §11.1/§11.3/§13 | SQLite AST schema (`files`, `node_kinds`, `node_text`, `ast_nodes`, `symbols`) — every row from a real tree-sitter parse; one transaction per file; ANL-06 `shape_hash` (pre-order kinds, rename-stable); parent-qualified symbols; parse errors still indexed and flagged; unsupported files counted, never guessed; revision-scoped rows (time-travel groundwork, acc #11) | 8 |
| 11 | §11.2/§14/§42 | UAST normalization — `uast_kinds` + `uast_map` (PRD §11.2 pinned rows at confidence 1.0, extended to the full §39 top-10 with kind names verified by real parses); one-join polyglot queries (`all_functions`, `by_uast_kind`, acc #77); unmapped grammar kinds stay NULL and COUNTED; idempotent | 4 |
| 10 | §15/§11.4/§11.5/§11.10 | IR lowering — per-symbol CFG (`ir_blocks`/`ir_instructions`/`ir_edges`) via per-language control-flow seed tables; every Call/Ret/Throw/Cmp carries its `ast_node_id` back-pointer (§11.5 invariant); the six sequence-diagram fields on `calls` (block_id, call_kind from await/lambda/go-statement, guard_summary rendered from the enclosing branch, throws_set only from real in-file throw bodies); RESOLVED-by-name / UNRESOLVED-with-callee_text; ANL-05 `ir_hash` + `ir_vec` op-bag; `file_source_cache` (zstd) | 8 |
| 12 | §11.8/§11.9/§18 | framework registry (§11.9 columns exactly; Spring/Actix/Axum/Rocket/Gin/Echo/Chi/FastAPI/Flask/Django/Express/NestJS/ASP.NET Core); detection emits `framework_roles` citing evidence; framework NAMES only from import/use evidence (shape alone = unattributed @ 0.5); cross-language bindings three tiers — DECLARED from parsed OpenAPI (JSON + YAML path scan) conf 1.0 (acc #78), URL_MATCHED from real client-call path literals conf 0.7 (acc #79), routes in Spring+FastAPI+Actix (acc #80); RUNTIME_OBSERVED reserved for the runtime layer | 6 |
| 12b | §20/§22.1/§11.7 | C4 detection — containers from manifest roots (§20.1 list, longest-prefix scoping); components from framework annotations (Fact, evidence in provenance), naming-suffix groups (*Repository→Persistence, *Controller/*Handler→Interface, Inference 0.7), DDD dir segments (Inference); classes link directly to components, files are evidence only — the no-file-path correction enforced + acceptance-28-guarded; relationships per §20.3 (cross-component `uses`, DECLARED→`calls` Fact, URL_MATCHED→`calls` Inference) with container-level membership fallback; §11.7 concept DAG tables exactly; every edge reason in the §22.3 closed set | 5 |
| 13 | §19 | state machines T1/T2/T3 — T1 Explicit (XState `createMachine` states+transitions verbatim from the literal; Spring `StateMachineBuilder` marker with the honest not-literal-extractable note) = Fact; T2 Structural (enum + switch/match arms assigning the switched variable; coverage counts case LABELS only — a transition target is an assignment, not handled behaviour) = Inference, incomplete → Fog with "N of M enum members appear in switch arms"; T3 Behavioral (≥2 branches + ≥2 cross-file resolved calls) = Hypothesis with the mandatory counter-argument; `audit()` enforces the disclosure rules | 6 |
| 14 | §19.2/§25/§26 | workflow detection — forward DFS from real entry points (route/boot sites + main/app/server names) over RESOLVED call edges only; conditional-heavy callees = gateways (Inference); async hand-offs STOP the walk = async boundaries (Fog with reason, acc #75); cycles collapse; workflows are Hypotheses with counter-arguments; state machines + workflows share the symbol surface (acc #76 seam); §25 ViewSpec producer whose `validate()` mirrors the TS `gate()` refusals (ELEMENT_WITHOUT_EVIDENCE / NON_FACT_WITHOUT_CLAIM / HYPOTHESIS_WITHOUT_COUNTER / FOG_WITHOUT_REASON) so the layers cannot drift | 6 |
| 15 | §23 | impact radius + duplicates — the §23.2 tier ladder exactly: `ir_hash` exact = Fact; cosine ≥ 0.85 = Inference; 0.60–0.85 + shape match = Inference (ir-shingle); 0.40–0.60 = Hypothesis with counter-argument; below 0.40 stored as `tier='hidden'` and surfaced as spec gaps (counted, never shown); canonical marking per family (connected components): most callers AND most tests, tests counted only from real test-path call sites; the impact-dup ViewSpec builds the §23.1 shape (center / caller rings / duplicate hemisphere, similarity + isCanonical + drift, bidirectional similarity edges that are never call arrows) and passes the gate mirror | 4 |
| 20 | §9/§45/§44 | runtime evidence — the §45 table exactly (`runtime_evidence`, indexed by symbol+observed_ns and trace); OTLP/JSON + flat span-array parsing; acc #32 at the storage layer: mapped to SymbolId (route evidence, then name match) or UNRESOLVED with reason in the documented `runtime_unresolved` extension; per-symbol `runtime_summary` (hits/errors/total/last-seen); hot paths by hits (acc #34); flaky ops (OK and ERROR both observed); runtime drift (acc #33) = observed parent→child span pairs with NO static RESOLVED call edge (`unmodeled-edge`, note stated) | 4 |
| CLI | §7 | `cie polyglot <path>` — fresh store per run (honest recomputation), `ensure_all_schemas` in dependency order, the full stack in one report, `--otel <file>` runtime join, `--viewspec <name>` §25 artifact emission; verified end-to-end on the generated sample repo | cli 18 |

## Documented deviations (same discipline as TRACEABILITY §2)

- `runtime_unresolved` + `framework_roles` + `c4_relationships` + `ir_meta`
  tables are documented extensions: the PRD defines the §45 DDL (symbol_id
  NOT NULL — so Unresolved needs its own home), the §11.9 registry (whose
  detection OUTPUT is unnamed), the §20.3 derivations (no table named), and
  the ir_hash/ir_vec computation (no carrier named).
- OpenAPI YAML paths are read by a top-level-key scanner (no YAML
  dependency in the workspace); JSON specs go through serde_json. Shapes
  the scanner cannot match yield an EMPTY set, never a guess.
- T2 coverage counts case labels only: being a transition TARGET means a
  state is assigned to, not that its behaviour is modeled. The PRD's
  "4 of 6 enum members appear in switch arms" is a handling statement.
- Java `switch` is `switch_expression` in tree-sitter-java (no condition
  field) — the subject is the first identifier child; verified by parse.
- Hidden duplicate pairs are STORED (`tier='hidden'`) so the per-center
  spec can cite its own below-threshold count as a gap.

## Verification

- `cargo test -p cie-polyglot`: **50 green** (10 unit families: schema, uast, ir, frameworks, c4, statemachines, workflows, duplicates, runtime, viewspec).
- `cargo test -p cie-cli`: 18 green (including the polyglot command surface).
- Full workspace: `cargo test --workspace` green (see CHANGELOG entry for the run that landed with this record).
- End-to-end smoke on `cie sample` output: `cie polyglot demo-sample` reports
  9 symbols / 506 ast nodes / 9 lowered / 33 blocks / 136 instructions / 9 calls /
  3 canonical-hash duplicate facts; 0 containers and 0 workflows are the honest
  §20.1/§19.2 answers for a manifest-less, framework-less fixture.
