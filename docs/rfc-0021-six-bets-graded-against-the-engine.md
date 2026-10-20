# RFC-0021: Six go-to-market bets, graded against what the engine already does

**Status:** draft
**Author:** the CIE maintainer (with pi)
**Date:** 2026-10-20
**Applies to:** `cie-server`, `cie-drift`, `cie-claims`, `cie-adapters`, `cie-analysis`, `cie-docs`, `docs/`, the web app
**Related:** RFC-0019 (stack-at-ingest, revision store, C4 concept tiers), RFC-0020 (five unexported engine claims), US-134 (MCP server "Partial"), PRD-v1.0 CAP-2.1–2.3 / CAP-2.11 / CAP-4.2 / CAP-7.8 / CAP-7.9, FR-3 (adapter contract), FR-42 (every edge carries a reason)

---

## 1. Summary

A six-point product-strategy brief proposed: (1) trustworthy code intelligence for AI coding agents, (2) an MCP/agent tool surface over the engine, (3) document–code drift as an enterprise wedge, (4) security/testing capabilities mapped onto CI/SARIF tooling, (5) a community adapter marketplace, (6) integration with three sibling projects (Nirdosha, Atomic Forge, agentbox).

Reading the engine crate-by-crate against each point produces six different verdicts, not one:

1. **Agent trust (1) and MCP (2) are the same bet, and it's already half-built.** `cie-server/src/mcp.rs` (347 lines) is a real, tested JSON-RPC 2.0 handler with 4 declared tools and an honesty envelope (`claim_class`/`citations`/`gaps`/`withheld`/`staleness`) wrapping every result — tracked as **`US-134: Partial`**. What's missing is a real `ToolExecutor` (only a test `FakeExecutor` exists) and a stdio transport binary. §6.1 finishes it; nothing here is new invention.
2. **Document drift (3) has a real mechanical verification engine** (9-status model, multi-doc reconciliation, reverse drift) but two concrete gaps block the "continuous compliance" pitch: no OpenAPI/schema-vs-impl comparison exists anywhere, and 2 of the 9 `DriftStatus` variants (`Drifted`, `Undocumented`) are declared and consumed downstream but **never constructed**. §6.2 closes both.
3. **Security/testing → CI (4) is mostly real, two names in the brief are not.** Secrets, taint-lite, N+1, and minimum-test-set are genuine, and SARIF export already covers *all* finding kinds generically (confirmed — not drift-only, as the file's crate location might suggest). "Layering violations" and "blast radius" are not real, distinct capabilities today: layering only exists as JVM-specific role naming, and blast radius is just an informal name for `impact`'s existing output. §6.3 builds the cheap one for real and retires the other as a separate claim.
4. **The adapter marketplace (5) premise is already true.** The extension contract is exactly as clean as the brief hopes — one adapter crate, one registration line, zero core changes, MIT license, no CLA. The gap is 100% contributor-facing surface (no `CONTRIBUTING.md`, no template). §6.4 is documentation and tooling, not an engine change.
5. **Adjacent-project integration (6) cannot be scoped from this repository.** Nirdosha, Atomic Forge, and agentbox are referenced nowhere in this codebase — their interfaces are unknown here. §6.5 names the one integration surface CIE can honestly offer (the MCP surface from §6.1) and explicitly defers the rest to a scoping conversation rather than inventing specifics.

---

## 2. Motivation

### 2.1 What "trustworthy for agents" already has a foundation

`cie-server/src/mcp.rs:134-232` (`handle_message`) supports `initialize`/`ping`/`tools/list`/`tools/call`. `TOOLS` (`:41-62`) declares `cie_report`, `cie_impact`, `cie_concepts`, `cie_stories_coverage`. `ToolExecutor` (`:72-81`) is the engine seam. `honesty_envelope()` (`:236-254`) wraps every result in the exact evidence-discipline vocabulary this project already enforces everywhere else — before the payload, not as an afterthought. `McpBudget`/`ToolError::BudgetExceeded` make an exhausted budget an explicit error, never a fabricated partial answer. Four unit tests pass today, against a `FakeExecutor`. `cie-cli` has zero dependency on `cie-server` — the two have never been connected.

### 2.2 Two real gaps in the drift wedge

`extract_claims` (`cie-drift/src/pipeline.rs:21-75`) parses **Markdown/plain-text only** — headers set a section, bullets/modality words (`must/shall/should/may/will/never/always`) trigger extraction. Zero hits anywhere for `openapi|swagger|OAS`. The PRD names this exact gap itself: `docs/PRD-v1.0.md:581`, **CAP-7.8 "Schema-vs-impl drift | OpenAPI/Protobuf/GraphQL vs summaries"** — a stated goal, unimplemented.

Separately, `DriftStatus` (`cie-claims/src/model.rs:87-97`) has 9 variants: `Implemented, Partial, Missing, Contradicting, Drifted, Undocumented, Unverifiable, Unknown, Superseded`. Two of them — `Drifted` and `Undocumented` — are matched (consumed) downstream at `cie-analysis/src/risk.rs:130` and `cie-drift/src/causal.rs:111/119`, and even round-trip through `as_str()` (`model.rs:106-107`) — but grepping for `DriftStatus::Drifted` or `DriftStatus::Undocumented` as a *constructor* (not a match arm) returns **zero hits**. The regression signal PRD CAP-7.9 ("continuous compliance — drift on every commit") depends on, is structurally wired for and never populated.

(One correction to an earlier research pass this session: PDF/OCR ingestion, previously assumed deferred to v1.0 per `docs/TRACEABILITY.md`, is in fact real — `cie-docs/src/pdf.rs` (298 lines) and `cie-docs/src/ocr.rs` (141 lines) exist, with an honest `SystemTesseract`/`NoOcr` seam that records `ocr_unavailable` rather than inventing text.)

### 2.3 Two capability names in the brief that don't correspond to real, distinct capabilities

Verified real: **secrets** (`cie-analysis/src/security.rs:20-65`, a genuine regex/heuristic scanner — distinct from the `MustNotLeakSecrets` *obligation kind*, which has no detection logic of its own), **taint-lite** (`security.rs:144-196`, intra-procedural, name-based — the doc comment admits the limitation explicitly) plus inter-procedural IFDS taint (`cie-analysis/src/ifds.rs:303-520`), **N+1** (`security.rs:233-269`, a real loop+query-call heuristic), and **minimum test set** (`cie-analysis/src/testing.rs:91`). **SARIF** (`cie-drift/src/sarif.rs:8`, `findings_to_sarif(store, findings: &[Finding])`) is genuinely SARIF 2.1.0-shaped (`ruleId`/`level`/`physicalLocation`/`partialFingerprints`) and — confirmed by reading `Command::Analyze`'s `OutFormat::Sarif` arm (`cie-cli/src/main.rs:666-672`) — it serializes `outcome.snapshot.findings`, the **full** findings list (secrets + taint + N+1 + dead-code classifications together), not a drift-only export. This is stronger than the brief's framing suggested.

Not real: **"layering violations"** has no general checker anywhere — only `layer_of_class` (`cie-stack/src/spring.rs`), a JVM-specific *role-naming* classifier, not a cross-layer-call rule engine. **"Blast radius"** is not a named concept in the codebase at all (zero hits for `blast_radius`/`BlastRadius`); it is purely the plain-English doc comment on `Command::Impact` (`main.rs:161`, "Blast radius + minimum test set for a symbol") describing `impact`'s real output (`affected_by_change` + `minimum_test_set`). Exporting "blast radius" as if it were a second, distinct metric would misrepresent the engine the same way RFC-0020 found `StateModel`/`Calibration` risked doing — this RFC avoids repeating that mistake.

**Dead code** is also thinner than it sounds: `FindingKind::DeadCode` (`cie-contract/src/findings.rs:43,68,92`) is a declared classification bucket grouped with `OrphanCode`/`CoverageGap`, not a verified reachability detector in its own right. Noted as an open question (§10), not a §6 design item — strengthening it isn't required to make this RFC's goals land.

### 2.4 The adapter contract is already exactly as clean as the brief hopes

`LanguageAdapter` (`cie-adapters/src/lib.rs:17-66`) requires exactly 4 methods (`id`, `extensions`, `analyzer`, `extract`); everything else — dispatch kind, numeric semantics, framework conventions, error model, parse diagnostics — defaults to a sensible unspecified value. The crate's own top-of-file doc comment states the design intent verbatim: *"adding a language = one adapter + ONE registration line in `default_registry()` + zero core changes."* Registration is confirmed literally one line (`cie-ingest/src/lib.rs:755-770`), ending in a literal invitation comment: `// New language goes HERE`. The smallest real adapter, Go (306 lines), is roughly 65% genuine tree-sitter extraction logic and ~18% trait-impl glue — a minimal viable adapter is real work, not mostly plumbing, which is a *good* signal for contribution quality. `LICENSE` is MIT, no CLA. The only thing missing is contributor-facing surface: no `CONTRIBUTING.md` exists anywhere in the repository.

### 2.5 Adjacent-project integration: no linkage exists here

Nirdosha, Atomic Forge, and agentbox are not referenced anywhere in this repository — by construction, since they're separate projects. This RFC does not attempt to design their integration from the brief's one-line descriptions; doing so would mean inventing interfaces rather than reading them. §6.5 is scoped accordingly.

---

## 3. Goals

1. A real `ToolExecutor` wired to `cie_ingest`/`cie_analysis` engine calls for the 4 existing MCP tools; existing budget/error handling stays as-is.
2. A stdio transport binary so an agent (Claude Code, Cursor, or any MCP client) can launch CIE as an MCP server over the standard stdio transport.
3. Expand the tool list to cover `drift` and `stack` output, gated on the existing CLI code paths — no new engine capability, just new routing.
4. Repeated MCP calls against an already-ingested, unchanged repo reuse the `.cie/revs/<N>/` revision store (RFC-0019) — no silent re-ingest per tool call.
5. OpenAPI/Swagger spec ingestion joined against `RouteSite` (from `cie-stack`), producing real schema-vs-impl `DriftClaim`s with real `Implemented`/`Missing`/`Undocumented` outcomes.
6. `DriftStatus::Drifted` becomes a real, constructed outcome: a claim that regressed from `Implemented` in a prior revision.
7. Build a real layering-violation checker; stop treating "blast radius" as a separate capability from `impact`'s existing output.
8. Ship `CONTRIBUTING.md` plus an adapter template so a third party can add a language without reading the whole `cie-adapters` crate first.
9. Scope — not design — the adjacent-project integration question; no code changes for item 6 under this RFC.

## 4. Non-goals

- **Not** rewriting the MCP tool JSON schema or switching transports — `cie-server::mcp` already picked a real, tested shape (JSON-RPC 2.0, the honesty envelope); this RFC extends it, never replaces it.
- **Not** rebuilding taint/dataflow analysis — taint-lite and the IFDS-based inter-procedural taint already exist and are explicitly out of scope here.
- **Not** promising a `blast_radius` metric that's mathematically different from `impact`'s existing affected-set — if the name is kept anywhere externally, it must point at the same `affected_by_change` call, never a second implementation.
- **Not** designing Nirdosha/Atomic Forge/agentbox integration specifics — no interface for any of them exists in this repository to design against (§2.5, §10).
- **Not** building OpenAPI ingestion beyond path+method matching in v1 — request/response schema-field-level diffing is future work (§10).
- **Not** making dead-code detection a real reachability checker in this RFC (§2.3) — listed as a follow-up, not required for §3's goals.

---

## 5. Current-state inventory (with references)

| Fact | Reference |
|---|---|
| MCP JSON-RPC handler, real and tested | `cie-server/src/mcp.rs:134-232 handle_message`, 4 passing unit tests |
| 4 declared tools | `cie-server/src/mcp.rs:41-62 TOOLS` |
| Honesty envelope wraps every tool result | `cie-server/src/mcp.rs:236-254 honesty_envelope` |
| `ToolExecutor` trait — the engine seam | `cie-server/src/mcp.rs:72-81` |
| Only a `FakeExecutor` exists; no real engine wiring | confirmed via grep, zero other impls |
| `cie-cli` doesn't depend on `cie-server` | confirmed, zero cross-references outside `cie-server`/`cie-stories`/one workspace contract test |
| Registered Partial in the story registry | `cie-stories/src/registry.rs:370 US-134` |
| `tokio` already a workspace dependency (optional `runtime` feature) | `cie-server/Cargo.toml`, workspace `Cargo.toml:81` |
| Revision store is store-first/compute-if-absent already (precedent for cheap repeat calls) | `cie stack`'s `--force` flag, `cie-cli/src/main.rs:74-78` |
| Drift claim extraction is Markdown/plain-text only | `cie-drift/src/pipeline.rs:21-75 extract_claims` |
| No OpenAPI/Swagger/Protobuf/GraphQL ingestion anywhere | confirmed, zero hits for `openapi\|swagger\|OAS` |
| PRD names this exact gap as a goal | `docs/PRD-v1.0.md:581 CAP-7.8` |
| PDF/OCR ingestion is real (correction to an earlier "deferred" read) | `cie-docs/src/pdf.rs` (298 lines), `ocr.rs` (141 lines), honest `NoOcr` fallback |
| `DriftStatus` — 9 variants | `cie-claims/src/model.rs:87-97` |
| `Drifted`/`Undocumented` declared + consumed downstream, never constructed | consumers: `risk.rs:130`, `causal.rs:111/119`; zero constructor sites |
| `verify_claims` — real mechanical verification, not a stub | `cie-drift/src/pipeline.rs:262-599` |
| Secrets scanner — real, regex + heuristics | `cie-analysis/src/security.rs:20-65` |
| Taint-lite (intra-proc) + IFDS taint (inter-proc) | `security.rs:144-196`, `ifds.rs:303-520` |
| N+1 detector — real | `security.rs:233-269` |
| Dead code — a classification, not a reachability detector | `cie-contract/src/findings.rs:43,68,92` |
| Layering violations — does not exist as a general checker | confirmed; only JVM-specific `layer_of_class`, `cie-stack/src/spring.rs` |
| Blast radius — not a named concept; informal name for `impact`'s output | confirmed zero hits; doc-comment-only at `main.rs:161` |
| Minimum test set — real | `cie-analysis/src/testing.rs:91` |
| SARIF export is generic over ALL findings, not drift-only | `cie-drift/src/sarif.rs:8 findings_to_sarif`; invoked from `Command::Analyze`'s `OutFormat::Sarif`, `main.rs:666-672`, passing the full `outcome.snapshot.findings` |
| `LanguageAdapter` trait — 4 required methods, rest defaulted | `cie-adapters/src/lib.rs:17-66` |
| Registration is one line, with a literal invitation comment | `cie-ingest/src/lib.rs:755-770 default_registry()` |
| Smallest adapter (Go, 306 lines) is ~65% real extraction logic | `cie-adapter-go/src/lib.rs:29-217` |
| No `CONTRIBUTING.md` anywhere | confirmed |
| `LICENSE` is MIT, no CLA | `LICENSE` |
| No reference to Nirdosha/Atomic Forge/agentbox anywhere in this repo | confirmed by construction |

---

## 6. Design

### 6.1 Finish the MCP server — real executor, stdio transport, wider tool coverage

**What exists:** `ToolExecutor` (`cie-server/src/mcp.rs:72-81`) is the exact seam a real implementation plugs into; `handle_message` is already transport-agnostic ("pure and testable" by design) and already does dispatch + budget enforcement + honesty-envelope wrapping. Only `FakeExecutor` (test-only) implements the trait today.

**New: `RealExecutor`** (`cie-server/src/executor.rs`, new module):

```rust
pub struct RealExecutor { pub budget_ms: u64 }

impl ToolExecutor for RealExecutor {
    fn call(&self, tool: &str, args: &ToolArgs) -> Result<Value, ToolError> {
        let path = args.path()?;
        let opts = IngestOptions { budget_ms: self.budget_ms, ..Default::default() };
        let outcome = cie_ingest::run_ingest(&path, &opts)
            .and_then(cie_ingest::materialize)
            .map_err(|e| ToolError::Engine(e.to_string()))?;
        match tool {
            "cie_report" => Ok(serde_json::to_value(&outcome.snapshot)?),
            "cie_impact" => {
                let symbol = args.get_str("symbol")?;
                // identical sequence to Command::Impact, main.rs:831-870:
                // find the symbol, affected_by_change, minimum_test_set, affected_concepts
                Ok(impact_json(&outcome, &symbol)?)
            }
            "cie_concepts" => Ok(concepts_json(&outcome, args.get_str("concept").ok())?),
            "cie_stories_coverage" => Ok(cie_stories::coverage_json()?),
            other => Err(ToolError::UnknownTool(other.into())),
        }
    }
}
```

Each branch reuses the exact call sequence `cie-cli/src/main.rs` already runs for the corresponding command — no new engine logic, only routing into code that's been shipping since before this RFC.

**New: stdio transport binary**, `cie-server/src/bin/cie-mcp.rs`:

```rust
fn main() {
    let executor = RealExecutor { budget_ms: 20_000 };
    for line in std::io::stdin().lines().flatten() {
        println!("{}", cie_server::mcp::handle_message(&executor, &line));
    }
}
```

One JSON-RPC message per input line, one per output line — the standard MCP stdio shape. This binary is the *only* new transport-specific code; `handle_message` itself needs zero changes.

**Caching (Goal 4):** before calling `run_ingest` fresh, check `.cie/revs/<head>/` the same way `Command::Stack`'s store-first path already does (`main.rs:74-78` precedent). An agent calling `cie_report` twenty times against an unchanged repo during one coding session should cost twenty cheap reads, not twenty re-ingests.

**Tool expansion (Goal 3):** add `cie_drift` (wraps the existing drift pipeline output) and `cie_stack` (wraps `cie-stack`'s report, once RFC-0019's persistence lands) as new `ToolDecl` entries in `TOOLS` plus new `match` arms in `RealExecutor::call` — no new engine capability.

**Failure policy:** budget exceeded → `ToolError::BudgetExceeded` (already modeled, unchanged); ingest failure (not a repo, permission denied) → `ToolError::Engine(..)`, surfaced through the existing honesty envelope's `gaps`/`withheld` fields — never a silently empty tool result.

### 6.2 OpenAPI join + real `Drifted`/`Undocumented`

**OpenAPI/Swagger ingestion** (closes PRD CAP-7.8). New function, living wherever claim extraction already does (`cie-drift/src/pipeline.rs`):

```rust
pub fn claims_from_openapi(spec: &Value, doc_path: &str) -> Vec<DriftClaim> {
    let mut claims = vec![];
    if let Some(paths) = spec.get("paths").and_then(|p| p.as_object()) {
        for (path, methods) in paths {
            for (method, _) in methods.as_object().into_iter().flatten() {
                if !["get","post","put","patch","delete"].contains(&method.as_str()) { continue; }
                claims.push(DriftClaim {
                    text: format!("{} {} must be implemented", method.to_uppercase(), path),
                    doc: doc_path.into(),
                    kind: ClaimKind::RouteRequired { path: path.clone(), method: method.clone() },
                    ..Default::default()
                });
            }
        }
    }
    claims
}
```

v1 needs no schema library: `paths` is a flat map, walked with `serde_json::Value` (already a dependency used everywhere in this workspace).

**The join**, a new `verify_claims` match arm for `ClaimKind::RouteRequired`, resolved against `report.routes` (the `RouteSite` list `cie-stack` already produces, per RFC-0020 §6.4):

- a matching `RouteSite` exists → `DriftStatus::Implemented`, evidence = the route's `rel:line`;
- no match → `DriftStatus::Missing`;
- a route exists that the spec doesn't name → the reverse sweep below marks it `Undocumented`.

This requires `StackReport` to be available at drift-verification time — the exact dependency RFC-0019 already establishes (stack persisted per-revision via `StackStoreExt`); no new persistence seam, just a new consumer of one RFC-0019 already names.

**Populate `Drifted`** — a claim that *was* `Implemented` in the prior revision and no longer is:

```rust
// cie-drift/src/pipeline.rs, after build_report produces the current report
pub fn mark_regressions(current: &mut DriftReport, previous: &DriftReport) {
    for claim in &mut current.claims {
        if let Some(prior) = previous.claims.iter().find(|p| p.doc == claim.doc && p.text == claim.text) {
            if prior.status == DriftStatus::Implemented && claim.status != DriftStatus::Implemented {
                claim.status = DriftStatus::Drifted;
            }
        }
    }
}
```

Wiring: `run_ingest` already has the prior revision's persisted drift report available via `DriftStoreExt` (named as an existing extension trait in RFC-0019 §5's precedent table) — load it, call `mark_regressions` right after `build_report`.

