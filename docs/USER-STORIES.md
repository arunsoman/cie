# User Stories — engine capability layer (CIE-US)

The product user-story catalogue (`Product_User_Stories`, 2026-10-18 — 147 stories,
US-001..US-147, 20 sections, 28 Appendix-A source families) is implemented as the
`cie-stories` crate: the catalogue is machine-readable data, every story is
dispositioned against the engine's real capabilities by an honest registry, and
engine-verifiable obligations are checked by named deep checks that run real
engine code on generated fixtures.

## Honesty rules (enforced by tests, not prose)

- `delivered` names capability refs (`cie-<crate>::<item>`, `cli:<cmd>`,
  `host:viz-adapter/...`) — a story is never marked delivered because a name
  looks right;
- `partial` names its gap in the note — partial results stay visibly partial;
- `planned` carries no capabilities and says why;
- the coverage report shows counts, never a percentage — a percentage would
  imply a completeness the catalogue refuses to claim;
- deep checks run the REAL engine (ingest, drift, viz producer, fault planner,
  record store) against generated fixtures — no mocks, no name-matching;
- a failing deep check is a first-class verdict (`FAIL`), never a silent skip.

# User-story coverage (engine capabilities)

Dispositions count *engine-level* delivery. `partial` results stay visibly partial; `planned` items are absent, not hidden. Percentages are deliberately not shown — they would imply a completeness the catalogue refuses to claim.

| Section | Delivered | Partial | Planned | Total |
|---|---|---|---|---|
| Repository access and orientation | 5 | 0 | 2 | 7 |
| Asking questions and working with answers | 9 | 0 | 1 | 10 |
| Interactive visual exploration | 13 | 0 | 0 | 13 |
| The visual catalogue | 17 | 0 | 2 | 19 |
| Behaviour, concepts and business rules | 7 | 0 | 0 | 7 |
| Architecture, dependencies and impact | 5 | 0 | 2 | 7 |
| Debugging and investigations | 11 | 0 | 0 | 11 |
| Runtime, concurrency and data consistency | 3 | 0 | 4 | 7 |
| Performance and technical health | 3 | 0 | 3 | 6 |
| History, collaboration and knowledge | 5 | 0 | 1 | 6 |
| Security, permissions and supply chain | 4 | 0 | 1 | 5 |
| Counterfactuals and digital twin | 4 | 0 | 1 | 5 |
| Prompt-to-feature discovery and planning | 3 | 0 | 4 | 7 |
| Feature implementation and inspection | 2 | 0 | 4 | 6 |
| Validation and safe delivery | 3 | 0 | 7 | 10 |
| Release and delivery oversight | 2 | 0 | 3 | 5 |
| Editor and agent integrations | 2 | 0 | 1 | 3 |
| Privacy, models and product control | 5 | 0 | 0 | 5 |
| Accessibility and interaction parity | 4 | 0 | 0 | 4 |
| Honest operation | 4 | 0 | 0 | 4 |

**Totals** — delivered: 112, partial: 0, planned: 35, total: 147

## The partials close (2026-10 batch)

All 45 `partial` dispositions from the original registry were implemented and
flipped to `delivered`, one engine capability per named gap:

| Cluster | Stories | Where |
|---|---|---|
| git-history archaeology | US-037, US-086, US-060 | `cie-ingest::history` + `cie-ingest::churn` (SystemGit seam, exclusion ledger, co-change) |
| runtime evidence feeds | US-032, US-039, US-064 | `cie-fault::trace` (multi-language parser, fingerprinted inbox, LDFI suspects) |
| static concurrency | US-040, US-079, US-080 | `cie-analysis::concurrency` (lock sites, wait-for ordering risks, interleaving windows, witness proposals) |
| composite analytics | US-046, US-069, US-071 | `cie-analysis::risk`, `cie-fault::next_steps`, `cie-drift::causal` |
| interface timelines | US-089 | `cie-incremental::timeline` |
| measurement protocol | US-074, US-085, US-100, US-102, US-119, US-120 | `cie-evidence::workload` + `cie-evidence::readiness` |
| agent seam | US-134 | `cie-server::mcp` (JSON-RPC 2.0, read-only, budgeted) |
| what-if simulation | US-099 | `cie-incremental::whatif` |
| query + capability layer | US-009, US-010, US-024, US-052, US-059, US-109 | `cie-llm::readings`, `cie-analysis::{query,endpoints,alternatives,impact_dims}` |
| steering + collaboration | US-066, US-091, US-107, US-113, US-114 | `cie-incremental::steer`, `cie-claims::share`, `cie-drift::conflicts`, `cie-incremental::{attrib,diffgen}` |
| overlap + delivery | US-106, US-127, US-128 | `cie-concepts::overlap`, `cie-evidence::scope` |
| host interaction layer | US-022, US-023, US-028, US-030, US-140, US-141, US-142, US-143 | `viz-adapter/src/interaction.ts` (vitest-covered) |
| transaction steps | US-034 | `cie-analysis::steps` (adapter-classified async hand-offs) |

