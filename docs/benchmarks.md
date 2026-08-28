# Benchmarks

**Run:** 2026-08-28, one pass, one target codebase. This is a first real
measurement, not a mature benchmark suite — published honestly, not as a
marketing number.
Treat this as a starting point to extend, not a final claim, and re-run
before citing these numbers anywhere that matters.

## Methodology

**Target:** `forge/` from [protobox](https://github.com/arunsoman/protobox)'s
`be-v2` backend — a real, in-use Python codebase, 36 files, 741 source
nodes, indexed via `cie index` in 0.87s (745 nodes / 1,656 edges / 910
resolved `calls` edges, using cie's own two-pass loader: extraction, then
`cie.callgraph.resolve_call_edges` for cross-file call resolution).

**Comparison:** for each task, the number of tool calls (and, where it
changes the picture, response size) needed to answer it two ways:

- **Naive** — what an agent with only `grep`/file-read has to do. Measured
  by actually running the equivalent shell commands against the real repo,
  not estimated.
- **cie** — the equivalent MCP tool call, run for real against the actual
  indexed graph (`cie-mcp forge/ --embedded`, `inspector` policy).

Three tasks, chosen to span "easy," "ambiguous," and "large file" — not
cherry-picked for the best number.

## Results

| Task | Naive | cie | Naive tool calls | cie tool calls |
|---|---|---|---|---|
| Find the definition of `AgentResult` | `grep -rn "class AgentResult"` — found directly, 1 file | `search_symbol("AgentResult")` | **1** | **1** |
| Find every real caller of `_post` | `grep -rln "_post("` matches 2 files, but can't tell a real call from a same-named unrelated method or the definition itself — reading both files to disambiguate is the honest next step | `callers("_post")` — resolved via the real call graph, not text matching | **3** (1 grep + 2 reads to confirm) | **1** |
| Understand `agent.py` (692 lines, 36,459 bytes) | Read the whole file | `file_skeleton("agent.py")` | **1** call, 36,459 bytes | **1** call, **50,504 bytes** |

## What this actually shows

- **Task 1 (easy case): a tie.** When the naive grep gets lucky and finds
  one unambiguous match, there's no call-count advantage. Not every task
  favors a graph — this one didn't move the needle.
- **Task 2 (ambiguous case): the real win, and it's about correctness, not
  just speed.** `grep` can't distinguish a real call site from an
  unrelated same-named method or the definition itself; disambiguating
  by hand costs at least 2 extra reads, and even then isn't guaranteed
  correct. `callers()` resolves through the actual extracted call graph —
  1 call, correct by construction. This is the honest shape of cie's
  advantage: fewer calls **and** a right answer instead of a
  probably-right one.
- **Task 3: a real loss, reported as found, not hidden.** `file_skeleton`
  returned MORE bytes than the raw file for this specific target — a
  densely-documented 692-line file with many small methods, where the
  JSON envelope (per-symbol docstrings, signatures, line ranges, a
  symbol index) outweighs the source it's describing. This is exactly the
  kind of result this project committed to publishing honestly
  rather than only reporting the favorable cases — CodeGraph's own
  published numbers included the same kind of disclosure ("near
  break-even on response-heavy ones"), which is part of what made that
  research treat their numbers as credible in the first place.

See [`competitor-benchmarks.md`](competitor-benchmarks.md) for the same
codebase indexed and queried with CodeGraphContext and Serena actually
installed and run — including a real bug this comparison surfaced in
cie's own ambiguous-name resolution, diagnosed precisely and since fixed.

## What this benchmark does not yet cover

- No token counts (byte counts are a proxy, not the same thing) and no
  real LLM-in-the-loop measurement — this is a structural tool-call/
  response-size comparison, the same kind of methodology CodeGraph's own
  headline numbers use, but a smaller, single-pass version of it.
- One target codebase, one pass, no variance across repo sizes/languages.
- No comparison against a live competitor tool (CodeGraphContext,
  CodeGraph) run against the same repo — a real next step, not attempted
  here.

## Reproducing this

```bash
pip install "cie[mcp]"
cie index /path/to/some/real/repo
cie-mcp /path/to/some/real/repo --embedded --policy inspector
# then call search_symbol / callers / file_skeleton over MCP and compare
# against the equivalent grep/read commands yourself
```