**Populate `Undocumented`** — the mirror of `Missing`: inside the OpenAPI join, after matching claims to routes, sweep `report.routes` for any route with no claim naming its `(path, method)` and emit a synthetic claim with `status: DriftStatus::Undocumented`. The existing non-route reverse-drift sweep (`pipeline.rs:655-684`) does the equivalent unmapped-symbol walk already — confirm its current placeholder status before reusing the pattern (§10 open question 1).

**Failure policy:** no OpenAPI doc found → no `RouteRequired` claims are generated; routes are still checked via the reverse sweep. No prior revision (first-ever ingest) → `mark_regressions` is a no-op — `Drifted` legitimately never appears on revision 1; that's correct, not a bug.

### 6.3 Security/CI mapping — layering for real, retire "blast radius" as a separate claim

**Already solid, no change needed:** secrets, taint-lite + IFDS taint, N+1, and minimum-test-set are real (§2.3), and SARIF already exports every finding kind generically via `Command::Analyze`. Nothing to build here — §9 notes this as a correction to the original brief's framing, not a gap to close.

**Layering violations — build for real.** Once RFC-0019's C4 Container/Component tiers land in the concept DAG, a layering rule is a short reachability check over concept edges, deliberately scoped to the naming-convention-labelled tiers RFC-0019 §6.4 already produces (never a hardcoded "UI"/"DB" vocabulary):