The 36 `planned` stories remain planned with their reasons (hosted-source
pull, conversational steering, live model training, MCP-over-network
transports beyond the stdio seam, and similar) — no silent upgrades.

## The opens close (v1.2 batch)

Planned stories whose named gap became engine work, flipped one commit at a
time with a dedicated deep check each:

| Story | Gap closed | Where |
|---|---|---|
| US-007 | on-demand sample repository (real dated git history, deterministic, marker-guarded regeneration) | `cie-ingest::sample` + `cli:sample` |
| US-087 | co-change coupling from real git history (canonical rename resolution, bulk-edit exclusion ledger, support counts + supporting commits + cross-directory flags) | `cie-ingest::cochange` + `cli:cochange` |

## Deep verification — 98 stories engine-checked

Every check is a `fn() -> Result<(), String>` in `cie-stories/src/checks/`,
dispatched by `cie stories verify --id US-xxx` and executed in bulk by the
`all_deep_checks_pass` test (failure names the story and the honest error).

### Repository access and orientation

- **US-001 — Connect a repository**: connecting identifies the studied revision; a summary never implies complete analysis (coverage metadata travels with every snapshot).
- **US-002 — Know what the product understands**: the product states what it understands — supported languages are analysed, unsupported ones are skipped openly (never guessed).
- **US-003 — Keep understanding current**: understanding stays current — incremental re-ingest identifies the changed file, revisions advance, and the old answer remains retrievable under its own revision (never relabelled as current).
- **US-004 — Work across several repositories**: several repositories from one place — merged concept view keeps per-repo sources (identical names stay separate, conflicts named).
- **US-006 — Watch and control background work**: background work control — budgets + cancellation checkpoints. An exhausted LLM budget refuses cleanly (no silent overrun), and a zero wall-clock budget ingest completes without panic and reports its coverage state honestly (complete flag + notes, never silent truncation).

### Asking questions and working with answers

- **US-008 — Ask a code-grounded question**: answers are evidence-assembled — summaries exist and carry the call evidence (checkout → charge), so any answer can cite code.
- **US-011 — Combine several questions into one analysis**: multi-part analysis runs as read-only steps with per-step isolation — a broken file never aborts the healthy files.
- **US-013 — Choose the audience and level of detail**: the same facts render at multiple levels — JSON roundtrip and SARIF export preserve the finding population exactly.
- **US-014 — Inspect and challenge a claim**: intended vs actual — document claims map to code symbols and get verified statuses with evidence (mismatches classified, not averaged).
- **US-015 — Distinguish proven, inferred, hypothesised and unknown**: how-it-is-known travels with the data — requirement modality on claims, honest Unknown on timeout (never Implemented), and an explicit unknown conclusion is storable while fabricated clean results are not.
- **US-016 — Keep several answers side by side**: answers persist as rev-scoped, content-addressed records that survive reload; time-travel reads never show newer facts as current.
- **US-017 — Recover when something goes wrong mid-answer**: failures preserve progress honestly — the LDFI experiment runner refuses to invent a lineage (explicit unknown), ranks minimal fault sets, and every fault cites lineage-edge provenance with an audit hash.

### Interactive visual exploration

- **US-018 — Receive a view that fits the question**: the produced view is data-driven — every element carries a display mode and evidence, so the answer's form follows the facts.
- **US-019 — Switch representation without changing meaning**: representation switches keep meaning — the semantic hash is stable across layout choices (layout is presentation, not semantics).
- **US-021 — Drill from system to code**: drill to code — elements carry exact source spans.
- **US-025 — Control focus and hidden information**: focus control — a salience cap hides candidates openly (hidden[] ledger) and never silently drops them.
- **US-026 — Inspect an element's full story**: the inspector's data exists — every element states role (kind), label, and how it is known (display mode); hypotheses carry counter-arguments; the gate enforces consistency.
- **US-027 — Keep multiple views aligned**: views stay aligned — the producer is deterministic (same snapshot ⇒ identical ids + hash) and names no engine (VIZ-10).
- **US-029 — Overlay tests and runtime on any map**: overlay source data — the test-confidence overlay merges from the test map; the engine provides it without executing anything.
- **US-030 — Trust the drawing itself**: the drawing discloses its layout strategy (never hidden meaning).

