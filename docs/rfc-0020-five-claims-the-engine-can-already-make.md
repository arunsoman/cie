# RFC-0020: Five claims the engine can already make, if anything exports them

**Status:** ready for implementation (vetted 2026-10-20 — every file:line claim independently verified against the code; ONE insertion-point design error found and fixed in §6.2/§7, see §13)
**Author:** the CIE maintainer (with pi)
**Date:** 2026-10-20
**Applies to:** `cie-contract`, `cie-evidence`, `cie-ingest`, `cie-analysis`, `cie-fault`, `cie-stack`, `cie-cli`, the web app
**Related:** RFC-0019 (stack-at-ingest; this RFC assumes RFC-0019's `StackStoreExt` persistence pattern but does not require RFC-0019 to have landed — see §4), TASK-052/053/054 (`cie-fault`), US-102 (`StateModel`/`Calibration` "Delivered"), FR-15 (LDFI), FR-42 (every edge carries a reason)

---

## 1. Summary

Four of this RFC's five items are not new engineering — they are **already-written, already-tested Rust that nothing calls**. `cie-contract::obligations::StateModel`, `cie-evidence::readiness::Calibration`, and `cie-ingest::history::archaeology_chain` all exist, compile, and pass their own unit tests; `grep -c` against `cie-cli/src/main.rs` returns **zero** for every one of them. `cie-fault::ldfi::LdfiPlanner` is the same story, one level deeper: it is wired to nothing, and even its own fixtures feed it a single hardcoded demo chain.

This RFC proposes to:

1. **Export `StateModel`'s evidenced transitions** — the payment/recovery flow edges `cie-contract` already derives from real effect summaries, filtered to drop the always-present template filler, so Trust/Promises can say *"CIE found code that performs this transition, here's the evidence"* (§6.1). (Runtime-calibrated Observed/Assumed/Unmodelled labeling is explicitly **not** part of this RFC — see the correction below.)
2. **Bind every claim to its commit ancestry**, using the `archaeology_chain` function that already exists and is already tested (§6.2).
3. **Bind every obligation to the tests that actually reach it**, computed where the data already sits — inside `cie-analysis::conformance::run`, which already holds both the symbol store (`sym.is_test()`) and the resolved call graph (`FunctionSummary.callees`) in scope (§6.3).
4. **Resolve outbound service calls into real cross-service edges** — today `JvmOutbound` doesn't even record *what* was called, so this is the one genuinely new data-capture problem in the set (§6.4).
5. **Wire lineage-driven fault injection (LDFI)** into the pipeline, feeding it from §6.1's evidenced transitions instead of the hardcoded demo chain it ships with today (§6.5).

Items 1–3 are days, not weeks: the hard part was already done by whoever wrote `cie-contract`, `cie-evidence`, and `cie-ingest::history`. Items 4–5 are real feature work, and §6.5 explicitly depends on §6.1 landing first.

---

## 2. Motivation

### 2.1 The engine has a research-grade layer the product has never seen

Searching the engine crates for capabilities that would differentiate CIE from CodeQL, Sourcegraph, SonarQube, or an APM tool turned up three that are **already built**:

| Capability | Where it lives | Proof it's real | Proof it's unused |
|---|---|---|---|
| State model with runtime calibration (Observed/Assumed/Unmodelled per transition) | `cie-contract/src/obligations.rs:147` (`StateModel`), `cie-evidence/src/readiness.rs:262` (`Calibration`, `:272 state_for()`) | Both types compile, are tested, and are the capability-refs `USER-STORIES.md:64` maps to US-102's section (`cie-evidence::workload` + `cie-evidence::readiness`); per-story disposition lists do not exist, so "Delivered" is documented at the section/capability level | `grep -c "StateModel\|Calibration" cie-cli/src/main.rs` → **0** |
| Claim → commit ancestry (blame-derived) | `cie-ingest/src/history.rs:244` (`archaeology_chain`), `:262` (`bind_claims_to_history`) | Unit-tested at `history.rs:355` (`archaeology_chain_oldest_first_and_bound_only_when_blamed`) | Only referenced at `cie-ingest/src/lib.rs:13` as a re-export; zero call sites anywhere else in the workspace |
| Lineage-driven fault injection (Alvaro/Rosen/Hellerstein LDFI) | `cie-fault/src/ldfi.rs` (352 lines: `LineageGraph`, `Fault`, `LdfiPlanner`) | Unit-tested (`cie-fault/tests/task_053_ldfi.rs`) and exercised narratively five more times across `cie-stories/src/checks/{s2,s4,s7,s11}_*.rs` | `grep -c "cie_fault\|ldfi" cie-cli/src/main.rs` → **0**; every one of those eight call sites constructs its `LineageGraph` **by hand** with `g.add(...)` — nothing in the workspace derives one from real data |

No mainstream code-intelligence tool ties a derived state machine to live observed-vs-assumed calibration, and none does targeted, lineage-derived fault injection (that is usually an academic-paper-only technique, not a shipped product feature). CIE already has the Rust for both. The only thing stopping a demo of either is that `cie-cli` never calls them and `cie-types.ts` never carries their output.

**Two corrections that change §6.1/§6.5's scope, found by reading `from_effects` line-by-line (not just its signature):**

1. **`StateModel` is not a generic per-repo extractor — it is a fixed payment/refund template.** `State`/`Event` (`obligations.rs:35-74`) are hardcoded enums (`Pending/Authorized/RefundRequested/RefundPending/Refunded/Fulfilled`; `Authorize/Charge/Debit/TopUp/Fulfill/RefundCalled/RefundEffective`), and `StateModelBuilder::from_effects` (`:179-277`) **always** emits all seven of these template edges for every repository, regardless of what the repo actually does — the doc comment says so explicitly ("The FR-15 template edges are ALWAYS present"). When no function name matches the payment lexicon (`Event::from_name_tokens`, `:88-108`: `authorize`/`charge`/`debit`/`topup`/`fulfill`/`refund`), each edge falls back to `template_prov()` (`:294-302`): a sentinel `EffectProvenance{ symbol: SymbolId(u32::MAX), effect: "model_template", external_write: false, ... }` — a fake symbol id, not real evidence. **Exporting `StateModel` unfiltered, as an earlier draft of §6.1 did, would render the same six-state payment diagram on a repo that has nothing to do with payments** — a direct violation of the honesty rule this whole RFC is written in service of. §6.1 below is revised to filter on this.
2. **`Calibration` has no production builder anywhere in the workspace.** The only places a `Calibration{...}` literal is constructed are inside `#[cfg(test)]` blocks in `cie-evidence/src/readiness.rs` (verified during review: the only construction literals are at `readiness.rs:431` and `:445`, inside the test module that starts at `:308`; `:262`/`:270` are the struct definition and its `impl` — not literals). No code ingests real captured runtime data (histogram buckets, `revision`/`environment`/`captured_at_ns`) and produces one. §6.1's `display_mode` and §6.5's entire premise ("attack the `Observed` half of the state model") are revised below to be honest about this: `Observed`/`Assumed` labels cannot exist in production until a runtime-bucket ingestion path is built, which this RFC does not attempt (new non-goal, §4).

### 2.2 Two genuine data-model gaps, found by reading the structs field-by-field

The other two items are real gaps, not wiring:

**Cross-service calls are recorded as presence, not destination.** `cie-stack/src/types.rs:213`:

```rust
pub struct JvmOutbound {
    pub kind: String, // "rest-template" | "web-client" | "feign"
    pub cls: String,
    pub rel: String,
}
```

No `line`, no target URL, no target service name — not even unresolved. Confirmed at the construction site, `cie-stack/src/spring.rs:1208-1233`: every push is presence-only —

```rust
if names.contains(&"FeignClient") {
    jvm.outbound.push(JvmOutbound { kind: "feign".into(), cls: c.name.clone(), rel: c.rel.clone() });
}
```

— it records *that* a class is a Feign client or injects a `RestTemplate`/`WebClient`/`KafkaTemplate`, never the `@FeignClient(name=…)` argument or any call-site URL literal. Contrast with the sibling `JvmListener` (`types.rs:203`), which **does** capture a `target` string and a `line` for inbound messaging. The outbound side is structurally behind the inbound side.

The web app's `buildC4Spec` (`web/src/lib/mastery.ts:455`) is sometimes read as prior art here — it isn't. Its `crossCalls` map (`:474`) crosses *directory*-based "containers" inside **one** `report.json`'s own resolved call graph (`buildCallGraph(report).calleesOf`, `mastery.ts:139`); it has no notion of a second service's `stack.json` at all. There is **no existing heuristic anywhere, client or server**, that resolves one service's outbound call to another service's `RouteSite`. This is a clean net-new feature, not a replacement.

**Obligations cite symbols, never the tests that exercise them.** `cie-contract/src/lib.rs:149`:

```rust
pub struct Contract {
    pub id: ContractId,
    pub obligation: ObligationKind,
    pub expr: String,
    pub authority: Authority,
    pub symbols: Vec<SymbolId>,
    pub source: ContractSource,
}
```

No test link. The web's `testsReaching` (`web/src/lib/atlas.ts:828`) is a **filename** heuristic (`test_*`/`*_test`/`.test.`) run over the client-rebuilt call graph. The engine already has a sharper primitive sitting unused one layer down: `cie-semantic/src/facts.rs:81`, `Symbol::is_test()`, which checks the parsed `SymbolKind::Test` (a real annotation-derived kind — `#[test]`, `@Test`, etc.) **before** falling back to name heuristics. Nobody currently reads it for this purpose.

### 2.3 Where the authoritative call graph actually lives today

Worth stating plainly because it shapes §6.3's design: **there is no call-graph abstraction on the Rust side.** `grep -rln "struct CallGraph" --include=*.rs` outside of web finds nothing; the only `CallGraph` type in the workspace is `web/src/lib/mastery.ts:139`, built client-side from `report.summaries[].callees` (which **is** a Rust-side field — `cie-summary/src/lib.rs:83`, `FunctionSummary.callees: Vec<SymbolId>` — the web just assembles it into a graph, the engine never does). `cie-analysis::conformance::run` (`conformance.rs:21-26`) already receives `summaries: &BTreeMap<SymbolId, FunctionSummary>` as a parameter — so §6.3 needs no new graph type, just a reverse-index walk over data already in scope.

---

## 3. Goals

1. **StateModel's evidenced transitions reach `report.json`**, filtered to drop the always-present template sentinel, labelled `Fact` (real static evidence) and renderable by the existing `displayMode`/`certaintyPlain` machinery. `Calibration`-driven Observed/Assumed/Unmodelled labeling is explicitly deferred (§2.1, §4) — no production `Calibration` builder exists to feed it honestly.
2. **Every `Finding`/`Contract`/`Claim` gains a commit-ancestry chain** (who introduced it, when, via which commits) using the already-tested `archaeology_chain`.
3. **Every `Contract` gains a `verified_by: Vec<SymbolId>`** — the test symbols that actually reach its bound symbols — computed from data already in scope at the conformance stage, with an honest empty list (not a guess) when no test reaches it.
4. **`JvmOutbound` carries a target, and the stack lens resolves it** against other apps' `RouteSite`s in the same revision, producing cited cross-service edges with both files as evidence.
5. **LDFI runs inside the pipeline** against a `LineageGraph` derived from the state model in Goal 1 (not a hardcoded demo chain), with the same honest `Unknown`/`TimedOut` outcomes the planner already returns, now surfaced instead of discarded.
6. **Backward compatibility**: every new field is additive (`#[serde(default)]` / `Option<T>`); absence of upstream data (no stack lens, no state model, no history) degrades each consumer to exactly its current behavior, never a wrong new claim.

## 4. Non-goals

- **Not** rewriting `cie-fault`'s injector model. `RuntimeFaultInjector` stays the seam; this RFC adds one more implementation path (deriving `LineageGraph` from `StateModel`), not a new fault-execution engine.
- **Not** building a live distributed tracer. §6.5's `LineageGraph` is derived from **statically + runtime-calibration-derived** evidence (§6.1's `StateModel`/`Calibration`), not from instrumenting a message bus. A live-trace-fed lineage builder is a plausible future RFC; this one explicitly ships the cheaper, already-evidenced version first.
- **Not** depending on RFC-0019's UNLANDED stages. RFC-0019's stage 1 (the `StackStoreExt` persistence pattern: snapshot `#[serde(default)]` field + `revs/<N>/artifact.json` via the Store's generic `write_json`/`read_json`) **has shipped** (`dc35647`, verified), and stage 2 (stack-input hashing — `IngestPlan.stack_inputs`, `cie-ingest/src/lib.rs:122`) **has landed** (`1ee156b`). This RFC reuses the shipped pattern for the state-model artifact. RFC-0019 stages 3–5 (docker governance, C4 tiers, consumers) remain unlanded — this RFC does not require them (§6.4's cross-service join is its own new feature, not a reuse of RFC-0019 stage 3's design).
- **Not** generalizing `Property` (the LDFI-attacked expression) beyond "derived from one `StateModel` transition's liveness label" in v1. A richer property language (temporal logic, multi-transition composite properties) is future work; see §10.
- **Not** adding new languages/frameworks to the stack lens beyond what's needed to prove §6.4 (the existing Spring/Feign path is extended; Express/Axum/Flask outbound detection is explicitly deferred, §10).
- **Not** generalizing `StateModel`/`State`/`Event` beyond the hardcoded payment/refund template (§2.1 correction 1). A generic, per-repo enum/switch state-machine extractor (the Tier-2 detector discussed earlier in this project) is a materially different, larger feature — this RFC only exports what `from_effects` already computes, filtered to real evidence (§6.1), never the full template.
- **Not** building a production `Calibration` pipeline (§2.1 correction 2). Ingesting real runtime buckets (what a "capture" step even is, what counts as one `environment`, how `captured_at_ns` windows get drawn) is its own RFC. This one ships `StateModel` export on its own merits (§6.1) and makes §6.5 openly degrade to `Unknown(NoLineage)` until that future RFC lands.
- **Not** pre-empting a blame/history build-out: this RFC mines git blame ONLY for the distinct cited `rel:line`s (§6.2) — bounded, disclosed, never a full-tree blame sweep.

---

## 5. Current-state inventory (with references)

| Fact | Reference |
|---|---|
| `StateModel`/`Transition` definition | `cie-contract/src/obligations.rs:129 Transition`, `:147 StateModel`, `:163 StateModelBuilder`, `:181 from_effects`, `:298 template_prov` |
| `Calibration`/`state_for` definition; sole construction literals inside the test module | `cie-evidence/src/readiness.rs:262 Calibration`, `:272 state_for`; literals `:431, :445` (test module starts `:308`; `:262/:270` are the definition + impl — verified) |
| Zero `StateModel`/`Calibration` references in the CLI | `grep -c "StateModel\|Calibration" cie-cli/src/main.rs` → 0 |
| `archaeology_chain`/`bind_claims_to_history` | `cie-ingest/src/history.rs:244`, `:262`; re-exported only at `cie-ingest/src/lib.rs:13`; zero other call sites |
| `Contract` shape (Rust + TS, identical) | `cie-contract/src/lib.rs:149`, `web/src/lib/cie-types.ts:43` |
| `Symbol::is_test()` (sharper than the web's filename heuristic) | `cie-semantic/src/facts.rs:81` |
| `conformance::run` already holds `SymbolStore` + `summaries: &BTreeMap<SymbolId, FunctionSummary>` | `cie-analysis/src/conformance.rs:16-28` |
| `FunctionSummary.callees` (the only call-edge data that exists, Rust side) | `cie-summary/src/lib.rs:73,83` |
| Web's `CallGraph`/`buildCallGraph` (client-only; no Rust equivalent exists) | `web/src/lib/mastery.ts:139` |
| Web's `testsReaching` (the heuristic this RFC's §6.3 makes authoritative) | `web/src/lib/atlas.ts:828` |
| `JvmOutbound`/`JvmListener` current shape | `cie-stack/src/types.rs:203,213` |
| Outbound detection is presence-only | `cie-stack/src/spring.rs:1208-1233` |
| `RouteSite` (the join target for §6.4) | `cie-stack/src/types.rs:266-279` |
| `CompositeParser` (one `StackReport` per revision, already multi-app) | `cie-stack/src/factory.rs:196-223`, merge at `:257` |
| `report.apps` populated directly from `detect::find_apps`, not per-sub-parser | `cie-stack/src/lib.rs:213` |
| The five-service polyglot fixture (no outbound calls between them today) | `cie-stack/tests/polyglot.rs:1-20`; `grep` for `FeignClient\|RestTemplate\|8081\|8082\|localhost` in that file → 0 hits |
| `buildC4Spec`'s `crossCalls` is intra-repo, directory-based — **not** cross-service | `web/src/lib/mastery.ts:455-527` |
| `LineageGraph`/`Fault`/`LdfiPlanner` definitions | `cie-fault/src/ldfi.rs` (whole file) |
| `SimInjector`'s hardcoded demo chain | `cie-fault/src/ldfi.rs:165` (`vec!["debit","ledger","credit_event","balance_update"]`) |
| `Property::liveness_from_task_052` — the only `Property` constructor | `cie-fault/src/ldfi.rs:112-122` |
| `trace::suspects_from_trace` — ranks suspects against an *existing* `LineageGraph`, doesn't build one | `cie-fault/src/trace.rs:7-12,435` |
| Every current `LineageGraph` is hand-built in test/story code | `cie-fault/tests/task_053_ldfi.rs:17-21`; `cie-stories/src/checks/{s2_answers,s4_catalog,s7_investigations,s11_closes}.rs` |
| `cie-fault`'s dependency set (no access to stack/evidence data today) | `cie-fault/Cargo.toml`: `cie-core`, `cie-semantic`, `cie-contract`, `serde` only |
| CLI subcommand style precedent (`--json`, `--force`, doc comment cites an ID) | `cie-cli/src/main.rs:56-100` (`Deps`, `Stack`, `Ingest`) |

---

## 6. Design

### 6.1 Export `StateModel`, filtered to evidenced transitions — `Calibration` deferred

**Where it's computed today:** `StateModelBuilder::from_effects` (`cie-contract/src/obligations.rs:179-277`) builds a `StateModel` from extracted effect summaries — but, per §2.1 correction 1, it **always** returns all seven payment-template edges, filling unevidenced ones with the `template_prov()` sentinel (`:294-302`, `SymbolId(u32::MAX)`, `effect: "model_template"`). `cie-evidence::readiness::Calibration` (`:262`) exists and its `state_for()` (`:272`) would label each transition `Observed`/`Assumed`/`Unmodelled` — but per §2.1 correction 2, nothing in the workspace builds a production `Calibration`, so this RFC cannot honestly promise that labeling yet.

**v1 scope, revised:** export only transitions with **real evidence** — i.e. at least one `EffectProvenance` whose `symbol != SymbolId(u32::MAX)` (equivalently, `effect != "model_template"`). A repo with no payment-shaped function names exports `state_model: None`, exactly as it should.

**New CLI surface**, matching the `Stack`/`Deps` precedent (`cie-cli/src/main.rs:56-100`):

```rust
/// Payment/recovery flow transitions the engine found real evidence for
/// (US-102, TASK-052). Template-only edges (no matching function names)
/// are dropped, not exported — this is a filtered view of StateModel,
/// not a generic per-repo state-machine extractor (see RFC-0020 §2.1).
StateModel {
    path: PathBuf,
    #[arg(long)]
    json: bool,
},
```

**Persistence** — the pattern RFC-0019 stage 1 SHIPPED (`cie-ingest/src/stack_store.rs`), not new design: a `#[serde(default)] state_model: Option<...>` field on `RevSnapshot` (so pre-RFC snapshots deserialize as `None` and never gain claims they weren't derived from) **plus** a `StateModelStoreExt` for the standalone artifact at `<repo>/.cie/revs/<N>/state-model.json` — mirroring `StackStoreExt`'s `write_json`/`read_json` implementation (the Store already has the generic helpers; the trait is ~30 lines), persisted BEFORE `save_snapshot` per the shipped ordering guarantee. Computed once per revision, inside `cie-ingest::run_ingest`, right after `cie_analysis::analyze` (the obligation extraction `analyze` already performs is `StateModelBuilder`'s input — `from_effects(store, &summaries)` at `cie-contract/src/obligations.rs:181` takes exactly the structures `run_ingest` holds).

**Report field** (additive; mirrors `RevSnapshot`'s `#[serde(default)]` precedent used for the stack field in RFC-0019):

```rust
// cie-types report shape (wherever report.json's top-level struct lives)
#[serde(default)]
pub state_model: Option<StateModelView>,   // None when every edge is template-only

pub struct StateModelView {
    pub transitions: Vec<TransitionView>,
    /// Always present when state_model is Some — states plainly that this
    /// is the payment/recovery template, not a general extraction, so the
    /// UI never implies otherwise.
    pub domain_note: String,  // "payment/recovery flow (TASK-052 template)"
}
pub struct TransitionView {
    pub from: String,
    pub event: String,
    pub to: String,
    pub provenance: String,
    /// v1: Fact when real (non-template) EffectProvenance exists — this
    /// IS static evidence, just not runtime-calibrated. NEVER Observed/
    /// Assumed/Unmodelled in v1 (§2.1 correction 2 — no Calibration
    /// producer exists); that upgrade is the explicit job of the future
    /// RFC that builds one (§4, §10).
    pub display_mode: String,
}

fn to_view(model: &StateModel) -> Option<StateModelView> {
    let transitions: Vec<_> = model.edges.iter()
        .filter(|t| t.provenance.iter().any(|p| p.symbol != SymbolId(u32::MAX)))
        .map(TransitionView::from)
        .collect();
    if transitions.is_empty() { None } else { Some(StateModelView { transitions, domain_note: "...".into() }) }
}
```

**TS side** (`cie-types.ts`): add `StateModelView`/`TransitionView` next to `Contract`; render through the existing `certaintyPlain`/`CertaintyChip` machinery — no new UI primitive needed. `domain_note` renders as a fixed caption under the section heading so the payment-specific scope is never ambiguous to a viewer.

**Failure policy:** no evidenced transitions (every edge template-only, or no obligations extracted at all) → `state_model: None`; consumer (Trust/Promises view) shows nothing, not an empty-but-present section — same `Option` discipline as RFC-0019 §6.6, now also covering "evidenced but empty."

### 6.2 Claim → commit ancestry

**What exists:** `archaeology_chain(history: &HistoryFacts, path: &str, line: u64) -> Vec<CommitMeta>` (`cie-ingest/src/history.rs:244`) and `bind_claims_to_history(...)` (`:262`), both unit-tested.

**Insertion point — split by producer stage (REVISED from the original draft, which wrongly assumed `run_ingest` already had `HistoryFacts`/blame and drift claims in scope; vetted against the code, §13):**

The binding happens **where each claim type is created and the repo's git dir is at hand, with blame mined ONLY for the cited `rel:line`s** (bounded, disclosed — never a full-repo blame sweep; see §7):

- **`Contract` + `Finding`** — inside `cie-analysis::conformance::run`, where both types are created (`conformance.rs:22–28`): after `contracts`/`findings` are finalized, mine blame for the *distinct cited paths only* (a `SystemGit` seam already exists in `cie-ingest`, `history.rs`'s `HistoryProvider::ingest` + `parse_blame` — **today nothing in production mines blame at all**: the cochange command runs `git log --name-status` only, `cie-ingest/src/cochange.rs:164–185`, which names paths+commits, blame-free). The provider returns `HistoryUnavailable` on shallow clones / non-repos → `introduced: None` per claim.
- **`Claim`** (drift's claims, `cie-claims/src/model.rs:66`) — inside `cie-drift`'s command path, right after its claims are persisted: the drift stage is the only stage whose claims exist at THIS revision, and it runs where the working tree (and `.git`) is present. Binding here also covers re-run drifts without re-parsing anything else.

```
// conformance.rs, after contracts/findings are emitted:
let history = history_provider.ingest(git_root, &HistoryOptions{ cited_paths: cited_paths(&contracts, &findings), ..default });
let ancestry = bind_claims_to_history(&history, &all_cited_sites(&contracts, &findings));
// each Contract/Finding gains `introduced: Option<Vec<CommitMeta>>` from the map
```

**Shape change** — additive on each claim-bearing type:

```rust
// cie-contract/src/findings.rs's Finding, cie-contract/src/lib.rs's Contract,
// cie-claims/src/model.rs:66's Claim (the draft's "DriftClaim" name does not
// exist in the workspace — corrected during vetting)
#[serde(default)]
pub introduced: Option<Vec<CommitMeta>>,   // archaeology_chain's output, oldest-first
```

`CommitMeta` already exists (used by `archaeology_chain`'s return type) — reuse it verbatim; no new type.

**Failure policy:** no git history available (shallow clone, not a repo) → `introduced: None` for every claim; `archaeology_chain`'s own tests already assert "bound only when blamed" (`history.rs:355`), so a claim with no blame match legitimately gets `None` rather than a fabricated chain.

**Consumers:** Quality/Trust views add an "introduced" line per finding/contract when present — pure additive UI, no existing render path changes.

### 6.3 Obligation → verifying-test binding

**Where it's computed:** inside `cie-analysis::conformance::run` (`conformance.rs:21-28`), which already has `store: &SymbolStore` (so `sym.is_test()` is one call away, `cie-semantic/src/facts.rs:81`) and `summaries: &BTreeMap<SymbolId, FunctionSummary>` (so `.callees` is the only call-edge data needed, `cie-summary/src/lib.rs:83`) — **in scope already**, at the exact point `contracts: Vec<Contract>` is assembled (`conformance.rs:18,28`).

**The walk** — a reverse BFS over `summaries` from each contract's bound symbols, capped (matching the house `MAX_*` convention, e.g. `flow.rs::MAX_FLOW_NODES`):

```rust
// cie-analysis/src/conformance.rs, after contracts are built, before returning ConformanceResult
const MAX_VERIFY_DEPTH: usize = 6; // test → …→ obligation symbol, bounded like flow.rs

fn verified_by(store: &SymbolStore, summaries: &BTreeMap<SymbolId, FunctionSummary>, target: &[SymbolId]) -> Vec<SymbolId> {
    // reverse index: callee -> callers, built once per run() call (small: O(summaries))
    let mut callers: BTreeMap<SymbolId, Vec<SymbolId>> = BTreeMap::new();
    for (from, s) in summaries {
        for &to in &s.callees {
            callers.entry(to).or_default().push(*from);
        }
    }
    let mut found = BTreeSet::new();
    let mut frontier: Vec<SymbolId> = target.to_vec();
    for _ in 0..MAX_VERIFY_DEPTH {
        let mut next = vec![];
        for sym in &frontier {
            for &caller in callers.get(sym).unwrap_or(&vec![]) {
                if let Some(s) = store.symbols.iter().find(|s| s.id == caller) {
                    if s.is_test() { found.insert(caller); } else { next.push(caller); }
                }
            }
        }
        if next.is_empty() { break; }
        frontier = next;
    }
    found.into_iter().collect()
}
```

**Shape change:**

```rust
// cie-contract/src/lib.rs
pub struct Contract {
    ...                                  // unchanged
    #[serde(default)]
    pub verified_by: Vec<SymbolId>,      // NEW — test symbols that reach a bound symbol, within MAX_VERIFY_DEPTH
}
```

**Honesty:** an empty `verified_by` is not an error — it is the honest answer "no test in the recorded call graph reaches this obligation," renderable as its own Trust-view note ("this rule has no test coverage CIE can see"), which is strictly more useful than today's silence.

**Why engine-side, not web-side (unlike RFC-0019's eventual direction for §6.4):** the data (`is_test()`, `summaries`) is already *in the function*; computing it in TypeScript would mean re-deriving `is_test()`'s annotation-aware logic a second time client-side, repeating exactly the "two lenses that never meet" problem RFC-0019 was written to retire. `web/src/lib/atlas.ts:828`'s `testsReaching` keeps working as the fallback when `verified_by` is empty/absent (older artifacts), per the compatibility rule in Goal 6.

### 6.4 Cross-service call resolution

**Step 1 — capture the target, not just the presence** (`cie-stack/src/types.rs:213`):

```rust
pub struct JvmOutbound {
    pub kind: String,            // unchanged: "rest-template" | "web-client" | "feign" | "kafka-producer"
    pub cls: String,
    pub rel: String,
    pub line: u32,                       // NEW — was missing entirely
    pub target: Option<String>,          // NEW — @FeignClient(name=…)/url=… as written, or a call-site URL literal
}
```

**Step 2 — read the argument, not just the annotation name** (`cie-stack/src/spring.rs:1208-1233`): the `@FeignClient` branch already has `a.args` available elsewhere in this file for exactly this kind of extraction (see the `QUOTED_RE` pattern already used for `JvmListener.target` at `spring.rs:1196-1199`) — reuse the identical regex capture:

```rust
if let Some(ann) = c.annotations.iter().find(|a| a.name == "FeignClient") {
    let target = QUOTED_RE.captures(&ann.args).and_then(|x| x.get(1).map(|g| g.as_str().to_string()));
    jvm.outbound.push(JvmOutbound { kind: "feign".into(), cls: c.name.clone(), rel: c.rel.clone(), line: c.line, target });
}
```

`RestTemplate`/`WebClient`/`KafkaTemplate` injections don't name a target at the class level (the URL is usually a call-site string argument, e.g. `restTemplate.getForObject("http://billing/invoices", ...)`); v1 scope is **`@FeignClient` only**, where the target is a class-level annotation argument and therefore staticallyfindable without full call-site literal tracking. RestTemplate/WebClient call-site resolution is explicitly deferred (§10) — it needs a different extraction (walking method bodies for string-literal arguments to the injected client), a bigger, separate piece of work.

**Step 3 — the join**, a new stage after `CompositeParser::parse` finishes merging all apps into one `StackReport` (`cie-stack/src/factory.rs:215-223`, so it runs **after** every app's `routes` are already present in the same report):

```rust
// cie-stack/src/lib.rs, after the composite parse + merge, before returning the report
pub struct CrossServiceCall {
    pub from_app: String,       // AppSite.name of the caller
    pub to_app: String,         // AppSite.name of the matched service
    pub via: String,            // "feign" (v1; widens as Step 2 widens)
    pub matched_path: String,   // the RouteSite.path it resolved to
    pub from_evidence: String,  // rel:line of the JvmOutbound
    pub to_evidence: String,    // rel:line of the matched RouteSite
}

fn resolve_cross_service(report: &mut StackReport) {
    let Some(jvm) = &report.jvm else { return };
    for ob in &jvm.outbound {
        let Some(target) = &ob.target else { continue };
        // match target against another app's RouteSite — by app name first
        // (Feign's `name=` conventionally holds the target service's
        // spring.application.name; AppSite.name is the MANIFEST name
        // (artifactId / package.json "name" / dir basename), which usually
        // but not always equals it — when they diverge the fallback below
        // catches path-literal matches; see the RFC's §6.4 note),
        let hit = report.routes.iter().find(|r| r.service.eq_ignore_ascii_case(target));
        if let Some(r) = hit {
            report.cross_service.push(CrossServiceCall {
                from_app: app_of(report, &ob.rel),       // the app whose dir contains ob.rel
                to_app: r.service.clone(),
                via: ob.kind.clone(),
                matched_path: r.path.clone(),
                from_evidence: format!("{}:{}", ob.rel, ob.line),
                to_evidence: format!("{}:{}", r.rel, r.line),
            });
        } else {
            report.notes.push(format!(
                "{} names outbound target \"{target}\" — no CIE-parsed service matches it (external, or unparsed)",
                ob.rel
            ));
        }
    }
}
```

`StackReport.cross_service: Vec<CrossServiceCall>` is a new, additive top-level field (`#[serde(default)]`).

**Step 4 — fixture proof**: extend `cie-stack/tests/polyglot.rs`'s fixture with one `@FeignClient(name = "billing")` client in `berth-booking` (java/spring) targeting `billing`'s (kotlin/spring) existing invoice route — both already exist per the fixture inventory in §5 — plus a new test:

```rust
#[test]
fn feign_outbound_resolves_to_the_other_services_route() {
    let r = stack_report(&dir);
    let hit = r.cross_service.iter().find(|c| c.from_app == "berth-booking" && c.to_app == "billing");
    assert!(hit.is_some(), "the feign client in berth-booking must resolve to billing's route");
}
```

**Atlas:** a new `AtlasView` panel ("Cross-service calls") beside the existing Containers/Components panels, reading `report.stack.crossService` directly — same click → `openCodeFor` pattern already used for the app/service entry tables added earlier this project, citing both `from_evidence` and `to_evidence`.

### 6.5 Wire LDFI, fed by §6.1's evidenced transitions — NOT by `Calibration` (it has no producer)

**The gap this closes:** `SimInjector` (`cie-fault/src/ldfi.rs:144-195`) hardcodes its success chain (`:165`) and `Property::liveness_from_task_052` (`:112-122`) hardcodes its attacked property. Both are fine as **unit-test fixtures**; neither is a path to running LDFI against a real analyzed repo.

**Revised from an earlier draft:** that draft derived `LineageGraph` from transitions `Calibration::state_for()` marks `Observed`. Per §2.1 correction 2, no code anywhere builds a production `Calibration` — gating on it would mean §6.5 can **never** fire outside tests, silently. The honest v1 instead derives `LineageGraph` straight from §6.1's **evidenced** transitions (real `EffectProvenance`, not the `model_template` sentinel) — the same static-evidence bar §6.1 already uses, no runtime data required or implied:

```rust
// cie-fault/src/lineage_from_state_model.rs
use crate::ldfi::{LineageEdge, LineageGraph, LineageKind};
use cie_contract::obligations::{StateModel, Transition};
use cie_core::types::SymbolId;

fn is_evidenced(t: &Transition) -> bool {
    t.provenance.iter().any(|p| p.symbol != SymbolId(u32::MAX))
}

/// Turns the EVIDENCED half of a StateModel (§6.1's filter — real
/// EffectProvenance, not the template sentinel) into a LineageGraph LDFI
/// can attack. This is static evidence, not runtime calibration: v1
/// explicitly does NOT claim these transitions were "observed running" —
/// only that CIE found code that performs them. The Observed/Assumed
/// distinction (ldfi.rs's own §7 qualification, :10-12, which this
/// function deliberately does not invoke) is future work, gated on a
/// production Calibration builder that does not exist yet (§4, §10).
pub fn lineage_from_evidenced(model: &StateModel) -> LineageGraph {
    let mut g = LineageGraph::empty();
    for t in model.edges.iter().filter(|t| is_evidenced(t)) {
        g.add(&format!("{:?}", t.provenance), LineageKind::StateUpdate, t.from.as_str(), t.to.as_str());
    }
    g
}
```

**CLI surface** (mirrors `Stack`'s `--force`-style flag pattern, `cie-cli/src/main.rs:56-100`):

```rust
/// Lineage-driven fault injection against the EVIDENCED half of the state
/// model (TASK-053, FR-15) — static evidence only, not runtime-observed.
/// Honest Unknown(NoLineage) when no transition has real evidence — never
/// "recovery is sound" by default.
Fault {
    path: PathBuf,
    #[arg(long, default_value_t = 2)]
    max_faults: usize,
    #[arg(long, default_value_t = 500)]
    budget_ms: u64,
    #[arg(long)]
    json: bool,
},
```

Wiring: load the revision's `state-model.json` (§6.1's artifact), call `lineage_from_evidenced`, derive `Property` from any one evidenced transition (v1: the first by `(from, event)` order — a real aggregation policy is §10 open question 4), then `LdfiPlanner::new(lineage, SimInjector::new()).run(&property, limits)` exactly as `cie-fault/tests/task_053_ldfi.rs` already does — the planner itself needs **zero changes**.

**Honest degraded path:** a repo with no payment-shaped code → `state_model: None` from §6.1 → `lineage_from_evidenced` is never called → CLI reports "no evidenced state model for this revision," distinct from and more informative than LDFI's own `Unknown(NoLineage)` (`ldfi.rs:276-278`, still used when a state model exists but `Property` derivation fails). This is why §6.5 is sequenced after §6.1 in §11: it reuses §6.1's filter, not a nonexistent `Calibration` instance.

**`RuntimeFaultInjector` stays untouched.** `SimInjector` remains the deterministic test double; a real-target injector (one that could apply drops/delays against an actual running system) is explicitly out of scope (§4) — this RFC proves the hypothesis-ranking half of LDFI against real (if currently sparse) lineage, not live fault execution.

---

## 7. Performance & budget

- §6.1/§6.2: both run once per revision, reusing data the ingest pipeline already computes (obligation summaries for §6.1; the existing `SystemGit` blame seam for §6.2). §6.2's honest correction: mine git blame ONLY for the distinct cited `rel:line`s — this is NEW git work (nothing mines blame in production today, `cochange.rs:164–185` is name-status only), bounded to cited paths and disclosed, not a full-repo sweep.
- §6.3: `O(summaries)` reverse-index build once per `conformance::run` call, bounded BFS at `MAX_VERIFY_DEPTH = 6` per contract — same order of magnitude as the existing print-only/destructive-call scans already in `conformance.rs`.
- §6.4: one pass over `jvm.outbound` × `report.routes` per revision (both already bounded by the existing `MAX_DIRS`/parse caps) — no new traversal of source files.
- §6.5: `LdfiPlanner::run`'s own `budget_ns`/`max_faults` caps (`RoundLimits::default()`, `ldfi.rs:231-235`) already bound the experiment count; `lineage_from_evidenced`'s output is at most `|StateModel.edges|` (7, fixed by the template) edges, so the hypothesis count LDFI enumerates (singles + pairs, `ldfi.rs:284-301`) stays bounded by the same existing math the planner already uses.

---

## 8. Test plan

| Level | Gate |
|---|---|
| Unit (`cie-contract`/`cie-evidence`) | existing `StateModel`/`Calibration` tests stay green (both untouched); **NEW**: `to_view` drops every edge whose only provenance is the `SymbolId(u32::MAX)` template sentinel; returns `None` when all seven edges are template-only |
| Unit (`cie-analysis`) | existing `archaeology_chain` tests (`history.rs:355`) stay green; **NEW**: the conformance-stage binding produces `introduced: Some(...)` only for blamed claims, `None` otherwise (per §6.2's revised split — binding lives at the producer stages, not `run_ingest`) |
| **NEW** unit (`cie-analysis`) | `verified_by` finds a direct test caller; finds a transitive one within `MAX_VERIFY_DEPTH`; returns `[]` (not an error) when no test reaches the symbol; respects the depth cap on a synthetic deep chain; §6.2's conformance-stage binding produces `introduced: Some(...)` for blamed contracts, `None` on `HistoryUnavailable` (shallow clone) |
| **NEW** unit (`cie-drift`) | drift claims gain `introduced` when blame matches, stay `None` without — pinned at the drift command's stage (the claims exist only there), not `run_ingest` (type: `cie-claims::model::Claim`, not "DriftClaim" — vetted) |
| **NEW** unit (`cie-stack`) | `JvmOutbound.target` captured from `@FeignClient(name=...)`; `resolve_cross_service` matches an outbound target to another app's route; unmatched target produces the honest note, never a silent drop |
| E2E (`cie-stack`) | polyglot fixture: `feign_outbound_resolves_to_the_other_services_route` (new, §6.4 Step 4) |
| **NEW** unit (`cie-fault`) | `lineage_from_evidenced` includes only edges with non-sentinel provenance, excludes template-only edges; a `StateModel` with zero evidenced edges → empty graph → `LdfiPlanner::run` returns `Unknown(NoLineage)` (regression-pins existing `ldfi.rs:276-278` behavior) |
| Compatibility | pre-RFC snapshots (no `state_model`/`introduced`/`verified_by`/`cross_service` fields) deserialize and every consumer falls back to current behavior exactly |
| Honesty | every new note/empty-list case asserts "as written"/"no X found" phrasing; `verified_by: []` never renders as "unverified" (a stronger, wrong claim) — only as "no test reaches this, as far as CIE can see" |

---

## 9. Alternatives considered

| Alternative | Why not (now) |
|---|---|
| Compute `verified_by` client-side in `atlas.ts`, reusing the existing `testsReaching` heuristic | Fastest, but re-derives `is_test()`'s annotation-aware logic a second time in TypeScript — exactly the "two lenses" anti-pattern RFC-0019 exists to retire. The engine-side data is already in scope for free (§6.3). |
| Build a live distributed tracer to feed LDFI real multi-service lineage | The "true" version of §6.5, but a much larger infra investment (message-bus instrumentation, a collector, a schema) for a result this RFC can approximate today from data the engine already computes. Revisit once real production traces are a CIE input (tracked as an open question, §10). |
| Extend `JvmOutbound` to resolve RestTemplate/WebClient call-site URLs in v1 | Requires walking method bodies for string-literal arguments to an injected client — a materially bigger parser change than reading one annotation argument. `@FeignClient`-only ships the join logic and fixture proof now; the other clients extend the same `target: Option<String>` field later without a shape change. |
| A richer temporal-logic `Property` language for LDFI | Solves a real limitation (today's `Property` is just a label), but is a language-design exercise on its own. v1's "attack the first evidenced transition, by `(from, event)` order" is mechanically derivable from data this RFC already exports; a property DSL is future work. |
| Gate §6.5 on a production `Calibration` (Observed/Assumed-based lineage), as an earlier draft of this RFC did | No code anywhere builds a production `Calibration` (§2.1 correction 2) — gating on it would make §6.5 permanently inert outside unit tests, silently. Shipping on §6.1's static-evidence filter instead gives LDFI real (if currently sparse) data today; the Calibration-gated version becomes a straightforward upgrade once that future RFC lands (§10 open question 3). |

---

## 10. Open questions

1. **Feign `name=` vs `url=`** — the join in §6.4 assumes `name=` names a `spring.application.name` that matches `AppSite.name`/`RouteSite.service`. When only `url=` is given (a literal host), the match needs to degrade to a path-literal comparison instead of a service-name comparison — tracked as a fallback branch in Step 3, not fully specified here.
2. **RestTemplate/WebClient/Kafka-producer target capture** — deferred per §4/§9; the `target: Option<String>` field is deliberately generic enough that a later RFC can populate it from call-site literal tracking without another shape change.
3. **A production `Calibration` builder, and `lineage_from_observed` as the Calibration-gated upgrade to §6.5** — once something ingests real runtime buckets and builds a `Calibration` (§2.1 correction 2; its own RFC), `lineage_from_evidenced` gets a sibling `lineage_from_observed` that uses `state_for()` to restrict to transitions runtime evidence actually confirmed, exactly as an earlier draft of this RFC envisioned. Until then, §6.5 ships on static evidence alone. Separately, once real distributed traces become a CIE input, a `lineage_from_trace` source can coexist (`LineageGraph` is just edges; any source fills it).
4. **`Property` beyond one transition** — composite liveness properties spanning multiple transitions (the real TASK-052 case was two chained obligations) need a small aggregation step this RFC doesn't specify; v1 picks the single least-certain transition per `Fault` run.

---

## 11. Build order (each stage shippable + revertable)

| # | Stage | Size |
|---|---|---|
| 1 | §6.1 — `StateModel` export, filtered to evidenced transitions: `StateModelStoreExt`, CLI command, report field, TS types (`Calibration` export explicitly deferred, §4) | S |
| 2 | §6.2 — commit-ancestry binding split by producer stage: `conformance::run` (contracts/findings; blame only cited paths), `cie-drift` (its own claims) — `introduced` field on `Finding`/`Contract`/`Claim` | S-M |
| 3 | §6.3 — `verified_by` inside `conformance::run`, `Contract.verified_by` field | S-M |
| 4 | §6.4 — `JvmOutbound.target`/`.line`, `resolve_cross_service`, `StackReport.cross_service`, polyglot fixture + test | M |
| 5 | §6.5 — `lineage_from_evidenced`, `Fault` CLI command, wiring to §6.1's artifacts | M (depends on #1) |
| 6 | Atlas/Trust/Quality consumers for all four new fields + sample regeneration | M |

Stages 1–4 have no ordering dependency on each other and can land in any order or in parallel; stage 5 explicitly requires stage 1's artifact to exist first (§6.5's degraded-path test pins the pre-stage-1 behavior so stage 5 is safe to land early anyway — it will just always return `Unknown(NoLineage)` until stage 1 ships). Stage 6 is the first that changes visible UI and should land last per field, not all at once.

---

## 12. Why this is safe for maintainers

- **Four of five items are exports, not inventions.** `StateModel`, `archaeology_chain`, and `LdfiPlanner` are already-reviewed, already-tested code written by someone who understood the problem; this RFC's job is plumbing, not algorithm design. A maintainer who trusts `cie-fault/tests/task_053_ldfi.rs` today should trust `lineage_from_evidenced` tomorrow — it feeds the identical, unmodified planner. (`Calibration` is the one exception — this RFC deliberately does **not** export it; see below.)
- **Every new field is `Option`/`#[serde(default)]`.** No existing artifact, snapshot, or report shape changes meaning; absence degrades every consumer to exactly today's behavior (§6's "Failure policy"/"Honest degraded path" notes per item).
- **§6.3 adds zero new infrastructure.** No new `CallGraph` type, no new crate dependency — the reverse-index walk runs on data (`SymbolStore`, `FunctionSummary.callees`) that `conformance::run` already receives as function parameters.
- **§6.1/§6.5 were corrected mid-draft, on purpose.** An earlier pass of this RFC proposed exporting `StateModel` unfiltered and gating §6.5 on `Calibration::state_for()`. Reading `from_effects` and `readiness.rs` line-by-line (§2.1) found that the first would show a fabricated payment diagram on non-payment repos, and the second would silently never fire (no production `Calibration` exists). Both are fixed by filtering on real `EffectProvenance` instead — the same bar CIE already applies everywhere else. This correction is left visible in the text (§2.1, §9) rather than quietly folded in, so a reviewer can see exactly what was caught and why, and so nobody re-proposes the unfiltered version later without re-deriving why it was rejected.
- **The rollback path is real.** Reverting any single stage (§11) returns its consumers to an `Option::None`/empty-list state they already handle today — none of the five exports are load-bearing for anything that currently ships.

---

## 13. Review and vetting log (2026-10-20) — what independent verification found

This RFC was reviewed by reading EVERY cited function and struct directly (not trusting the draft's
own citations). Outcome: **ready for implementation**, with three findings — one substantive design
error (fixed in the text above), one precision fix, one citation fix:

1. **§6.2's original insertion point was wrong — fixed.** The draft claimed `run_ingest` could reuse
   "HistoryFacts already mined for cochange" and that §7 needs "no new full-repo walk". Verification:
   production `cochange` runs `git log --name-status` ONLY (`cie-ingest/src/cochange.rs:164–185`) —
   its facts are numstat rows, NOT `HistoryFacts` (which includes `blames` via `parse_blame`; no
   production code calls the blame seam today — verified by grep: `parse_blame` referenced only in
   `history.rs` itself + the `lib.rs:13` re-export). Also, the draft's snippet bound `&drift` inside
   `run_ingest` — drift claims do not exist at that stage (drift is a later, separate command).
   **Fix applied**: binding split by producer stage (§6.2 rewritten): contracts/findings bound at
   `conformance::run` with blame mined ONLY for the distinct cited `rel:line`s; drift claims bound
   inside the drift command. §7's claim rephrased honestly: blame on cited paths is NEW, bounded,
   disclosed git work.
2. **§6.4's `AppSite.name` premise softened — fixed.** `AppSite.name` is the MANIFEST name
   (artifactId / package.json `"name"` / dir basename, `cie-stack/src/detect.rs`), not
   `spring.application.name` (which the config pass does not read into `AppSite`). The Feign
   `name=` join keeps its name-based first attempt + path-literal fallback, with the divergence
   stated plainly.
3. **Persistence shape aligned with what RFC-0019 stage 1 actually shipped — fixed.** The draft
   proposed inventing a third standalone store trait; RFC-0019's shipped `StackStoreExt`
   (`cie-ingest/src/stack_store.rs`) is snapshot field + `revs/<N>/artifact.json` over the Store's
   generic `write_json`/`read_json`. §6.1 now mirrors exactly that (and the §4 non-goal was updated:
   RFC-0019 stage 1 has landed (`dc35647`); this RFC reuses the shipped pattern, not just its naming).

Also verified during the pass (all TRUE, no changes needed):

- `state_for()`'s `ModelState` = Observed/Assumed/Unmodelled exactly; a bucket-total of 0 is
  `Unmodelled` (the honest default the planner inherits).
- §6.3's load-bearing assumption holds: tests ARE summarized (`behavior.rs:281` tags `"test"` and
  keeps the symbol) — the reverse-index BFS reaches test callers with no behavior change.
- `LdfiPlanner` really needs zero changes: `new(lineage, injector)` (`:262`) + `run(&property,
  RoundLimits::default())` (`:270`) — the shipped unit test's exact call form.
- The polyglot fixture contains no outbound calls today (the claim's grep is scoped to the test
  file — accurate as written).
- All cited crates exist; `cie-fault`'s dependency set carries `cie-contract`, so §6.5 adds no
  crate dependency.
- Line-number drift on several citations (±3–8 lines, e.g. `from_effects` :181, `template_prov`
  :298, `merge` :257, `types_fresh` :225) — corrected where load-bearing (§5 table), immaterial
  elsewhere.
4. **§6.2's claim-type names corrected during vetting.** The draft cited a "`DriftClaim` (cie-ingest)"
   type — that name does not exist in the workspace. Actual types: `cie-contract/src/findings.rs:105`
   `Finding` (not "cie-analysis::findings" as drafted) and `cie-claims/src/model.rs:66` `Claim`.
   §6.2's text and test plan now cite the real names/paths.