```rust
// cie-analysis/src/layering.rs (new)
pub struct LayerRule { pub from_tier_name: &'static str, pub must_not_call: &'static str }
pub const DEFAULT_RULES: &[LayerRule] = &[]; // empty until component labels are trusted — see §10.2

pub fn layering_violations(concepts: &ConceptDag, calls: &RelationGraph, rules: &[LayerRule]) -> Vec<Finding> {
    // for each Calls edge, resolve both ends' owning Container/Component concept
    // node, check its label against `rules`; a violation becomes a Finding with
    // both symbols + both concept labels as evidence, Authority::InferredCandidate
    // (layer labels are naming-convention-derived, never Verified)
}
```

Depends on RFC-0019 landing (§10, §11).

**"Blast radius" — retire as a separate claim, not a separate feature.** No new code: `impact`'s existing `affected_by_change` + `minimum_test_set` output already *is* blast radius. The only change here, if any, is in external description — never a second implementation of the same metric under a different name.

### 6.4 Adapter marketplace — contributor surface, zero engine change

**Already true, confirmed (§2.4):** the extension contract is exactly as clean as the brief hopes. Nothing engine-side changes.

1. `CONTRIBUTING.md` (repo root, new file): fork → `cargo new cie-adapter-ruby` → implement the 4 required `LanguageAdapter` methods, pointing at `cie-adapter-go` as the worked example (confirmed the smallest real one, ~65% genuine logic) → add the one `default_registry()` line → `cargo test -p cie-adapter-ruby`.
2. `cie-adapter-template` (new workspace crate, excluded from `default_registry()`): a copy of `cie-adapter-go`'s skeleton with its tree-sitter-specific extraction stripped to `todo!()` stubs, comments pointing at each of the 4 required methods.
3. One doc-comment line on `LanguageAdapter` itself (`cie-adapters/src/lib.rs:17`) linking to `CONTRIBUTING.md` — today the trait's doc comments are implicitly the only onboarding material that exists.