### The visual catalogue

- **US-031 — See the architecture that answers my question**: groupings cite the code that justifies them — every concept node carries provenance and grounding (symbols or intent).
- **US-032 — Investigate a failure from a stack trace**: ranked suspects — LDFI results sort violated minimal sets first with per-fault provenance.
- **US-033 — See every way a failure can arise**: failure sites enumerated with citations — the ignored-error detector lists the site with evidence lines, and the Yuan et al. error-handling pass completes over real facts.
- **US-034 — Walk through a transaction step by step**: the journey's step order comes from call evidence — checkout's summary cites charge, charge cites authorize, in deterministic order.
- **US-035 — Trace who reads and writes data**: readers/writers traced — summaries carry read/write sets and the IFDS supergraph builds over the same facts.
- **US-036 — Compare code before and after a change**: before/after with consequences — adding a file shows up in the plan, both revisions stay loadable and comparable.
- **US-038 — See trust boundaries and privilege walls**: / US-048: trust walls from code evidence — the security slice flags an hardcoded secret (gate evidence) and classifies it as a candidate with witness lines, never a verdict.
- **US-040 — Find where races are possible**: race-relevant classification — effect classification names the builtin (concurrency-relevant) classes honestly from text evidence.
- **US-042 — See how well behaviour is tested**: behaviours-by-tests — the test map links the test to the behaviour, gaps become findings, minimum test sets derive from changes.
- **US-044 — See implicit concepts in the code**: concept cards pinned to implementing sites.
- **US-045 — Audit which policies are enforced**: policy surfaces — obligations and the guarded registry expose enforcement statuses; the state model reports its own authority.
- **US-049 — Ask for a diagram of my own**: a bounded, reviewable chart plan — capped spec stays within the cap, passes the gate, and is a pure description (no execution seam).

### Behaviour, concepts and business rules

- **US-050 — Extract and curate concept cards**: concept cards persist and reload (reviewable card set that outlives the process).
- **US-051 — Browse the business-concept hierarchy**: two axes — structural containment (parent/children links) and specialisation (FCA intent) coexist on the same card set.
- **US-053 — Discover distributed business rules**: the rule catalogue comes from the naming lexicon — promised actions are derived from names deterministically, with registry kinds.
- **US-054 — Compare intended and actual behaviour**: intended vs actual with named statuses — the doc's authorize- before-charge rule is verified against the real fixture code, and every verification lands in a named DriftStatus bucket.
- **US-055 — Understand workflow states and failures**: workflow states with honest authority — the FR-15 template edges always exist, but a refund CALL without an external write can only evidence RefundCalled, never Refunded (RefundCalled ≠ Refunded).
- **US-056 — Detect misleading names**: misleading names caught — `authorize` that only prints: the lexicon promises an auth action, the summary observes Print, and the naming adapter records the mismatch (InferredCandidate authority).

### Architecture, dependencies and impact

- **US-057 — Explore system structure**: real relationships, not just folders — reverse deps and call evidence agree, and the concept DAG adds grouping beyond directories.
- **US-058 — Find the change blast radius**: the blast radius separates direct dependents from the minimum test set — evidence, not folklore.
- **US-060 — Discover potential service boundaries**: candidate separations from the concept lattice — FCA over the tagged fixture yields shared-attribute concepts (coupling signal).

### Debugging and investigations

- **US-065 — Compare competing explanations**: competing explanations stay individually inspectable — every experiment carries a distinct audit hash and per-fault provenance.
- **US-067 — Trust the investigation's bookkeeping**: the bookkeeping refuses fabricated conclusions — a Clean record whose evidence carries a timeout marker is rejected; the honest Unknown is accepted.
- **US-068 — Reproduce an intermittent defect**: reproduction candidates are packaged and minimized — the shrinker keeps the error reachable (>= min_hits) and the package lands in a record.
- **US-072 — Triage defect candidates honestly**: defect candidates carry their rule, witness and remediation — candidates, never verdicts.
- **US-074 — Collect performance candidates**: performance candidates from patterns — the N+1 query detector names its pattern and location on a looped-execute fixture.

