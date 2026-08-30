# Directory-listings drafts (roadmap R18 — DRAFT ONLY, nothing submitted)

> Drafted 2026-08-31 per R18's step 1 ("Draft (no submission)"). **Nothing
> below has been submitted anywhere** — per goal.md's external-action
> rule, submission waits on the repo owner's explicit go-ahead, and per
> R18's gating on R6: **that gate is now satisfied** (v0.1.0 released
> 2026-08-31), so these drafts are ready to apply the moment the
> go-ahead arrives. The README badge row (step 3) stays un-added until
> the listings are actually live — never link-to-404.

## 1. mcpservers.org — submission fields

(As of the form at mcpservers.org/submit; each field is real, sourced
from the shipped README on this commit.)

| Field | Value |
|---|---|
| Name | cie — Code Insight Engine |
| Tagline | The code graph that knows which tasks and tests implement your code |
| One-liner | 135-tool MCP server over a local SQLite or Neo4j code graph — callers/callees/call-graphs, task→file→test traceability, read-only by default; 9 languages, zero-config embedded mode, one static-HTML export. |
| Install | `pip install "cie-mcp[mcp]"` then `cie index <repo>` + `cie-mcp <repo> --embedded` |
| Transport | stdio (default) and streamable-HTTP (`--transport streamable-http --host 127.0.0.1 --port 8000`, localhost-only default) |
| Policy note | Server-side `ToolPolicy` (`--policy readonly` default; writes 403'd server-side per tool — machine-checked, docs/security.md) |
| Repository | https://github.com/kannamma-labs/cie |
| Docs / benchmarks | docs/benchmarks-requests.md, docs/benchmarks-urllib3.md, docs/competitor-benchmarks.md (each includes its published misses) |

Screenshot candidates: `demo.svg` (real MCP stdio transcript) and the
`docs/images/` export-html captures (`file://`, zero external requests).

## 2. awesome-mcp-servers — PR text

*(One line, per that repo's convention of a compact entry per server.
Section placement to be re-checked against the list's current table of
contents at PR time — sections get renamed.*)

**Proposed entry line:**

> - [cie](https://github.com/kannamma-labs/cie) — Code-insight MCP server over a local SQLite/Neo4j code graph: callers/callees with edge provenance, task→file→test traceability, GraphRAG QA, read-only by default, 9 languages, zero-config embedded mode, plus CLI and static-HTML export. Benchmark docs include published misses ([requests](https://github.com/kannamma-labs/cie/blob/main/docs/benchmarks-requests.md), [urllib3](https://github.com/kannamma-labs/cie/blob/main/docs/benchmarks-urllib3.md)).

**PR body (short):** adds a code-graph MCP server under the (check
current name at PR time) *Python / code-analysis* section; MIT;
stdio + streamable-HTTP; measured README claims with reproducible
benchmark scripts.

## 3. Post-listing tasks (blocked until both live)

- README "Listed on" badge row with links (never before live — no 404s).
- goal.md E3 close-out: capture PR URL + listing URL by hand.

## Provenance

- R18 plan (todo.md): step 1 = this file; steps 2–3 gated on go-ahead.
- R6 gate: satisfied 2026-08-31 (v0.1.0).
- Every install string above matches README's current (GitHub-install)
  instructions — checked same-day.