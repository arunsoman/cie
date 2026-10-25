# Atlas migration matrix

Code inventory: 2026-10-25. Governing rule: [any chart, anywhere](atlas-semantic-architecture.md). A rendered legacy chart is not a migrated core chart. Every analytical producer must send and receive shared selection, retain contributor identity, and support evidence navigation before its old path is retired.

## Persona routing

| Persona | Current route | Delivered slice | Remaining objective gaps |
|---|---|---|---|
| Fresher | `FresherAtlas` → shared core | Guided file → declaration → function exploration; distinct onboarding layout; shared filtering and evidence | External systems, unresolved entry coverage, data flow, automated tour and start-point ranking |
| Reviewer | `ReviewerAtlas` → shared core | Symbol, file, kind, declaration, native CFG and static call views | Full unresolved-entry coverage, read/write effects, findings and change-specific review |
| Architect | `ArchitectAtlas` → shared core | PRD seven-panel grid, declared dependency graph, candidate contexts, scorecard, structural duplicate candidates, captured drift and risk treemap; charts-first display | Dependency hero, external systems, complete component interactions, duplication, drift, timelines, risk charts and quality collection |
| Auditor | `LegacyAtlasView` | Legacy facts preference; core has locked facts policy but this screen is not migrated | Evidence-backed routes, externals, findings, boundary writes and locked policy in rendered UI |
| Consultant | `LegacyAtlasView` | Legacy scoped exploration | Shared producers, busiest-container evidence and change-context composition |
| Performance | No selectable persona in `PERSONA_ORDER` | PRD and foundation plan only | Measurements, performance semantic producers and an explicitly supported UI route |

The five selectable personas are defined in `web/src/lib/atlas-presets.ts`; the sixth has a PRD but is not currently offered by the menu. Migrated slices do not imply full PRD completion.

## Chart-by-chart inventory

Legacy panel membership: F=Fresher, R=Reviewer, A=Architect, U=Auditor, C=Consultant. Membership below records the legacy preset, including panels replaced by the partial core slices above.

| Legacy analytical item | Personas | Existing source | Core status and required work |
|---|---|---|---|
| Containers / C4 | F A U C | `buildC4Spec`, stack apps/externals + report | Registered as `containers`: captured build application identities, unique longest-directory ownership, captured cross-service matches. Architectural grouping remains Inference; deployment and external systems remain unverified. |
| Application entries | F R C | stack mains | Registered as `entries` for uniquely resolved captured bootstrap targets; class identity remains a class, unsupported listen/inline entries remain unresolved |
| Service entries | F R U | stack routes | Registered as `routes` with exact captured handler links; nested declared calls expand up to eight levels; ambiguous/unsupported handlers are excluded from symbol-bound scope |
| Components | F R A U C | `buildComponentSpec`, concepts/report | Registered as `components`: captured framework classes plus stored generated groupings, with recorded symbol membership and container context. Complete component dependency mapping remains pending. |
| Concept hierarchy | F R A U C | `buildConceptTreeSpec`, concepts | Registered as `concepts`: generated interpretations, primary + extra parent edges, descendant selection, representative member evidence. Concept label correctness still requires domain review. `hierarchy` remains declarations. |
| Flow | F R A U C | `buildFlowSpec`, report/selection | Native function CFG exists in core; aggregate component/API flow not yet migrated |
| Sequence | R A | `callSiteSequence`, report graph | Typed static sequence exists in core; occurrence order, dispatch uncertainty, depth limits disclosed; not runtime proof |
| Data flow | F R A U C | `buildDataFlowSpec`, recorded reads/writes | Missing typed producer and storage relations; do not substitute declared calls |
| Test coverage | Conditional legacy quality footer | quality coverage heatmap | Missing metric producer with file/test contributor mapping |
| Complexity | Conditional legacy quality footer | quality function metrics | Missing metric producer with exact symbol contributors |
| Churn per file | Conditional legacy quality footer | quality churn sample | Missing revision-bounded metric producer |
| Hot spots | Conditional legacy quality footer | quality hotspot rows | Missing ranked metric producer, score provenance and contributor mapping |
| Cross-service evidence | Legacy screen | `CrossServiceEvidence`, stack | Missing registered boundary view and relationship selection |