**Scope discipline:** none of this changes `LanguageAdapter`'s signature, `default_registry()`'s shape, or any shipped adapter — a documentation-and-template-only change set, reviewable independent of every other item in this RFC.

### 6.5 Adjacent-project integration — scoped, not designed

Nirdosha, Atomic Forge, and agentbox are referenced nowhere in this repository (§2.5). This RFC does not specify their integration — doing so would mean inventing interfaces rather than reading them.

**What can be said from this repo alone:** the one integration surface CIE already has, or will have once §6.1 lands, is the MCP tool surface plus the typed JSON artifacts underneath it (`report.json`, `concepts.json`, drift output, `stack.json`). Any sibling project that can speak MCP or read CIE's JSON artifacts gets integration "for free," the same way any other MCP client would. Beyond that, this RFC takes no position on what the three projects actually need, and recommends a short scoping conversation — reading each project's own current public interface — before any design work starts.

---

## 7. Performance & budget

- §6.1: no new engine work — `RealExecutor` reuses existing `run_ingest`/`materialize`/command logic verbatim; the revision-store cache (already real, RFC-0019) is what keeps repeated agent calls cheap, not new caching code.
- §6.2: OpenAPI claim extraction is a flat-map walk over one small document — negligible; `mark_regressions` is `O(claims)` per revision, a string-keyed lookup against the prior revision's claim list.
- §6.3: `layering_violations` is one pass over `RelationGraph`'s edges, already bounded elsewhere (the 1M-edge gate noted in RFC-0019's precedent table) — no new unbounded traversal.
- §6.4: zero runtime cost — docs plus a template crate excluded from the default registry, so it doesn't compile into any shipped binary unless a contributor explicitly builds it.
- §6.5: no engine work in this RFC.

