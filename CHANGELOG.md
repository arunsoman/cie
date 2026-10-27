# Changelog

## Unreleased — big repositories open: sectioned artifacts, yielding project buttons (2026-10-26)

Opening "your own project" used one storage retrieval for the whole
`ui_bootstrap_v1` bundle. On large repositories (a 3.4k-file repo's report
alone is ~48MB) that crossed the retrieval policy's hard 16MB / 30s ceilings
every single time, and clicking **Show my project** / **Master this repo**
showed "Request deadline exceeded" — the project could never open at all.
Three changes, storage-first throughout:

- **The storage contract now serves a stored artifact one named section at
  a time** (`Output::Artifact { section, page }` in
  `cie-storage/src/retrieval/contract.rs` + `outputs.rs`): whole fields
  served whole, array fields as bounded windows behind a plain-offset
  continuation. A new `report_section` artifact document carries the
  window shape (`artifact_payload.rs`). Requests whose outputs are all
  artifacts also skip building the full entity dataset first
  (retrieval/mod.rs) — artifact calls were paying a full files+symbols
  dataset build they never read; warm artifact windows now cost
  milliseconds instead of six-plus seconds. The per-symbol language lookup
  in the dataset build is a single pass, not O(files × symbols).
- **`/api/analyze/result` composes its `report` from bounded section
  retrieves** (`web/src/lib/analyze-artifact.ts`): `describe` (1ms) names
  the captured report fields, array sections page at 2,000 records, and a
  byte-budget failure halves the window and retries the same cursor. The
  whole-artifact fetch stays as the last-resort fallback for engines that
  predate sections. The `extra` route's per-kind retrieves are unchanged.