All legacy quality charts render through `QualityPanel`; their existing click behavior is not proof of universal core conformance. Existing sources are implementation receipts, not a claim that native storage exposes every required relation.

## Registered core catalog (available to either migrated persona in any slot)

| ID | Meaning | Selection / evidence |
|---|---|---|
| files | Symbol counts grouped by captured file | Aggregate contributors; captured file on double-click |
| symbols | Selected symbol table with pagination | Stable symbol IDs; declaration span and file |
| kinds | Selected symbols grouped by kind | Aggregate contributor descriptor; source only where supplied |
| hierarchy | Captured file/declaration parent structure | Entity contributors and recorded parent evidence |
| containers | C4 L2 application boundaries | Captured app ownership; typed container contributors and representative member source; no deployment proof |
| components | C4 L3 component interpretation | Captured framework roles and generated groups; class/method contributors and parent/container context |
| scorecard | Captured per-file coverage, complexity, duplicate-pair, risk and security values | File contributors, source, cursor paging, explicit unknown/zero semantics; collection and ENG-11 gaps remain |
| entries | Captured application bootstrap entries | Unique recorded symbol target; bootstrap classes do not imply function IR |
| routes | Captured service routes | Exact handler selection; API double-click opens that handler, file cells select file contributors |
| concepts | Generated concept DAG with recorded parent edges | Typed concept-to-symbol contributor mapping; recorded descendants up to eight levels; representative member source; never upgraded to Fact |
| controlflow | Native IR for one selected function | Blocks/branches point to captured code; unavailable IR is explicit |
| sequence | Structured static call occurrences | Symbols, call evidence and disclosed unresolved boundaries |
| calls | Declared call dependency graph | Symbol/relationship contributors; static inference policy applies |

## Checkpoint acceptance

For each migrated persona: select a file, select a function, remove just the function, remove the file; verify every analytical view population. Double-click must open captured evidence without toggling selection. Duplicate or move a chart while selected; scope must survive. Switch personas and back; scope must survive. Validate semantic chart shape separately from the shared-set invariant.

Do not delete `LegacyAtlasView`, the legacy preset vocabulary or shared builders yet: Auditor and Consultant still depend on them. Retire each producer only after all callers have migrated. Ask CIE/MCP still need explicit shared-context integration.

## Validation receipt — Fresher checkpoint

- 166 automated tests passed; TypeScript check and production build passed.
- 19 production-browser checks passed with no browser errors using a freshly ingested Python fixture, including file/function refinement, function deselection, double-click captured source, Fresher → Reviewer selection preservation, chart relocation/duplication and bounded accessible node lists.
- Fresher screenshot reviewed at 1550 × 1000. This is not a readability audit of every chart or a large-repository/persona acceptance result. `smpp-mls` and the remaining persona pages still need that exercise.
- Large accessible graph outlines now scroll within 192px instead of stretching neighboring panels. G6 canvases retain their own readable-scale pan/zoom controls.

## Validation receipt — generated concepts checkpoint

- 172 application tests passed, including the new concept producer checks and a native-storage comparison against an independent oracle built from captured concept memberships and descendant edges.
- All 20 Rust storage tests passed; the native Node module was rebuilt. The type checker now permits explicit typed intersection narrowing of polymorphic traversals. Disjoint typed intersections and mixed-kind unions remain rejected. This avoids the work-limit failure reproduced by nested reverse relation predicates; no storage budget was increased.
- TypeScript and production build passed. 22 production-browser checks passed with zero browser errors, including adding and removing a concept while a file constraint remains active. Fresher's default graph is now `concepts`; declaration hierarchy stays available through the chart menu.
- Concept labels are preserved from ingestion, not validated as business truth by this test. Multiple parent edges, low/missing confidence, incomplete coverage and bounded graph/source evidence are disclosed. The representative captured member preview does not prove concept naming quality. Full `smpp-mls` domain review and all-persona readability acceptance remain outstanding.