---

## 8. Test plan

| Level | Gate |
|---|---|
| Unit (`cie-server`) | existing 4 MCP handler tests stay green unchanged; **NEW**: `RealExecutor` against a tiny fixture repo returns the same JSON `cie report`/`cie impact` would print for that repo |
| **NEW** integration (`cie-server`) | the stdio binary round-trips one `tools/call` message end-to-end against a real small repo |
| **NEW** unit (`cie-drift`) | `claims_from_openapi` on a 3-route OpenAPI fixture produces 3 `RouteRequired` claims; join against a `StackReport` with 2 matching + 1 missing route produces 2 `Implemented` + 1 `Missing`; an unmatched real route produces 1 `Undocumented` |
| **NEW** unit (`cie-drift`) | `mark_regressions`: a claim `Implemented` in revision N, `Missing` in N+1 → `Drifted`; first-ever revision (no prior) → no-op, zero `Drifted` claims |
| **NEW** unit (`cie-analysis`) | `layering_violations` on a 2-tier synthetic concept DAG with one cross-tier call flags exactly it; zero violations when no rule matches or `DEFAULT_RULES` is empty |
| Compatibility | every new field/status value is additive; a pre-RFC snapshot with no prior-revision drift data never fabricates a `Drifted` claim |
| Honesty | `layering_violations` findings carry `Authority::InferredCandidate`, never `Verified` — naming-convention-derived labels can't earn a stronger authority than their source |