- **"Show my project" / "Master this repo" yield to the landing view
  instantly** (dialog closes, no server round-trip while the button sits
  in view). `openStoredProject` (`web/src/lib/cie-store.ts`) lands the
  dashboard on Start or the Mastery journey with its own loading skeleton,
  loads the workspace through the same launch-restore path the page uses
  on every opening (workspace-memory's `restoreJobResult`), and backfills
  the heavier fields after first paint (fetchAndMergeExtra). A failure is
  an honest loadError on the landing page — never a stale fallback.

Also fixed: Atlas's storage probe no longer selects every captured symbol
(an unfiltered select makes the engine materialize the whole symbol list
before paging — past the 30s bound on large repos); the probe uses a
never-matching predicate, since its only need is the envelope's repository
identity. The module-duck-typed storage codes catch the dev-bundler case
where `StorageFailure` exists as two module instances.

Verified on the 3.4k-file repository: restore renders the landing page,
start/map/mindmap/what-it-does/usecases/trust render from the composed
workspace (715 usecases, 18,257 member functions), Atlas opens from a
fresh mount, small repositories keep the fast path (~150ms), and 28
storage tests pass.

## Unreleased — every question is answered by the configured LLM, grounded by the store's own tools (2026-10-25)

The chat's fast-path cascade is retired: a canned instant summary ("no AI
involved") answering "what does this project do?" is gone. Whenever a
query is made, the CONFIGURED LLM answers it, and two things now hold.

- **The answer's data comes from the store.** The server runs the model as
  a store agent over the same read-only tools the MCP server declares:
  `cie_describe` (opens a pinned context, teaches the query DSL),
  `cie_retrieve` (reads symbols, call records, read/write facts, evidence)
  — plus the new `cie_charts`. When neither the store nor the digest can
  find what the question needs, the system prompt now REQUIRES the first
  sentence to say plainly that it could not find an answer in the loaded
  data (and what would make it answerable) — a guess is never dressed as
  an answer. When the data IS found, the model also picks the
  complementary chart, returned in the one-JSON `{answer, chart}` contract.
- **`cie_charts` — the store names which charts are possible, so
  impossible picks cannot happen (`chat-tools.ts` + `chat-digest.ts`).**
  The digest now carries a structured `chart_possibilities` list, derived
  strictly from the loaded datasets (sequence only when call records
  exist; dfd only when read/write records exist; mindmap/tree only with
  the concept tree; count charts point at the datasets that actually
  recorded numbers). The tool returns that list verbatim, opens the
  pinned store context alongside it, and constrains `chart.kind` to its
  exact kind strings — the model picks among what the data can back, the
  app fills every box/arrow/lane from the records (store-fed kinds take
  `data: null`; the count kinds' rows must be numbers the model actually
  read). Tool-call loop hardening: per-result 20k and per-turn 60k char
  budgets return counted narrowing advice as tool results (never
  half-cut data), a 4-hop budget forces an honest one-JSON closer, and
  providers without tool support fall back to the plain digest call.
- **One chart, the whole stage (`stage.tsx` + `chat-chart.tsx`).** The canvas
  shows exactly one chart, so it now fills all of it: the camera wrapper
  sizes to the stage for the single-chart view and the chart host is
  flex-filled (`ChatChart fill`) instead of a fixed 640px strip that the
  fit-to-content camera then shrank or letterboxed; the chart's meta rows
  (rendered-by note, layout select, exports, outlines) collapse into one
  compact strip below the drawing. The builtin SDK's missing-`​.z-ai-config`
  failure is also named plainly now (a plain-words `LlmError` with the exact
  fix) instead of a generic 500.
- **Pinned:** the agent loop (tool-call hops, malformed-args teaching,
  budget closer), the three-tool set + `cie_charts` payload and its
  no-store teaching error, provider tools normalization (OpenAI
  tool_calls, Ollama object-args), and the possible-charts conditionals
  (chat-digest-pins.test.cjs). 218 tests pass; the one `rfc0023` G6
  render-host failure is pre-existing on a clean tree (environment, not
  this change).

## Unreleased — the asked kind is honored: a named-ask refusal now draws its kind, and the LLM writes the prose (2026-10-25)

*(Same-day follow-up: the chat's instant/hybrid panel paths below were
then RETIRED wholesale — see the newer entry above; the seam, focus and
kind-honoring machinery itself lives on — the model's picks are filled
from records by the same builders.)*

Ask CIE's concept seam used to answer a resolved-nothing ask ("draw the
flowchart of the operator" where no usecase claims "operator") with an
honest refusal — and no picture at all: the panel attached no chart to the
message and skipped the canvas, so the user's requested KIND never drew.
The refusal text stays exactly as honest as before; what changes is that
the ask now gets both the kind it asked for and a proper prose answer.

- **The refusal carries its focus and draws the asked kind (`concept-chat.ts
  → `draw-anything.ts` → `chat-panel.tsx`).** A `ConceptDrawResult` refusal
  now carries `focus` — the symbol ids the question actually matched — and
  the panel hands it to `buildConceptRefusalChart`, which draws the asked
  KIND centered on that focus, from the records only: a flowchart ask →
  the one-hop recorded call flow around the named symbol
  (`buildFocusedFlowSpec`, salience-capped at 30 boxes with the cut
  counted); a sequence ask → the cross-file message order
  (`buildProjectSequenceSpec`; when no cross-lane order honestly exists,
  the focused flow is offered *named as itself*, never as a sequence).
- **DFD and bare-sequence asks draw their own kind (`draw-anything.ts`).**
  An explicit `dfd`/`data flow diagram` ask now draws the recorded
  data-flow picture (`buildDataFlowSpec` — stores from the summaries'
  reads/writes plus schema files, the unresolved-reaching box as the honest
  external) instead of falling to the AI; with no data-store records the
  focused call flow is the closest honest picture, labeled AS ITSELF. A
  bare `draw the sequence diagram` (no story named — the concept seam
  never seizes) draws the whole-records sequence instead of the who-calls-
  whom stand-in. The UML ask and the flow ask now center on the question's
  focus (the tapped map bubble's file, else the longest symbol name in the
  question) instead of the plain store order.
- **The hybrid leg: the chart is the records' picture, the configured LLM
  writes the explanation (`chat-panel.tsx`).** After a refused concept ask
  draws, the panel runs the configured provider (builtin / ollama / a
  checked `llmUsable` config) on the SAME question, passing the instant
  refusal as an assistant turn so the model answers it knowing precisely
  what the records said and drew. The model's own chart pick (or the
  generic companion) is suppressed on this leg — the asked kind owns the
  canvas, and a follow-up leg never swaps it away. Scoped chats skip the
  hybrid (the scoped route reads only the selection's turns). The honesty
  footer stays on the instant bubble; the model's turn is visibly the AI's.
- **Pinned:** the refusal-focus contract (`concept-chat.test.cjs`: refund →
  `[6]`, the ambiguity and no-match refusals), the DFD/sequence/flow/uml
  kind-honoring + focus-centering (draw-anything.test.cjs on the real demo
  artifacts). 197 tests pass; the one `rfc0023` G6 render-host failure is
  pre-existing on a clean tree (environment, not this change).

## Unreleased — duplicates: jscpd era hardened; summary seeds extracted in phase 3 (2026-10-25)

The jscpd migration's leftovers are removed, the two-detector contract is
pinned by five scenario tests, and the FunctionSummary's file-local fields
(incl. never-before-populated `inputs`/`outputs`) are now extracted once in
the per-file pass that holds the tree.

- **The local half of the FunctionSummary is extracted once, in step 3.** The
  per-file pass already derived `LangFacts` from the parse tree; a new seed
  builder (`cie_summary::seed::summary_seeds`) groups a file's OWN facts into
  one `SummarySeed` per function-like symbol — declared parameters (inputs),
  distinct return shapes (outputs — a field declared on FunctionSummary since
  the start but never populated anywhere until now), non-handler raises,
  branch conditions (bounded like the stage), loop/failure/constant-return
  flags, `reads_input`/`entrypoint` evidence tags, and the callee texts
  extraction could not resolve — and `put_file` persists it inside the file's
  `CachedFacts` payload (serde default: older payloads simply fall back).
- **The dead cosine engine's remnants are gone from duplicates.rs.** The
  jscpd migration (commit 5267564) had left the old similarity machinery in
  the file without callers: the `LOW_FLOOR`/`ABORT_SLACK` constants were
  referenced nowhere at all, the dense `Bag`/`build`/`cosine` block survived
  only inside one self-referential test (a dead engine tested against a dead
  engine), and the `load_spans`/`load_kinds`/`load_ir_meta` loaders lived in
  the lib but were consulted only by the test oracle — all three now sit in
  the test module with their only consumer, the brute-force reference engine
  that production-vs-oracle parity still needs. Production reads `ir_hash`
  directly and never parses the `ir_vec` op-bag JSON.
- **The cosine era's storage is retired for real.** `ir_meta` no longer
  carries the `ir_vec` op-bag: the DDL drops the column for new stores, and
  `create_ir_schema` self-migrates pre-jscpd stores in place
  (`ALTER TABLE ir_meta DROP COLUMN ir_vec`; test
  `ir_vec_carrier_self_migrates_from_pre_jscpd_stores` builds a real
  old-shape store, verifies the column drop, row survival, and that
  two-column inserts land). The writer stops serializing the bag (`ir_hash`
  only, via `ir_hash_of`); the brute-force oracle in duplicates' test module
  now RECOMPUTES the op-bag from the persisted `ir_instructions` +
  `ir_edges` — independent of what the store carries — and the ingest dump
  tool prints only hash rows. `duplicate_gaps` stays for now (its one
  reader, `impact_duplicate_spec`, is load-bearing in the §23.1 spec shape);
  its values remain honest structural zeros. PRD §15 step 5 and §23.2 carry
  the retirement amendment (cosine ladder → jscpd token tiers, the three
  tier rows re-expressed as fragment coverage), the implementation record
  gains a slice-15 amendment row, and the impact-duplicate spec footer says
  "ir-hash and jscpd token tiers (§23.2 as amended)".
- **The two-detector contract is pinned by scenarios now.**
  `cie-polyglot/tests/duplicate_detection_two_detectors.rs` runs five real
  stores through the publisher's duplicate detection and pins which detector
  claims each row: verbatim copies are `ir-hash-match` Fact rows even though
  jscpd would also match them (canonical pairs are seeded first); a copy with
  one extra guard branch (same tokens, different IR op sequence) falls to
  jscpd's token-clone tiers and stays Inference; a pasted fragment inside two
  different bodies is the low tier; same logic in java and python is a
  cross-language Fact pair jscpd is structurally blind to (it compares within
  one format only); and class×method containment is excluded before any
  detector tier sees it.
- **The summary stage consumes seeds instead of re-deriving.** Fresh symbols
  (`behavior::compute_summaries_with_reuse`) prepopulate every file-local
  field from the seed and skip the local derivation loops; everything that is
  a property of the repository and not of a file stays with the stage as
  before: resolved `callees` + the `callers` inversion (repo-wide resolution
  ladder), reasoned `unresolved_callees`, receiver-gated db claims in
  `reads`/`writes`/`effects`, call-site records, transitive absorption, and
  `complete`. Seeds only exist for a file's own facts, so a single file can
  never impersonate a sibling's call graph — the same scope bar as the stack
  lens' capture. Old-payload stores take the unchanged fallback path — pinned
  field-for-field by a parity test; `is_entrypoint_call_text` moved to
  cie_summary so the seed builder and the fallback share one predicate.
## Unreleased — the stack lens rides the reads it already paid for; reuse keys go identity-stable (2026-10-25)

Two changes. One finishes the RFC-0019 §6.2 merge the codebase had already
half-shipped; the other restores TASK-030's exact-invalidation contract that
a fresh regression had broken, turning four pre-existing red tests green.

- **The manifest/config half of "step 5 into step 3" is real now.** Ingest's
  read phase already read every stack-input file (manifest, Dockerfile,
  compose, application.*) to hash it — and threw the bytes away, so the stack
  lens re-opened the same files from disk later. The walk now keeps the utf8
  bodies (`Walked.stack_input_sources`, same ≤16 MB + utf8 gates as the walk
  itself) and both stack-stage call sites hand them to
  `cie_stack::stack_report_stage_with`. The lens consults the capture FIRST
  (`CapturedInputs.read_input`) and falls back to disk for anything the walk
  did not capture — the report is identical either way, only the I/O changes.
  One scope bar enforced by the code, not caller discipline: the overlay can
  replace reads of stack INPUTS only — code-file reads go through a separate
  `read_code` that always hits disk, so no body can impersonate a source
  file (tested). The standalone `cie stack <dir>` path is untouched; the
  decorator half of the merge (frameworks::detect_file_tree off the live
  tree in put_file) was already shipped.
- **TASK-030 exact invalidation restored.** 9bce635 folded repo-global
  material into the per-symbol local-fact hash (serialized bindings with
  revision-local numeric owner ids, dep files' content hashes, repo-wide
  declaration profiles), so one file's edit poisoned untouched files' keys —
  four tests red (task_030 counters, acc27/task_084, US-004, US-045/US-094;
  all bisect-verified pre-existing at HEAD, failing from 9bce635 onward,
  green at a3b4ac4). The fix splits the key material the way the contract
  intends: a symbol's local-fact hash carries only its own identity-stable
  local facts (call rows name resolved callees by decl file + qual name,
  never numeric ids; spans/lines dropped — a positional shift is not a fact
  change; own bindings serialized without ids); file-level (file, dep-set)
  keys keep content-based dep invalidation unchanged; and the declaration
  basis (non-function symbols + class-scoped field bindings + type decls +
  imports, all stably serialized) moved to its own persisted per-file hash
  in `SummaryCacheEntry.decl_hash` — a changed basis poisons the file's
  prior symbol keys, so the dirty worklist recomputes them and cascades to
  transitive callers even when a caller's body is unchanged
  (changing_declared_receiver_invalidates_an_unchanged_method_body still
  passes). query_log seeds after store.py gains a line above; authorize
  recomputes exactly when query_db's facts move.
- **The four red tests, fixed at the seam 9bce635 actually intended.**
  acc27/task_084 pinned the retired FCA-as-UseCase shape; they now pin the
  new semantics honestly — business concepts are UseCase (source-backed,
  provenance `api:declared-route-v1`) or Facet, and every UseCase cites its
  route. US-004: the org-level merge lens had lost its label-bearers when
  FCA groups demoted to Facet — `OrgMember::new` and `run_org_ingest` take
  Facet beside UseCase/Aggregate/Entity (two repos sharing a concept is
  exactly the fact the org view claims). US-045/US-094: BB4 retired the
  naming-convention contract blocks; what survives structurally is the
  caller-side precondition candidate — a money/destructive action whose
  resolved callee is an auth-named primitive earns an InferredCandidate
  obligation citing BOTH the guarded action and its guard (the guard call is
  evidence, not a spelling).
- Gates: cargo workspace **617 passed / 0 failed** (was 613/4); clippy clean
  on touched lines; the new hand-off proofs live in
  `cie-stack/tests/captured_inputs.rs` (capture consulted with files gone,
  stale-copy preference over junk on disk, byte-parity with the disk-only
  lens, no impersonation of code files).

## 1.6.1 — the canvas tells the truth again: one navigation layer, real lanes, charts that draw (2026-10-23)

Three user-reported defects, root-caused and fixed at the seam where the
stage camera, the G6 host runtime and the concept surfaces meet.

- **Navigation is single-layered (the "charts move / zoom loses control"
  fix)** — every G6 graph shipped with its own `zoom-canvas` /
  `drag-canvas` / `drag-element` behaviors while the stage camera ALSO
  wheel-zoomed and drag-panned the wrapper. One wheel tick zoomed both
  layers at different anchors (the toolbar % tracking only one of them —
  repro: 100% → 162% while the graph had zoomed too); one drag panned
  both. In document views G6's wheel listener swallowed the page scroll
  outright. Now G6 never owns navigation anywhere: on the stage the
  camera is the one gesture → one layer → honest toolbar; in doc views
  the page owns the wheel; clicks and the selection surface are
  unaffected (browser-proven: the canvas buffer is byte-identical under
  drags and wheel zooms, and the wrapper pans by exactly the pointer
  delta). The admin grid loses its `zoom-canvas` for the same reason.
- **Concept sequences render as lanes, not a dagre circle** — the B3
  mapper emits lifeline elements + `message:` edges with a deliberately
  engine-neutral layout hint, and the generic dagre branch turned a
  one-class story into one giant circle filling the canvas. The host
  runtime now recognizes a lifeline-shaped spec and draws real lanes:
  header boxes top and bottom, dashed lifelines, one horizontal arrow
  per cross-class message ordered by the BFS depth that (as of this
  release) actually rides the edge in `fields.depth` — the mapper's
  caption promised it; now the data keeps the promise (ViewEdge grows an
  optional `fields` bag in viz-adapter). The disclosure names the truth
  — `layout sequence-lanes (Exact)` — and relayout honestly declines to
  re-dagre a sequence's semantic geometry.
- **The Usecases section draws the story first** — the section used to
  offer two buttons whose only effect was navigating you AWAY to the
  canvas. Now the story's diagram lives inline in the section
  (Flowchart/Sequence toggle, epistemic footer, disclosures, exports,
  text outline — the full gated canvas, section-sized), with "Open on
  the big canvas" as the explicit step, not a side effect of asking.
- **Drawing asks draw** — "draw a chart" / "draw a diagram" (a drawing
  verb + an unspecified chart noun) fell through to the AI, which
  replied with a clarifying menu and drew nothing; now they draw the
  project's structure map instantly, kinds named in the answer, while
  non-requests ("explain this chart") still reach the AI. A named ask in
  the user's own words resolves instead of refusing: "booking" reaches
  the story holding BookingService (camelCase/qual-name word tokens,
  longest token wins, ≥4 chars so "get"/"set" never decide), a story
  chain resolves to its narrowest story (most-specific-wins, the
  label-rule applied to membership), and a class word scopes through the
  A5 class cut — with every refusal still honest and counted.
- **The canvas chart host is big where the canvas is** — ChatChart grows
  a `big` prop (640px host) for the stage, so a 100-node call flow no
  longer autoFits into a 300px strip that reads as "nothing was drawn".

Suites: web node 66+ (concept-chat 18, draw-anything 5 new, B3 9,
polyglot 12+9, usecase 7, rfc0020 7, rfc0023 8), browser smokes concept
9/9, rfc0020 5/5, rfc0023 6/6, polyglot 8/8; viz-adapter 310/310;
tsc clean. The browser smoke re-anchored to the inline first: the
section draws the flowchart without navigating, the Sequence toggle
discloses `sequence-lanes`, and the big-canvas step is explicit.

## 1.6.0 — Phase D: the sweep — nested containment, fresh demo data, remembered stories (2026-10-22)

Phase D of the concepts plan — every deferred observation from A/B/C,
plus the pre-existing breakage the full regression surfaced. Nothing
brushed under the rug.

- **Same-file span containment is not duplication (engine)** — the
  second containment rule the kind rule's work named for follow-up: when
  one symbol's byte span strictly contains another's, in the same file,
  the pair is never a duplicate at any tier, whatever the kinds (a
  nested function inside its encloser, a nested class inside its outer
  class — the outer's op-bag contains the inner's by construction).
  Identical spans are not containment; different files never nest.
  Excluded pairs are counted (`DupReport.skipped_nested`), never shown,
  never hidden; the kind bucket stays disjoint. The per-symbol skip
  degrees come from a per-file (start asc, end desc) span sweep, both
  emission points exclude, the brute-force reference mirrors the rule,
  and the parity corpus now carries a nested file.
  Proof (scripts/check-nested-rule.py): the five-service sample is a
  true no-op (its 8 span-nested pairs are all in the kind bucket — 355
  rows before, 355 after); web/src/lib loses exactly 5 pairs, every one
  a real same-file containment in g6-engine.ts, zero collateral.
  62/62 cie-polyglot (552 workspace-wide).
- **The polyglot demo bundle re-ingests on the current engine (D1)** —
  the bundle predated the kind rule and still embedded containment
  pairs as duplicate-hemisphere facts. Regenerated: 37 specs, c4 and
  workflows byte-identical, every impact spec sheds exactly its
  excluded duplicates (0 additions, 0 changed elements), stale
  hidden-pair gaps drop to honest zero. All 1,493 evidence refs resolve
  into the refreshed report.json file list; bundle + hermetic fixture
  refreshed together.
- **The Usecases section reopens the story you chose (D3)** — the
  workspace's own last pick rides workspace memory (demo:kind for
  bundled samples, project:label for connected repos), restored on
  return — a preference, never a fact: it only changes WHICH story
  opens, never what the engine recorded, and a pick the rows no longer
  hold is ignored. The module fails soft everywhere (private mode,
  corruption, malformed entries); 7/7 unit tests + a browser step that
  leaves the section, returns, and finds the remembered story standing.
- **The atlas evidence panel is back** — the persona-atlas rework had
  deleted the `<CrossServiceEvidence>` mount, silently removing the
  RFC-0020 cross-service surface from the product. Restored verbatim.
- **One pick is one chip** — an atlas selection used to chip twice (the
  chart's raw-entry chip + the handler's `container:` chip + the
  store's selection chip under yet another id), and the Ask CIE dock
  mounted the chips row twice on top. Fixed at the root: a chart with
  an `onSelectElement` handler lets the handler own the chip; the
  handler's duplicate and the stray second mount are gone; the store's
  chip carries the 'picked from Atlas' provenance.
- **Browser smokes re-anchored to the current UI** — rfc0020 (outline
  based container selection, hydration-guarded Atlas click, 30s
  data-driven waits: 5/5 stable) and rfc0023 (bundled-chromium launch,
  stable input locator, 'clear all' release: 6/6).

## 1.5.0 — Phase C: the usecase-first UI — stories, and each one draws (2026-10-22)

Phase C of the concepts plan (A1–A6 engine truth + B1–B3 browser seam
landed earlier): the usecase-first projection the engine computed is now
browsable, and every story draws its own flowchart and sequence — from
the section, and from Ask CIE by name.

- **Demo data regenerated on the Phase-A engine** — all three samples'
  `report.json` now carries the A5 `usecase_view` (bankapp 40 records /
  29 with real broader-story chains; polyglot 7/6; spring 2/0, honestly
  short) and `concepts.json` carries the A1 induction chains. The
  polyglot bundle's 37 specs keep every evidence file inside the
  refreshed file list.
- **`UsecasesView`** (Explore → "Usecases — the stories, drawn") — reads
  the engine's own projection, re-derives nothing: chain words
  ("part of X → Y → Project"), intent badges with confidence and
  provenance, narrower stories, the resolved member table (name, owner,
  file:line — tap opens the code exactly there), and the class cut as
  counted chips. A projection-less artifact (pre-A5) says so honestly and
  still draws — the mappers need only the recorded extent.
- **`ConceptDiagramCanvas`** — the concept twin of the named-diagram
  canvas: same negotiation, same click→chip→source law, epistemic footer,
  gap disclosures (the no-internal-calls gap says members act
  independently), mermaid/png export, layout select. Gate refusal is a
  NAMED clause; nothing renders.
- **A third canvas slot** — `openConceptOnCanvas(conceptId, kind)` is
  mutually exclusive with the chat-chart and named-diagram slots; the
  stage titles it "Usecase diagram" and the footer states the membership
  law (membership is the engine's, never a label's guess).
- **Ask CIE draws the story** — "draw the flowchart of \<usecase\>" /
  "show the sequence diagram of \<usecase\>" resolve word-bounded (longest
  label wins, then member match scoped to the most specific named
  symbol), and the answer quotes the same counts the canvas renders. The
  refusal ladder is honest: a named nothing is refused by name with the
  counted menu; a member no usecase claims gets its own wording; a member
  in two stories is refused with both names, never merged; a bare "draw
  the flowchart" still belongs to draw-anything, and a bare "sequence"
  never fires.
- **Repair** — concept-diagrams.ts's `Boolean(useOwner)` never narrowed
  in tsc (B3's one latent typecheck error); explicit undefined check.
- **Tests** — web: concept-chat 14 (vocabulary + no-hijack, resolution,
  refusal ladder, counts-drift guard, real demo data) + concept-diagrams
  9 + concept-browser smoke 7 steps on the production server; polyglot
  suites 21/21; tsc and build clean.

## 1.4.6 — Phase B: a concept renders — flowchart + sequence ViewSpecs (2026-10-22)

B3 of the concepts plan (B1 store slot + guarded consumers and B2's
polyglot mapper landed earlier): every concept — a usecase above all — can
now be turned into two gate-ready diagrams from `concepts.json` +
`report.json` alone, no engine changes, no recompute of what the engine
already decided.

- **`web/src/lib/viz/concept-diagrams.ts`** — `conceptToFlowchartSpec` and
  `conceptToSequenceSpec`, pure functions of (concept, report). Shared
  prep: member symbols → report.symbols (names, byte spans, owning
  container) → report.summaries (resolved callees) → the call edges whose
  BOTH ends are inside the extent. Membership stays 100% the engine's
  (the concept's recorded `symbols`); labels name, they never decide.
  - Flowchart: nodes = member functions named by their recorded behavior
    labels, edges = internal calls (caller's bytes evidence each edge),
    entry points = members no in-extent caller reaches, isolates shown but
    flagged in `fields`.
  - Sequence: lifelines = the classes the members belong to (deduped,
    entries' classes first), messages = CROSS-CLASS edges ordered by BFS
    depth from the entries (depth rides `fields`), intra-class calls
    suppressed in v1 AND disclosed when they were the only edges.
  - Honesty guards: no internal call edges → the gap says "members act
    independently" (edges are never fabricated); extent members the
    tables cannot evidence are skipped and counted in a gap; both builds
    are byte-deterministic.
- **`message:` edge-kind family registered** (viz-adapter/src/engines/
  mermaid.ts) — sequence messages are `message:<text>`; the adapter's
  sequence detection accepts the family and renders the text after the
  prefix. This resolves the prior tension where the sequence detector
  demanded a bare `message` kind while the renderer used the kind as the
  message text (messages could carry no text). Pinned in TC-03.
- **Pre-existing breakage repaired** — rfc0023's G6 host mock predated
  f4656de's knownIds highlight sync (`graph.getNodeData()`); the mock now
  supplies the adapted data like the real v5 Graph. Suite green again.
- **Tests** — web: concept-diagrams 9 (flowchart content, sequence lanes/
  order/suppression, honesty gaps, determinism, mermaid adapt for both
  shapes) + all prior suites green (9/7/8/12); viz-adapter: 310.

## 1.4.5 — the usecase-first projection rides the artifact (2026-10-22)

A5 (+ the last A6 gate) of the concepts plan: the artifact now carries a
precomputed, read-friendly view of every usecase — the induction chain and
the resolved membership — so the web layer reads instead of re-deriving.

- **`usecase_view` on the snapshot** — one record per UseCase node (dag id
  order, byte-deterministic): the primary-parent `chain` (nearest-broader
  first, ending at the Domain root — the same walk `depth_of` and the
  invalidation wave speak), immediate `narrower` children, the extent
  resolved to `members` (name, qual_name, file, 1-based line, owning
  container), and `member_classes` — the class-like cut resolved with
  exactly the participating member ids (derived from `sym.parent`, the
  same rule pass 4/5 used to place the member). Pure function of
  (dag, store): no summaries, no stack, no labels — membership only.
- **Computed post-purge, pre-save** — the projection is derived after the
  TASK-028 purge pass, so its symbol ids speak the final (possibly
  renumbered) id space of THIS snapshot, and before `save_snapshot`, so
  the persisted artifact already carries it (verified by an ingest-level
  roundtrip test: member ids resolve through `snapshot.symbols`, chain ids
  through `concepts.nodes`, member_class participants ⊆ extent).
- **RFC-0024 tier bumps 2 → 3** — every folder's next ingest runs ONE full
  pass to rebuild the concepts tree and fill the projection, then the
  fast path holds again. Artifacts from before the field load as an empty
  vec (`#[serde(default)]`), never as an error.
- **Derived, never stored** — `usecase_view` lives on the snapshot, NOT on
  the ConceptDag: rollup/invalidate never see it, the six-key node store
  never persists it, and the DAG stays the only truth.
- **A6 closed** — the last uncovered gate: an `llm`-sourced label now has a
  provenance test (node stamps `label:llm`, exactly one inert
  `LlmCandidate` rule named after the proposal, extent/behaviors
  untouched — the LLM names the story, membership stays mechanical).
  Plus a projection suite: chain/narrower/members/class-cut content,
  purity (two builds serialize identically), rollup-stability, and the
  artifact roundtrip above.
- **E2E on bankapp** — 40 usecase records, 29 with real chains (depth > 1),
  clean evidence-fed labels; class cut is honestly empty (bankapp has no
  Python classes — its one "method" is a SQL routine owned by a Table,
  which pass 4 rightly does not class-ify).

## 1.4.4 — the labeler sees the story, not the tag words (2026-10-22)

A3/A4 of the concepts plan: pass-2 labeling is fed the cluster's actual
member evidence, and the LLM provider became reachable from every command
that produces concept labels.

- **Enriched pass-2 prompts** — `LabelFn` (and `cie-llm::Labeler::
  label_concept`) now receive the concept's member evidence: each member's
  name, tags and effects, bounded at 12 members in extent order
  (deterministic). A real provider can now name the story ("Transfer funds
  between accounts") instead of joining attribute words. Membership is
  STILL decided mechanically — the LLM names, it never decides (the
  offline provider ignores the evidence and returns the mechanical
  fallback; an `llm`-sourced label still binds only an inert
  `LlmCandidate` rule). Prompts are content-addressed, so existing caches
  miss once per concept and then behave as before.
- **Offline parser fixed** — the provider's `splitn(2, ':')` consumed the
  `LABEL:`/`EXPLAIN:` prefix before the branch checked it, so EVERY
  structured prompt fell through to the catch-all: concept labels came out
  mangled (`Concept:money:money movement`), symbol labels carried their
  kind suffix (`Cart Add Item:method`), and the EXPLAIN branch was dead
  code. Prefixes now match on the full prompt: concept labels read the
  mechanical fallback (`money movement`), symbol labels are clean
  (`Cart Add Item`), EXPLAIN produces its intended sentence. The labeler's
  cache namespace bumps to `offline-v2` so cached labels produced by the
  broken parse are invalidated; the EMBEDDING provider id stays
  `offline-v1` (vector_store pins it separately). Cached records now store
  the namespace as `model_id` so entries are self-describing.
- **Provider wiring completed** — `cie concepts` used to hard-wire offline
  labeling with no escape hatch; it now takes `--llm-provider`,
  `--no-llm` and `--llm-budget-usd` like `cie ingest`. `cie analyze`
  gains `--llm-budget-usd` (was hardcoded 1.0). Unconfigured external
  backends fall back to offline silently (unchanged `make_provider`
  behavior); budget exhaustion downgrades to offline mid-run; the
  persistent label cache makes LLM labeling a one-time spend per concept.

## 1.4.3 — the induction chain: usecases hang off their broader usecases (2026-10-22)

Structural change in the concept DAG (`cie-analysis` hierarchy), closing the
gap where the FCA lattice's own order was computed and then discarded: every
surviving usecase was flattened into a sibling of the Domain root, so the
climb "function → its usecase → the broader story above it" existed only as
implicit symbol lists.

- **The chain** — `fca::Concept.parents` (lower covers: minimal strict
  intent-supersets, i.e. the more specific concepts) is inverted into upper
  covers at the build site; a usecase's PRIMARY parent is now its broadest
  surviving upper cover (`FcaSubsumption`), falling back to the Domain root
  when nothing broader survives (only the top concept). Remaining covers are
  recorded as FcaSubsumption extra edges on the usecase itself — traceable,
  never mirrored into the cover's children, so an inferred 0.8-confidence
  node cannot drag a recorded node's weakest-link rollup (same rule as the
  member-class edges).
- **Latent staleness bug fixed first** — `build_lattice` computed cover
  indices BEFORE re-sorting the concept vec, leaving `parents` pointing at
  pre-sort positions. Harmless while nothing consumed the field; fatal for
  the chain that now reads it. The sort runs first;
  `covers_are_lower_neighbors_in_sorted_order` pins the direction (a cover's
  extent is strictly smaller, so it sorts strictly before the concept it
  hangs under).
- **Invalidation semantics improve with the topology** — the lazy dirty
  wave (TASK-082) propagates along primary-parent edges only; with the
  chain primary, invalidating a narrow usecase now correctly marks every
  broader usecase whose extent contains it.
- **Determinism holds** — the broadest-cover pick is ascending lattice
  index (extent desc, intent asc); same input yields the same DAG. The
  rollup and depth gates (`task_083`, `rfc0019_c4_tiers`) are unchanged and
  green; the new `fca_induction_chain` suite gates the chain itself, the
  diamond case, the rollup isolation, and the serialized artifact (parent,
  extra_edges and intent reach `concepts.json` — the web layer never has to
  recompute the climb).

## 1.4.2 — duplicates: a class and its functions are not duplicates (2026-10-22)

Behavioral change in `duplicates.detect` (PRD v2.0 §23), user-reported: a
pair where one symbol is a container kind (`class`, `trait`) and the other
a member kind (`function`, `method`) is never a duplicate — the member is
*defined inside* the container, so its op-bag is a subset of the
container's by construction and the similarity is containment geometry,
not duplicated behavior. The sharpest edge: a class whose only content is
one function shares that function's **exact** `ir_hash`, which used to
surface as a canonical **Fact** pair at similarity 1.0.

- **The rule** (`cie-polyglot/src/duplicates.rs`) — applied before every
  tier, at both emission points: the exact-hash grouping (canonical Fact
  rows) and the candidate scan (staging skips the pair entirely, saving
  the cosine). Sibling pairs keep comparing — two functions, function ×
  method, two classes, class × trait, across files included — where an
  identical body in the same kind is a genuine duplicate.
- **Honest accounting** — excluded pairs leave BOTH the shown set and the
  hidden residual: `hidden_gaps = C(n,2) − skipped − pairs`, per-symbol
  `hidden = n−1 − shown − skipped` (the skip count is exact arithmetic:
  every container pairs with every member exactly once, no walk needed).
  `DupReport` carries the new `skipped_kind_mismatch` counter; the CLI
  summary, `bench_phases` and `dup_dump` print it.
- **Brute-force reference mirrored** — the test reference implements the
  same exclusion, so `indexed_engine_matches_brute_force_reference` still
  pins rows, gaps, winner and now the skip count.
- **Verification** — 2 new unit tests (class × its own function: no row,
  skipped=1, gaps 0; the class-with-one-method duplicated across files:
  all 4 class × function combinations excluded while the two REAL
  duplicates — class × class, function × function — stay canonical Facts,
  every surviving row same-kind). Before/after `dup_dump` on the
  five-service sample: 581 → 355 shown pairs, 226 removed rows **all**
  container × member (8 further sub-floor hidden pairs reclassified;
  234 total excluded = the report's counter), 0 same-kind removals, 0
  additions, 0 kept-pair changes (`scripts/check-kind-rule.py`). Workspace
  531 tests green.

## 1.4.1 — Ask CIE draws the named diagrams (2026-10-22)

Chat integration: "draw the C4 architecture" typed into Ask CIE now opens
the engine's OWN artifact on the canvas — not the store-derived stand-in.

- **Intent module** (`web/src/lib/polyglot-chat.ts`) — conservative
  vocabularies for the four named kinds (C4/architecture, state machines,
  workflows, impact views); resolves the artifact from the bundle (a named
  machine or impact center wins over the first in stable order), maps and
  gates it, and writes the instant answer from counted facts (element
  count, footer certainty split, gap count). A kind the bundle doesn't
  hold is an honest "the pass named no X — what it did name: …" answer,
  never the stand-in drawn as if it were named. Deliberately NOT hijacked:
  bare "impact" questions (they keep the live `cie impact --symbol` path),
  "who calls whom" (keeps the report-based call flow), and bare "pipeline"
  (not a drawing ask).
- **Fast-path order** (`web/src/components/cie/chat-panel.tsx`) — the
  polyglot path runs after story/quality and BEFORE the mastery journey:
  the journey's `\bc4\b` prompt used to win and draw its file-table C4.
  When the bundle carries the kind, the byte-cited artifact wins; without
  a bundle every old path stands unchanged, and the journey still owns
  "the big picture" / "containers" asks so its chip remains earnable.
- **Canvas slot** (`web/src/lib/cie-store.ts`) — `canvasPolyglot` (the
  bundle key) rides beside `canvasChart`; the two senders are mutually
  exclusive; both clear on project switches. The stage renders it through
  the exported `ArtifactCanvas` (new `big` variant, 640px) — same mapper,
  same gate, same negotiation as the Named-diagrams section, with its own
  toolbar title, footer line and close button (`stage.tsx`).
- **Discoverability** — "Draw the C4 architecture" joined the composer's
  starter questions.
- **Tests** — `web/test/polyglot-chat.test.cjs` (12 tests: intent vocab,
  no-hijack guarantees, named/default picks, kind misses, empty bundle,
  gate refusal as a named result, fixture-backed gate re-runs); the
  browser smoke gained two steps (chat→canvas on the five-service demo;
  the honest stand-in fallback without a bundle).

## 1.4.0 — the web UI renders the polyglot layer: Named diagrams (2026-10-22)

The last mile of the PRD v2.0 seam, client side. The engine and the server
pipeline were already wired (1.3.1: the `polyglot` analyze step ran
`cie polyglot <dir> --viewspec all` and exposed `result.polyglotSpecs`) —
but both browser consumers dropped the bundle on the floor: the store had
no slot, no mapper existed, and no view could render a C4 diagram, a state
machine, a workflow or an impact view. All four gaps closed; the artifacts
now render through the same honesty law as everything else on the canvas.

- **Store slot + both result consumers** (`web/src/lib/cie-store.ts`,
  `web/src/components/cie/use-your-own-dialog.tsx`,
  `web/src/lib/workspace-memory.ts`) — `polyglotSpecs` is now a first-class
  slot on the zustand store; `setCustom` carries it in its extras; the
  connect-a-repo dialog and the launch-time workspace restore both read it
  from `/api/analyze/result`, guarded by `isPolyglotBundle` (a malformed
  bundle is an honest null, never a crash). `loadDemo` fetches an optional
  `polyglot.json` per sample directory — a sample without one loads fine.
- **The mapper** (`web/src/lib/viz/polyglot-spec.ts`) — maps each bundle
  spec onto the viz-adapter's `ViewSpec` shape: salience/fields defaults,
  synthesized edge ids, the EpistemicFooter counted from the final element
  AND edge sets, `{reason}` gaps → `{id, area, why}`, a ScopeAttestation,
  and semanticHash/renderHash recomputed with the adapter's own functions.
  Then the REAL gate runs on the mapped spec: a refusal is a named result
  (stable codes printed verbatim), never a rendered lie. One refused spec
  never takes the bundle down — each artifact gates independently.
  Producer-named layout strategies without a dedicated renderer ride the
  dagre baseline; an EDGLESS spec never gets a layering layout (dagre with
  zero edges never settles — the same rule `chartView()` already applies;
  found the hard way when the manifest-less sample's one-node C4 hung the
  first smoke).
- **The view** (`web/src/components/cie/polyglot-views.tsx`, stage view
  `polyglot`, Explore-menu entry "Named diagrams — C4, machines, flows") —
  an artifact picker in the producer's stable order (c4 → workflows →
  machines → impact), one gated canvas per artifact via the same
  negotiator/every other canvas view uses, epistemic footer + producer
  disclosures (counter-arguments, fog reasons, counted-not-shown gaps),
  text outline, Export Mermaid/PNG, and click → source panel at exactly
  the cited bytes. A workspace without a bundle gets an honest empty
  state, not a hidden menu.
- **Demo + fixture** — `web/public/data/polyglot/polyglot.json` is the real
  engine bundle over the five-service sample (37 specs: 1 honest single-
  Domain C4, workflows, 35 impact views; every evidence path verified
  against the demo's `report.json` file list); the same document is
  committed as `web/test/fixtures/polyglot-bundle.json` so the suite is
  hermetic.
- **Tests** — `web/test/polyglot-bundle.test.cjs` (9 tests: the real
  bundle maps and passes the gate spec by spec, ordering, labels, every
  mapper fill-in, named refusals, per-artifact isolation) and
  `web/test/polyglot-browser.cjs` (6 steps against a production server:
  picker count equals spec-count, c4/workflows/impact render with their
  epistemic footers, no client exceptions, honest empty state on the
  bundle-less banking sample). rfc0020 7/7, tsc clean; rfc0023's G6-mock
  unit failure pre-exists this change.

## 1.3.5 — polyglot duplicates: streaming CASE-literal bulk insert, redundant FK probes suspended (2026-10-21)

Same outputs, no threads — third round of the perf campaign, aimed at the
phase 1.3.4 left dominant. Measured on this repository (12,618,554 shown
pairs over 5,225 symbols), `duplicates.detect` 17.7 s → 6.7 s (2.7×);
the full `cie polyglot` pipeline ~39 s → ~28 s. Peak memory drops by the
~1.2 GB the old engine used to stage 12.6M rows before writing them.

- **Equivalence proven, not assumed** — the new `dup_dump` harness dumps
  `duplicates` + `duplicate_gaps` deterministically (ids, f64 bits,
  texts included); before/after runs over a frozen repo snapshot produced
  BYTE-IDENTICAL gzip streams (194,276,504 bytes, 12,623,779 lines each).
  529 workspace tests green, including the 500-bag bit-identity and
  full-corpus brute-force equivalence tests.
- **Rows stream, nothing staged** — the verify loop used to push a ~96-byte
  struct per shown pair (~1.2 GB at this corpus size) and only then open
  the transaction; rows now flow straight from the cosine ladder into the
  INSERT. Row order (and therefore every autoassigned id) is unchanged:
  canonical rows first in hash-byte order, then candidate rows in scan
  order.
- **512-row multi-row INSERT with CASE-literal texts** — one prepared
  statement per run binds only (a, b, similarity, outcome) per row and
  expands the outcome to tier/detection/epistemic/counter via `CASE ?4`
  literals living IN the statement; rusqlite copies every bound text
  (SQLITE_TRANSIENT), so moving four never-varying texts into the
  statement removes ~40M mallocs+memcpys per run. One step/reset cycle
  per 512 rows instead of per row; the tail chunk replays into an
  exact-size statement so ids stay sequential.
- **Redundant FK probes suspended for the bulk write (the 8.6 s lever)** —
  with `foreign_keys=ON`, SQLite probed three parents per row
  (symbol_a, symbol_b, canonical_symbol_id ≈ 38M B-tree searches,
  ~55% of the phase). Every referenced id is read out of
  `symbols`/`ir_meta` earlier in the same call, and those tables' own
  foreign keys already enforced it at write time, so the probes verify
  already-verified data. An RAII guard turns the pragma off before the
  transaction opens and restores the connection's original setting on
  every path (success, error, panic); table contents are identical.
  *Judgment call, easily reverted*: delete the `FkGuard::off` line in
  `detect()` if per-row enforcement is wanted anyway.
- **Merge-intersection cosine** — the per-pair dot product now walks two
  ascending dim arrays with two pointers instead of binary-searching
  each dim; same hit sequence, same accumulation order → bit-identical
  result, O(dims_a + dims_b) sequential instead of O(dims_a · log dims_b)
  random probes. Hash and shape equality inside the walk/ladder are now
  u32 group-id compares (interned once) instead of 32-byte memcmps and
  per-pair BTreeMap lookups; the walk marks partners with generation
  stamps instead of unmarking them.
- **Measured non-levers (left alone)** — page_size 8192/16384/32768:
  ≤5% (would change the tool's DB file format); cache_size 64 MB: made
  inserts slower; deferred FKs: the parent lookup still happens per row.
- bench_phases' TOTAL line measured the last phase, not the pipeline
  (stale timer variable) — fixed.

## 1.3.4 — polyglot ingest: one-pass shape stream + folder-bulk writes + normalize query fix (2026-10-21)

Same outputs, no threads — the sequel to 1.3.3, aimed at the phase that
1.3.3 left dominant. Measured on this repository, `ingest_and_lower`
62.3 s → 16.7 s (3.7×); the full `cie polyglot` pipeline ~81 s → ~38 s.
Frozen-corpus split (same 609 files, 1.64M ast nodes, 5,225 symbols):
`ingest_folder` 16.9 s → 12.8 s, `lower_all` 46.2 s → 4.1 s (11×),
`store_sources` ~50 ms unchanged.

- **Equivalence proven, not assumed** — a persistent-db harness dumps all
  12 tables (ids included) deterministically; before/after runs over a
  frozen repo snapshot produced BYTE-IDENTICAL dumps (1,975,480 lines).
  The new shape computation is additionally asserted node-by-node against
  the kept per-node brute-force reference over multi-language corpora
  (55 crate tests; 529 workspace tests green).
- **One-pass shape stream (ANL-06)** — `shape_hash` used to re-walk the
  whole subtree under EVERY node (O(subtree) node visits per node, with a
  child-Vec allocation and String rebuild at each). Now one pass per file
  builds the shared pre-order kind stream (kind + \u{1} per node, the
  exact byte sequence the old walk produced) and a stack close-marker
  hashes each node's contiguous slice the moment its subtree completes —
  DFS pre-order contiguity makes that slice exactly the subtree's stream,
  so every stored hash is bit-identical, hashed at memcpy speed.
- **Folder pass = one transaction, savepoint per file** — the per-file
  BEGIN/COMMIT fsynced the store once per file; the folder now writes
  through a single transaction with a SAVEPOINT per file, so a failing
  file still rolls back alone while everything before it stays committed
  (the exact old outcome, including stats counters and abort semantics).
- **Shared intern caches** — `node_kinds`/`node_text` are folder-global
  tables, but the caches were rebuilt per file and re-queried rows an
  earlier file had already answered; they are now shared across the
  folder (HashMap keyed memoization — first-encounter order, hence every
  id, unchanged). The qualifier chain for qualified symbol names moved
  from a per-child deep `Vec<String>` clone to a persistent Rc cons list
  (O(1) clone, identical oldest-first "."-join).
- **uast::normalize query rewrite (the 11× in lower_all)** — the
  `kinds_unmapped` count ran an `EXISTS` probe over all 1.6M `ast_nodes`
  for every `node_kinds` row (no kind_id-leading index exists) —
  rewritten as the single-scan `COUNT(DISTINCT kind_id) … WHERE
  uast_kind_id IS NULL`, the same set by definition. The node-tagging
  UPDATE gained a null-safe `IS NOT` change guard so NULL→NULL rows
  (unmapped grammar kinds) are not rewritten; final state identical.

## 1.3.3 — polyglot pipeline: mathematically faster detection + bulk persistence (2026-10-21)

Same outputs, no threads — the speed comes from algorithms, data
structures and write batching. Measured on this repository
(5,210 lowered symbols): the full `cie polyglot` pipeline previously
could not finish inside 10 minutes; it now completes in ~81 s.
On the cie-polyglot subtree: duplicates 3.44 s → 30 ms (113×),
ingest+lower 3.68 s → 1.00 s.

- **Duplicates engine (§23)** — rewritten as threshold all-pairs cosine
  over a DENSE op-bag space: one shared vocabulary (sorted op names →
  u32 dims; ascending dim == the old BTreeMap's string order, so every
  similarity is BIT-IDENTICAL to brute force), norms computed once per
  symbol instead of twice per pair, exact-hash grouping (O(n)) emitting
  canonical Fact rows with no cosine, and an inverted index whose scan
  walks each vector's dims in descending weight with a Cauchy–Schwarz
  remainder abort — any pair below the 0.40 floor is provably
  unreachable through the unscanned tail, so sub-threshold pairs are
  never enumerated. Equivalence is enforced in-test against the kept
  brute-force reference: identical rows (similarity bits, tiers,
  detection, counter-arguments, canonical winner), identical totals,
  identical per-symbol hidden counts, over a corpus with exact dups,
  near dups, weak pairs and a hidden residual (54 crate tests).
- **Hidden pairs are counted, not materialized** — the old engine wrote
  one row PER sub-floor pair (O(n²) rows read only as per-symbol
  counts); they now live in `duplicate_gaps(symbol_id, hidden_count)`,
  one row per symbol, and the impact/duplicate spec reads the same
  number from there. On this repo: ~1.02M hidden pair rows never
  written.
- **Bulk persistence** — the §23.3 canonical winner is computed up
  front and written with each row; the whole output (12.5M shown rows
  on this repo) goes in ONE transaction instead of one auto-commit
  INSERT per pair, replacing the old post-hoc full-table UPDATE per
  family member (every pair got a row, hidden included, so the
  brute-force family walk always reduced to the global argmax —
  preserved exactly and documented).
- **Ingest/lowering** — one transaction per pass instead of per-row
  autocommit; the call-name index is loaded once per run instead of
  re-scanning all symbols PER FILE (was O(files × symbols)); one
  span→node index per file replaces a full-tree DFS per symbol (×2)
  and per call (was O((symbols+calls) × nodes) per file); `AstIdMap`
  lookups no longer allocate a kind String at every ancestor level;
  prepared statements across the ast/symbol/block/instruction/edge/
  call/meta write loops.
- Known remainders (named, not hidden): ingest is now the dominant
  phase (~61 s of 81 s — per-file ingest transactions, and shape_hash
  walks each node's whole subtree); the §23 cosine treats every op as
  equally informative, so on this repo 12.5M of 13.6M pairs land above
  the floor — correct per spec, but the signal wants IDF weighting,
  which would change similarities and is a semantic decision, not a
  speed one.

## 1.3.2 — `cie sample` git isolation: refuse targets inside a host repo (2026-10-21)

Retires the incident warned about in 1.3.1: running `cie sample
demo-sample` inside an existing repository committed the sample's 5
fixture commits into the HOST repo, and the first commit's repo-wide
`git add -A` swept the host's uncommitted work into it.

- **Root cause** — `git rev-parse` walks UP the tree, so a fresh empty
  subdirectory of any repository answered "already inside a work tree";
  the `git_ok`-gated init was skipped and every add/commit silently hit
  the enclosing repo's index, refs and object store.
- **Ownership guard** — before anything is written, `rev-parse
  --show-toplevel` is compared with the canonicalized target: a path
  that sits inside a repo that is not its own is refused with an
  explicit message. The probe only fires when a repo still has to be
  created, so "no git binary" and "inside someone else's repo" name
  different causes.
- **Physical isolation** — every git invocation (init, add, diff
  --cached, commit, rev-list) is pinned to the sample's own absolute
  `GIT_DIR`/`GIT_WORK_TREE`, so even a future probe bug cannot touch
  another repository's index, refs or objects.
- Regression test replays the exact incident shape (fresh empty subdir
  of a host repo) and asserts the refusal message, unchanged host
  commit count, clean host status, and an untouched target; the happy
  path now also asserts the sample is its OWN toplevel. 145 workspace
  suites green.

## 1.3.1 — UI-reuse seam: byte-span ViewSpecs, bundle mode, C4 spec, pipeline step (2026-10-21)

Closes the four gaps between the polyglot layer's §25 artifacts and the
already-standing UI substrate (viz-adapter negotiator + 5 engines + the
web analyze pipeline). One commit per gap, pushed as it landed.

- **Gate parity + byte-span evidence** — every element/edge evidence is
  now the `{file, start, end}` ref `viz-adapter/src/types.ts` requires
  (machines anchor at their symbol span, workflow async boundaries cite
  their entry symbol, impact/duplicate elements cite their own spans);
  the producer-side `validate()` mirrors the TS gate clause-for-clause
  (duplicate ids, span-checked evidence, claim/counter/fog, RFC-0024
  bodyLines/alsoEvidence, the 2 000-element cap) — the drift the mirror
  existed to prevent is closed.
- **`--viewspec all`** — one bundle document (`spec-count`,
  `impact-centers-truncated`, `specs{}`), impact specs per
  duplicate-family center capped at 50, cap counted.
- **C4 spec** — the §11.7 DAG + §20.3 relationships as the `c4`
  ViewSpec: containers cite their manifest FILE, components the symbol
  they were detected at, edges the closed §22.3 reason set, over-cap
  stores truncate class nodes salience-first with the count stated.
- **Pipeline step** — `analyze-jobs` runs the bundle as its own step
  (`polyglot`, between stack and cochange), type-guards it, exposes
  `result.polyglotSpecs`; no new catalog rows (the artifacts are
  instances of already-enumerated chart kinds).
- 52 polyglot tests, 145 workspace suites green; web + viz-adapter
  typecheck clean. Known pre-existing: `cie sample <path>` inside an
  existing repo commits its fixture history into the HOST repo (its
  repo-wide `git add` also sweeps uncommitted work) — sample into a
  path outside any repository until the generator isolates its git ops.

## 1.3.0 — PRD v2.0 polyglot layer (2026-10-21)

The polyglot architecture lands as `cie-polyglot`: SQLite AST/UAST/IR
schema fed by real tree-sitter parses, framework + cross-language
detection, C4 containers/components into the §11.7 concept DAG,
three-tier state machines with epistemic encoding, workflow DFS with
async-boundary Fog, ir-hash/cosine duplicate tiers with canonical
marking, §25 ViewSpec production with a producer-side gate mirror, and
the §45 runtime evidence layer with the OTel join. One commit per PRD
slice, all pushed to `main` as they landed. Record:
`docs/implementation/prd-v2.0.md`.

- 50 crate tests green (`cargo test -p cie-polyglot`), CLI surface
  `cie polyglot` + `--otel` + `--viewspec` (18 cli tests).
- Acceptances landed here: #11 groundwork, #28 guard, #32, #33, #34,
  #75, #76, #77, #78, #79, #80 (+ the §23.2 tier ladder and §19.3
  disclosure rules enforced by audit()).

## 1.2.0 — CIE-US opens close + plain-English web dashboard (2026-10-18)

Four planned user stories whose named gap became engine work, one commit per
story, each with a dedicated deep check; plus the first user-facing UI.
Registry after the batch: **113 delivered / 1 partial / 33 planned** (100
stories engine-checked).

- **US-007 (delivered)** — `cie-ingest::sample` + `cli:sample`: on-demand
  sample repository with REAL dated git history (deterministic demo app;
  marker-guarded `--force` regeneration; refusal rules; missing git named).
- **US-087 (delivered)** — `cie-ingest::cochange` + `cli:cochange`: co-change
  coupling from real git history — renames resolved to final canonical names
  (transitive chains), bulk-edit commits excluded with the effect counted,
  per-edge support counts + supporting commit ids + cross-directory flags,
  labelled historical coupling (never a static/runtime dependency claim).
- **US-062 (delivered)** — `cie-analysis::deps` + `cli:deps`: dependency
  manifests (package.json / pyproject.toml / requirements.txt / Cargo.toml /
  go.mod; unparseable named, never guessed), usage evidence from
  adapter-recorded ImportFacts (other ecosystems carry a named-gap note —
  never a fabricated zero; unresolved packages are NOT called unused), and
  upgrade what-if whose compatibility is ALWAYS `Unknown`.
- **US-098 (partial BY DESIGN)** — `cie-analysis::deps::licence_report` +
  `cli:deps --licences`: licence obligations read as written, undeclared
  licences NAMED (provenance unavailable ≠ guessed), honesty statements in
  the report itself; advisory/vulnerability feeds stay unplanned offline
  (fabricated feed data is refused).
- **web/** — plain-English Next.js dashboard ("Your code, in plain words"):
  map (ViewSpec as layered SVG), promises, findings by urgency, trust page;
  load-your-own-project flow; every engine term translated
  (`web/src/lib/plain-words.ts` is the language policy).

## 1.1.0 — CIE-US partials close (2026-10-18)

All 45 `partial` user-story dispositions implemented and flipped to
`delivered`; the honest registry now reads **110 delivered / 0 partial /
37 planned** (the 37 planned keep their stated reasons). One cluster per
commit, each with deep checks (`cie-stories::checks::s11_closes`) and the
permanent honesty labels:

- **git-history archaeology** — `cie-ingest::history` (SystemGit seam:
  log/numstat/blame porcelain) + `churn` (exclusion ledger with re-ranking
  effects, rank stability, co-change coupling). Closes US-037/086/060.
- **runtime evidence feeds** — `cie-fault::trace` (python/rust/java/
  javascript trace parsers, fingerprinted exceptions inbox, LDFI suspects
  with frame evidence) + `next_steps` (ranked discriminating checks with
  concrete blockers). Closes US-032/039/064/069.
- **static concurrency** — `cie-analysis::concurrency` (lock sites,
  wait-for ordering risks with call paths, interleaving windows citing
  both statements, witness proposals that never claim certificates).
  Closes US-040/079/080.
- **composite analytics** — `cie-analysis::risk` (change-risk terrain with
  disclosed weights and neutral missing data), `cie-drift::causal`
  (chronology vs causal chain, alternatives visible). Closes US-046/071.
- **interface timelines** — `cie-incremental::timeline`. Closes US-089.
- **measurement protocol** — `cie-evidence::workload` (named workloads;
  measured-vs-potential by construction) + `readiness` (external evidence
  verdicts, readiness rollup with waivers-that-disclose, capacity sandbox
  refusing unsupported domains, calibration states). Closes US-074/085/
  100/102/119/120.
- **MCP server seam** — `cie-server::mcp` (JSON-RPC 2.0 over stdio,
  read-only declared-scope tools, budget-exhausted as explicit errors,
  honesty envelope). Closes US-134.
- **what-if simulation** — `cie-incremental::whatif` (remove/split dry
  runs; structural vs inferred consequences). Closes US-099.
- **query + capability layer** — `cie-llm::readings`, `cie-analysis::{
  query, endpoints, alternatives, impact_dims, steps}`. Closes US-009/
  010/024/034/052/059/109.
- **steering + collaboration** — `cie-incremental::steer`, `cie-claims::
  share`, `cie-drift::conflicts`, `cie-incremental::{attrib, diffgen}`.
  Closes US-066/091/107/113/114.
- **overlap + delivery oversight** — `cie-concepts::overlap`,
  `cie-evidence::scope`. Closes US-106/127/128.
- **host interaction layer** — `viz-adapter/src/interaction.ts` (LOD +
  settle, hover lens, gesture→proposal, measured layout quality, keyboard
  parity contract, announcements, never-colour-alone audit, theme tokens;
  15 new vitest cases, 274 total). Closes US-022/023/028/030/140/141/142/143.

Fidelity fixes surfaced by the batch: the miniature incremental world now
ingests repo content before dep recomputation (per its documented two-pass
contract) and the `pending_callees` orphan invariant key/caller semantics
were corrected.


## 1.0.0 — Full Task Matrix (2026-10-17)

All 84 instructor tasks implemented to their validation contracts; the 21 tasks
remaining after v0.3's first half (039/040/041/043, 052/053/054, 058, 067, 027, 071 and
the v1.0 row 024/047–051/057/059/060/062) landed one commit each. The completion
record lives in `docs/TRACEABILITY.md` §5; the completion record supersedes the §3
disposition rows.

### v0.3 closeout (behavior layer)
- **039/040** naming adapter (lexicon verb analysis, FR-10 immutable labels, FR-28
  LLM-off fallback) and Engler belief inference (inconsistent checks, cleanup
  pairing, co-occurrence) combined in a Kremenek factor graph — every output an
  `InferredCandidate` through the guarded registry (§8 rule 4).
- **041/043** protocol mining (k-bounded automata, Houdini refutation with an
  append-only content-addressed log) and bounded predicate checking (requires-DSL,
  path-condition conjunction, Unknown/TimedOut never clean).
- **052/053/054** FR-15 recovery modeler (RefundCalled ≠ Refunded), LDFI fault
  injection (minimal-faults-first, NoLineage → Unknown), Yuan et al. error-handling
  detectors (Safety/Liveness distinct).
- **058/067** deletion-correctness proptests (clean-recompute oracle after every
  step) and the §17.2 parallel folder walk (gitignore, shebang detection, rayon,
  per-file error isolation).

### v1.0 (hardening)
- **047–051** budgeted SMT, CEGAR, SLAM product checking, bounded symbolic
  execution with repro-test emission, and §14.9 counterexample packaging with a
  reachability-validated shrinker.
- **024/027** sled (default) + redb (feature) KV backends behind one `KvStore`
  trait, BE key codec, L0 caches, and the §15.4 latency-gated typed tables
  (50 ns / 30 ns / 5 µs, batched p99 timing).
- **057/059/060/062** differential relation maintenance, HNSW k-NN store,
  zero-copy rkyv+memmap2 snapshots, L3 columnar zstd archive with
  byte-equivalent restore.
- **071** PDF text-layer extraction and the honest OCR seam.

## 0.2.0 — Contract Fidelity (2026-10-17)

Every instructor task marked *partial* by the 84-task cross-check is implemented to its
validation contract. One commit per task; the disposition matrix lives in
`docs/TRACEABILITY.md`, the plan in `docs/V0_2_PLAN.md`.

### Core (TASK-002..005, 007..010)
- `hash()` / `BLAKE3_EMPTY` exports; lock-free `RevisionAllocator` with `meta.json`
  restart recovery (`Store::allocator`/`commit_rev`); injectable `ClockSource`.
- §14.1 identity newtypes `SymbolId/OperationId/ValueId/ResourceId/FileId` (serde-transparent)
  and the FxHashMap `Interner<T>` / `StrInterner` / `InternedStr`.
- §9 `AnalysisInput`/`AnalysisOutput` protocol, `CieError` taxonomy (+`Cycle`),
  `CI_TOLERANCE = 3.0`, `#[perf]` convention reference test.
- `LangId` closes over the ten §16 v1 languages; `LanguageTable::v1()`.
- Typed relation substrate: `RelKind` (10 kinds), `RelationGraph` (petgraph StableGraph,
  (from,to,kind) dedup), `CsrGraph` freeze with rkyv snapshots + 1M-edge perf gate,
  Ferrante–Ottenstein–Warren `Pdg` with bounded backward slicing.
- `TypedFacts` aggregate with deterministic idempotent `dedup()` and
  `check_integrity() → CieError::DanglingReference`.

### Storage & incremental (TASK-028, 030, 056, 073, 078)
- Tombstones (`file:{path}:tombstone`) with explicit snapshot purge (symbols, summaries,
  findings, orphaned call edges); re-add clears.
- Per-file summary reuse keyed `(blake3(file), blake3(dep set))` with persisted
  `summary_cache`; symbol-level dirty worklist invalidates exactly the transitive
  caller set (semantic local-fact hashes). Includes the FR-4 fix: adapter AST
  traversal no longer sorts by unstable `node.id()` (byte-stable evidence).
- Persisted `ClaimStore` (`claims.json`) with (doc, span, kind, modality) provenance
  and blake3 dedup keys.
- Concept persistence split into the six `concept_node:{id}:*` keys with
  reason-carrying children pairs and a `:rollup` aggregate view.

### Languages (TASK-011)
- Semantics tables as adapter data: `DispatchKind`, `NumericSemantics`
  (`OverflowPolicy`/`CoercionPolicy`/`Precision`), `FrameworkRule`; Python vouches
  Dynamic + ArbitraryPrecision. `LanguageRegistry::by_extension` + `new()`;
  FR-18 one-line extension guard pinned (acceptance #9).

### Drift & documents (TASK-071..077)
- `.rst`/`.adoc` line-based and `.html` tag-strip ingestion via `run_drift`.
- Drift re-run cache keyed (file-index code hash, docs hash) — pipeline re-run ≤ 5%
  of cold (acceptance #24, release-gated); acceptance #17 (≥90% non-Unknown) and
  #25 (≥60% LLM-off) pinned.
- Runnable acceptance harness: locked `ACCEPTANCE_ITEM_COUNT = 26` (24 green / 2
  deferred) with the authorized TASK-084 extension to 31 (29 green / 2 deferred).
- §15.4 perf regression suite for shipped subsystems (symbol lookup, CSR scan,
  interner lookup; release-gated × `CI_TOLERANCE`).

### Concepts & CLI (TASK-079..084)
- `ConceptDag::try_link_child` with ancestor-chain cycle rejection (`CieError::Cycle`).
- Four binding sources (`Explicit`/`InferredCandidate`/`Verified`/`LlmCandidate`);
  `LlmCandidate` inert by construction (`promote_to` refuses decisive targets).
- `cart` fixture (three-level hierarchy) + binding-source coverage tests.
- `cie concepts --max-depth N` (depth-limited view across md/json/dot/mermaid).
- Parent-edge-only invalidation with lazy `rollup_dirty` (dirty rollup == clean
  recompute, acceptance #31).

### Recovery (TASK-054 subset)
- `IgnoredErrorValue` findings (throwaway-assigned call results) and bare-except-
  without-re-raise recovery gaps (`sec/SEC-004`/`SEC-005`).

### Known deviations
- 6-crate workspace instead of the §10 30-crate scaffold (gate D-1 → v0.3), documented
  mapping in `docs/TRACEABILITY.md`.
- `SourceLocation` keeps rendered line/col + path (byte-span fields arrive with the
  §14.2 shapes in v0.3); `RevId` stays the §14.1 alias per TASK-002's explicit constraint.
- Perf gates bind on release builds only.

## 0.1.0 — Foundation (2026-10-16)

Initial release: 6-crate workspace, Python-deep + JS/TS/Go/Rust adapters, behavior/
conformance/security/testing analysis, FCA concepts, drift pipeline, offline-first LLM
layer, seven-command CLI, 30 tests green.
