# Repo trust signals — applied-ready drafts (roadmap R19)

> Drafted 2026-08-30, **applied 2026-08-31** (description, topics, all
> three starter issues — see stamps below). The release page (§3) rides
> the 0.1.0 release itself (R6); the PyPI upload under the name `cie` is
> blocked by a name collision — `pypi.org/project/cie` is unrelated
> (cluster311/cie10, ICD-10 codes; latest 0.208) — **resolved 2026-08-31:
> the distribution ships as `cie-mcp` from v0.1.1** (install strings in
> §1 updated same-day). Remaining [APPLY] items were run
> while the metadata's own claim set is already true of HEAD (135-tool
> surface, 9 languages, read-only default), keeping the honesty rule:
> metadata must match the artifact.

**Applied:** §1 description ✓ 2026-08-31 · §2 topics ✓ 2026-08-31 ·
§4 GFI-1 → issue [#17](https://github.com/kannamma-labs/cie/issues/17),
GFI-2 → [#18](https://github.com/kannamma-labs/cie/issues/18),
GFI-3 → [#19](https://github.com/kannamma-labs/cie/issues/19)
✓ 2026-08-31 · §5 README label link ✓ (5b527c4).

## 1. [APPLIED 2026-08-31] Repository description

```
The code graph that knows which tasks and tests implement your code —
MCP server, CLI, and HTTP tool surface over a local SQLite or Neo4j
graph. 9 languages, 135 LLM-callable tools, read-only by default.
```

(~180 chars; the first sentence is README's differentiator claim.)

## 2. [APPLIED 2026-08-31] Topics

```
mcp  code-graph  knowledge-graph  static-analysis  tree-sitter
llm-tools  ai-agents  neo4j  sqlite  developer-tools
```

(≤ 20 topics, all currently used on GitHub; drop `neo4j` only if the
tooling limits us to 10.)

## 3. Release page (0.1.0)

Render from CHANGELOG's `0.1.0` section, plus this caveat block at the
bottom (KEPT from the alpha — dropping it would be the launch-post
antipattern R20's gates exist to prevent):

> **Maturity caveats, stated:** single-maintainer, weeks-old codebase;
> benchmarks cover three curated task shapes on three repos (see
> docs/benchmarks*.md including the published misses); the ambiguous-
> caller heuristic has a real recall gap that is visible in tool output
> (`resolution.unresolved_call_sites`) rather than hidden; 5 of 135
> tools require the host LLM environment and say so with machine-
> readable `unavailable` reasons.

## 4. [APPLIED 2026-08-31] `good-first-issue` starters (2–3, against CONTRIBUTING's
second-maintainer path)

**GFI-1 — CLI tests for the remaining embedded query commands**
`tests/test_cli.py` covers `files/stats/callers/callees/skeleton/view-
file/path` on the embedded backend; `discover`, `communities`,
`community`, `health`, `node`, `neighbors`, `search`, `signature`,
`methods` and the tasks/coverage families still lack direct CLI-level
tests. Pattern to copy: `TestQuickstartParity` (CliRunner, --json,
ground-truth set equality, no monkeypatching of the openers). Safe
entry-point modules: `tests/`, `cie/cli.py` read paths only. Acceptance:
green suite, no prod-code changes, one test per command family.

**GFI-2 — `failing_context` ground-truth coverage for the heuristic
fallback**
`tests/test_graph_semantics_ground_truth.py` asserts the graph path's
answer sets; the heuristic fallback's `failing_context` (which serves
when the graph can't) has no equivalent truth assertions. Fixture in
`tests/test_graph_semantics_ground_truth.py`'s style, oracle = the test
author. Files: `tests/` only.

**GFI-3 — `docs/`: regenerate the urllib3 benchmark tables from
`scripts/benchmark.sh` on a fresh clone and diff against
`docs/benchmarks-urllib3.md`**
Proves R9's reproducibility claim on someone else's machine. Deliverable
= a dated re-run note or a mismatch report (both are valuable — "the
numbers didn't reproduce" would be REQUIRED to publish, per the
honesty rules). Files: `docs/benchmarks-urllib3.md` (append a dated
section), `scripts/`.

## 5. [APPLIED] README trust-signal links — landed in 5b527c4

## Provenance

- CONTRIBUTING's second-maintainer path (C4) already landed — GFI texts
  reference its safe-entry list directly.
- Application steps are mechanical (repo settings / gh CLI); they gate
  on R6 (stable cut) per the trust-claims rule: metadata matches the
  artifact at cut time, not before.