## Validation receipt — captured entries and API scope

- 178 application tests passed, including a native Spring fixture with two uniquely linked routes, one ambiguous overloaded route, a bootstrap class, a private helper chain and an overridable virtual-call boundary.
- 24 Rust tests passed across storage and Java extraction; CLI and native storage module rebuilt. Java private/static/final helper discovery is order-independent and class-scoped, with overloaded names excluded from direct classification.
- TypeScript and production build passed. 26 production browser checks passed with zero browser errors, covering API selection, exact handler CFG/source, double-click preserving scope, API toggle and file-cell contributor selection, in addition to existing shared-selection checks.
- Bootstrap classes retain Class semantics. Unknown virtual calls are withheld by Facts-only and remain unknown when interpretations are enabled. Unresolved and ambiguous entries are stored and inspectable via retrieval, but the current symbol-bound entry tables exclude them and disclose this coverage gap. No claim of complete interprocedural runtime flow is made.

## Validation receipt — C4 and Architect orientation

- 183 application tests passed. Native storage tests verify nested application ownership against an independent captured-file symbol oracle, framework component/class membership, representative source, selection retention, and per-layout chart placement export/restore and rejection of invalid saved slots.
- 23 Rust storage tests passed; native module rebuilt. The new projection operates over captured stack records and declaration identities. No ingestion responsibilities or processes were added.
- TypeScript and production build passed. All 32 production-browser checks passed with zero browser errors, covering Architect core routing, shared file/container/component selection, captured component source, duplication, persona switching, and replacement canvas readiness.
- Architect screenshot reviewed at 1550 × 1000. QA exposed and fixed an asynchronous canvas replacement race: each render now owns an isolated DOM host, stale events are ignored, and the canvas stays hidden until the current semantic render is ready. Old renders cannot repaint the next chart's host.
- This is not the full Architect PRD. Deployment, captured external-image system projection, complete component dependencies, duplication/drift/risk scorecards, large-repository readability and `smpp-mls` domain acceptance remain pending. Auditor and Consultant retain legacy screen dependencies, so their shared legacy builders have not been deleted.

## Validation receipt — Architect REN-0 scorecard

- 185 application tests and 25 Rust storage tests passed; TypeScript and production build passed.
- Native storage tests cover measured zero, unknown measurements, scan-coverage requirements, generated/vendor filtering before paging, same-file duplicate pair counting and descending risk ordering.
- Production browser checks cover scorecard metric-cell selection and captured file source, alongside shared chart interaction and persona checks. Fixtures with quality measurements are explicitly synthetic captured test data; they are not evidence that the production ingestion pipeline collects quality.
- Current ingestion does not publish a quality artifact. The scorecard renders unknown states when none is captured, rather than borrowing demo data. Collector implementation, richer generated-file metadata and non-symbol file population coverage remain pending. See the Architect PRD ledger.


## Architect PRD composition checkpoint

The default now follows `architect.default`, not the earlier orientation layout.
All seven PRD panel IDs are registered; registration is not evidence that their
underlying engine requirements are complete. Timeline is an explicit unavailable
state. Duplication's literal clone/chord/FCA requirements, paired drift comparison,
layer/quality facets, production quality collection and ENG-11 remain pending.
Chart-only display hides explanatory text/controls by default while preserving
labels, active constraints, errors and unavailable states. See Architect PRD
current delivery boundary for the exact scope.


Validation: 189 application tests and 25 storage tests passed, with TypeScript
and production build checks. Native acceptance covers a manifest-only empty module,
declaration-based selection independent of imports, scoped recorded drift with
both document/code references, missing/zero quality values and shared populations.
Production-browser coverage includes the seven Architect slots, charts-first
startup, risk selection/deselection, captured source, opt-in details and constraint
preservation, plus existing cross-persona chart placement and selection checks.
Screenshots use explicitly synthetic captured QA metrics, not production quality
collection evidence. Domain acceptance on `smpp-mls` remains outstanding.