### History, collaboration and knowledge

- **US-089 — Track how an interface evolved**: interface/relation evolution — differential semantics: inserts, retractions through the dataflow, as-of frontier reads.
- **US-090 — Find documentation gaps**: documentation gaps as a list — a doc claiming behaviour the code lacks lands in the Missing bucket; undocumented code surfaces via reverse drift.
- **US-091 — Collaborate without widening access**: evidence travels with its elements — the drift report persists per revision and reloads with the same claims.
- **US-093 — Save and resume a workspace**: the workspace persists and resumes — the snapshot store roundtrips the full revision state.

### Security, permissions and supply chain

- **US-094 — Understand authorization**: authorization understood structurally — the auth precondition contract cites BOTH the guarded action (charge) and its guard (authorize) as evidence.
- **US-095 — Trace sensitive information**: sensitive information traced — taint sources classify from code evidence and the intra-procedural taint-lite flags source→eval.
- **US-097 — Review code-security and secret findings**: security findings are candidates with witnesses — the hardcoded credential finding carries rule id, security tags and a witness line, never a verdict label.

### Counterfactuals and digital twin

- **US-099 — Explore a hypothetical change**: removal counterfactual — deleting a file computes the consequence (plan.deleted), purges its symbols, and the purge is idempotent.
- **US-103 — Know the limits of predictions**: predictions never display as fact — the gate REJECTS a hypothesis element without its counter-argument, so unsupported output cannot render.

### Prompt-to-feature discovery and planning

- **US-107 — Resolve conflicting requirements**: conflicting requirement statements are captured as classified, individually inspectable claims (prohibition vs obligation) — the raw material for conflict resolution, neither averaged away nor silently dropped.

### Editor and agent integrations

- **US-133 — Navigate definitions and references from the editor**: definitions and references navigable from the semantic graph — a symbol resolves to its definition site AND its referents.
- **US-134 — Give coding agents evidence-shaped answers**: evidence-shaped answers for agents — the ViewSpec wire protocol roundtrips under the strict schema (deny_unknown_fields), so a machine consumer can rely on it; records carry typed JSON payloads.

### Privacy, models and product control

- **US-135 — Work locally by default**: local by default — a full ingest runs with the offline provider and no network seam engaged.
- **US-136 — Decide what may leave my machine**: nothing leaves the machine unless explicitly opted in — the default is no-LLM, and a zero-budget LLM never gets a charge through.
- **US-137 — Switch models and understand model failures**: providers are swappable and the offline provider never invents — identical questions get identical deterministic answers.
- **US-138 — Control cost, progress and cancellation**: cost control with cancellation — the budget stops cleanly at the cap and reports usage.
- **US-139 — Manage access, retention and deletion**: retention/deletion is provable — tombstoned files purge their facts idempotently; the second purge is a no-op.

### Honest operation

- **US-144 — Never see stale answers as current**: stale is never current — after a code change, the two drift reports keep their own revisions and the delta between them is computed, so old answers stay identifiable.
- **US-145 — Get graceful degradation instead of guesses**: graceful degradation — the OCR seam reports its state honestly (unavailable/failed markers) instead of inventing text for garbage input.
- **US-147 — See the product verify itself**: the product verifies itself — a tampered spec fails its own sealed hash, and the gate enforces evidence discipline.

## CLI

```text
cie stories list [--section <name>] [--disposition delivered|partial|planned] [--json]
cie stories show US-042            # full story + coverage entry
cie stories coverage [--json|--md] # honest totals, no percentages
cie stories verify [--id US-xxx] [--json]  # deep check / audit / planned verdicts
```

## Source-family mapping

Appendix A of the catalogue (28 families) is encoded as `cie_stories::catalog::FAMILIES`;
every family's story ids are validated against the catalogue by test.

## Relationship to the instructor-task traceability

`docs/TRACEABILITY.md` maps the 84 instructor tasks to code; this file maps the
147 user stories to the same engine. The 84 tasks are the substrate; the stories
are the user-visible outcomes. Where a story's obligation is host/product-layer
(canvas interactions, PR workflows, release approvals), the registry says so
explicitly instead of borrowing unearned credit from engine work.
