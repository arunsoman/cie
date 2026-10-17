# Local product workspace

Implementation base: `1dc9ffa397a47d80003af96e158eb48138c7a017`.
Requirements: [147 product stories](USER_STORIES.md). These stories are acceptance
obligations, not a statement that every capability is implemented.

## Run

```sh
cargo run -p cie-server --bin cie-workspace -- --allow-root /path/to/your/projects --data /private/workspace-data
# Open http://127.0.0.1:4317
```

The local browser workspace uses the JSON API and analysis services. It provides
repository/job controls, saved answers, evidence inspection, reviews, exports,
linked graph/table/outline views, feature planning and a story-status register. The app is single-user,
loopback-only, offline and has no hosted model egress. It does not merge, approve,
deploy, generate or apply source changes. The existing `cie` CLI still works.

## Service contracts

- Repository connections are canonicalized and constrained by explicit allowed
  roots. Indexing copies bounded supported sources into an isolated staging area.
- Engine cache writes occur in staging, not the connected repository. Only a
  completed, freshly checked job can atomically publish a snapshot. Cancellation
  blocks publication; native analysis may continue until its internal budget ends.
- A restart marks active jobs failed and does not automatically resume them.
- Exact snapshots back questions, tables, graphs, citations and JSON exports.
  Freshness is checked independently; static test reachability is not runtime
  assertion coverage and analyzer findings are not universal runtime proofs.
- Refutation hides claims and their incident edges, retaining historical evidence.
  Confirming a claim never promotes its epistemic class.
- Feature requests currently record acceptance criteria and lexical overlap hints.
  They are planning-only, not autonomous candidate implementations.

## HTTP API

The root page carries a per-process session token. Send it as `X-Cie-Token` on API
requests, with `Content-Type: application/json` for POST. Cross-origin requests,
foreign Host headers, duplicate headers and chunked request bodies are rejected.
The API is deliberately not suitable for remote multi-tenant deployment.

- `GET /api/state`, `/api/capabilities`, `/api/health`
- `POST /api/repositories` `{path,name?}`
- `POST /api/repositories/:id/index`, `/api/jobs/:id/cancel`
- `GET /api/repositories/:id`
- `POST /api/questions` `{repositoryId,question,intent?,symbolId?,parentAnswer?}`
- `GET /api/answers/:id`, `/api/answers/:id/export`
- `POST /api/reviews` `{answerId,itemId,verdict,reason}`
- `POST /api/search` `{query}` across connected indexed snapshots
- `POST /api/workspaces` `{name,answerIds,activeAnswer}`
- `POST /api/features` `{repositoryId,prompt,criteria}`
- `DELETE /api/repositories/:id`, `/api/answers/:id`

Supported deterministic intents: overview, search, callers, callees, impact,
tests, findings and concepts. A chosen intent is honored. Missing or ambiguous
symbols are not guessed. Arbitrary natural-language behavioral conclusions,
runtime causality, financial recovery guarantees and predictions remain unknown.

Limits: 32 repositories, 8 retained snapshots per repository, 2,000 supported files,
1 MiB per file, 16 MiB source data per capture, 8 queued/active jobs, 1,000 answers,
64 MiB persisted workspace, 16 concurrent HTTP connections, 1 MiB HTTP bodies.
Custom workspace data must sit outside connected repositories or use the excluded
`.cie-workspace` / `.cie-product` directory names. Symlinks are excluded. Git
repositories use Git's tracked/untracked list and ignore rules; non-Git folders
use operational exclusions and disclose that limit. No history/authors are sent.

The transport has input/session/origin protections, not an external penetration
test certification. Browser accessibility and the full 147-story acceptance
matrix require their own named tests and remain tracked separately.

## Backend increment validation

- `cargo test --workspace --locked`: **225 passed, 0 failed, 11 ignored**.
  Ignored performance/external-dependency tests were not executed.
- `cargo clippy -p cie-server --all-targets --no-deps --locked -- -D warnings`:
  passed for server targets; existing dependency warnings remain.
- New integration coverage: 21 product tests and 4 HTTP tests, including live
  Host/origin/session checks, cancellation publication, interrupted jobs, atomic
  persistence, immutable citations, stale sources and claim refutation.
- Browser acceptance and the complete story-by-story matrix are pending.

## Browser increment validation

- `node --test scripts/product/evidence.test.cjs`: 3 passed.
- `cargo test -p cie-server --locked`: 25 passed.
- `cargo clippy -p cie-server --all-targets --no-deps --locked -- -D warnings`: passed for server targets.
- Real headless Chrome against the compiled Rust server: **15 journeys passed**,
  including reload persistence, keyboard graph navigation, source inspection,
  refutation, export, saved workspaces, theme persistence and stale source checks.
  No API mocks were used.
- Reproduce: `cargo build -p cie-server --bin cie-workspace --locked`, then
  `node scripts/product/browser.cjs` with Playwright/Chromium installed. Set
  `CIE_CHROMIUM` to an alternative compatible Chrome/headless-shell binary.
  The test exits unsuccessfully when its browser cannot launch.
- [Full acceptance register](STATUS.md): 51 partial, 96 pending, 0 fully complete.
  Browser tests verify the delivered subset; they do not certify all criteria of
  the product-wide stories. The register lists remaining obligations explicitly.