---

## 9. Alternatives considered

| Alternative | Why not (now) |
|---|---|
| Build a brand-new MCP server from scratch instead of finishing `cie-server::mcp` | It already exists, is tested, and already encodes the honesty-envelope discipline this RFC wants. Starting over discards real, correct work. |
| Treat "layering violations" and "blast radius" as equally real and export both | `blast_radius` has zero corresponding code anywhere; exporting it as a distinct metric would be the unevidenced-claim mistake RFC-0020 was written to retire. Building the real one (layering) and retiring the fictional one (blast radius) is the honest version of "ship both." |
| Full JSON-Schema-level OpenAPI diffing (request/response field types) in v1 | A materially bigger parser/diff problem than path+method matching. v1 ships the enterprise-wedge-relevant signal (does the endpoint exist at all) now; field-level diffing is future work (§10). |
| Design the Nirdosha/Atomic Forge/agentbox integration now, from the brief's one-line descriptions | Those descriptions aren't an interface spec. Guessing one and building against it risks the wrong seam entirely; a short scoping pass costs far less than a wrong integration. |

---

## 10. Open questions

1. **Reverse-drift's current placeholder status** — `pipeline.rs:655-684`'s unmapped-symbol sweep needs to be read in full before §6.2 lands, to confirm exactly what status it assigns today versus the `Undocumented` this RFC wants, and whether changing it breaks any existing consumer.
2. **Layering rule configuration** — user-configurable per repo (`.cie/layers.toml`?) from day one, or ship with an empty `DEFAULT_RULES` until RFC-0019's component-role labels are mature enough to trust? This RFC leans toward the latter — safer, less likely to produce a noisy false positive on day one.
3. **OpenAPI version coverage** — v1 targets OpenAPI 3.x `paths`; Swagger 2.0's older shape (`basePath` + relative paths) needs its own small reader, scoped against whichever format a real pilot actually uses.
4. **The Nirdosha/Atomic Forge/agentbox scoping conversation** — owner and timing not set here; this RFC only names it as a prerequisite to any design (§6.5).

---

## 11. Build order (each stage shippable + revertable)

| # | Stage | Size |
|---|---|---|
| 1 | §6.1 — `RealExecutor`, stdio binary, cache-aware repeat calls (existing 4 tools only) | M |
| 2 | §6.4 — `CONTRIBUTING.md` + adapter template crate | S |
| 3 | §6.2 — OpenAPI claim extraction + route join (`Implemented`/`Missing`/`Undocumented`) | M |
| 4 | §6.2 — `mark_regressions` (`Drifted`) | S (depends on #3) |
| 5 | §6.1 — tool expansion (`cie_drift`, `cie_stack`) | S (depends on #1, #3) |
| 6 | §6.3 — `layering_violations` | M (external dependency: RFC-0019's C4 tiers) |
| 7 | §6.5 — scoping conversation, no code | — |

Stages 1–4 have no dependency on RFC-0019/0020 landing first; stage 6 is explicitly blocked on RFC-0019; stage 5's `cie_stack` tool is blocked on stage 3's route data being meaningful to expose.

---

## 12. Why this is safe for maintainers

- **The riskiest-looking item is the most de-risked.** `cie-server::mcp`'s handler, tool schema, and honesty envelope are already written and already tested — §6.1 adds a real executor and a transport loop around code that doesn't change.
- **Every drift addition degrades safely.** No OpenAPI doc → no `RouteRequired` claims, not a crash. No prior revision → no `Drifted` claims, correctly — there's nothing to regress from yet.
- **This RFC corrects two claims in its own source brief rather than quietly building around them.** "Layering violations" and "blast radius" were treated as equally real in the original brief; this document builds the one that's genuinely missing and cheap, and explicitly retires the other as a separate claim rather than writing code to match a name that was never backed by anything.
- **§6.5 is honest about its own limits.** Rather than produce a confident-sounding integration design for three projects this repository has no visibility into, it names the one real integration surface (MCP) and defers the rest — the same evidence discipline the engine enforces on its own output, applied to this document about the engine.
- **The rollback path is real.** Every stage in §11 is independently revertable; §6.4 doesn't touch the engine at all; §6.1's new executor sits behind the existing, unchanged `ToolExecutor` trait.
