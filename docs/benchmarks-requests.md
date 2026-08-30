# Benchmarks — a second dataset (psf/requests)

**Run:** 2026-08-29. `docs/benchmarks.md`'s dataset was one repo
(`protobox`'s `forge/`, not public) and its hook proof case
(`docs/competitive-landscape.md`'s strength #1) is Nirdosha, a
from-scratch language by this project's own author — a fair critique is
that neither is proof on code a skeptical reader already runs. This is
that second data point: a well-known, widely-used public repo, indexed
and queried for real, same methodology as `docs/benchmarks.md`. Treat
both documents together as the current evidence, not this one alone —
one repo still isn't a distribution, and that gap is named honestly here
rather than closed by this file.

## Methodology

**Target:** [`psf/requests`](https://github.com/psf/requests) at commit
`5460f467`, `src/requests/` — 15 files, 6,874 lines of real, in-production
Python (the HTTP library most of the Python ecosystem depends on).
Indexed via `cie index .`: 37 files (includes `tests/`), 858 nodes, 1,822
edges (875 resolved `calls` edges), in well under a second.

**Comparison:** same two-column shape as `docs/benchmarks.md` — naive
(`grep`/file-read, commands actually run against the real clone) vs cie
(the equivalent MCP tool call, run for real via
`cie-mcp . --embedded`-equivalent `ToolService` calls against the actual
indexed graph). Not cherry-picked for the best number — chosen to
mirror the same three task shapes the first benchmark used (easy,
ambiguous, large file), then reported as found.

## Results

| Task | Naive | cie | Naive calls | cie calls |
|---|---|---|---|---|
| Find the definition of `PreparedRequest` | `grep -rn "class PreparedRequest"` — 1 file, unambiguous | `search_symbol("PreparedRequest")` | **1** | **1** |
| Find every real call to `close()` (defined 4x: `HTTPAdapter`, `BaseAdapter`, `Response`, `Session`) | `grep -rn "\.close()" src/requests/*.py` — 6 real call sites, no receiver-type info | `callers("close")` — resolved via the real call graph | **1**, finds **6** | **1**, finds **3** |
| Understand a large file (`models.py`, 1,184 lines, 41,462 bytes) | Read the whole file | `file_skeleton("models.py")` | **1** call, 41,462 bytes | **1** call, **17,887 bytes** |

**Re-measured 2026-08-30 (R7):** that ambiguous-caller gap is now
*visible in the tool's own output* — `callers("close")` carries a
`resolution` block (persisted per-name call-site tallies): this repo's
live numbers are `total_call_sites: 19, unresolved_call_sites: 16,
resolved_edges: 3`. The name-keyed denominator is broader than the
doc-table's 6 adapter-close sites (any `.close()` on any object counts
under the name), so the two figures aren't the same fraction — read
them together: the doc counts adapter-relevant sites; the tool counts
every call site sharing the name. Both are honest; neither is hidden.

## What this actually shows

- **Task 1 (easy): a tie**, same as the first benchmark's Task 1 — an
  unambiguous grep and an unambiguous `search_symbol` both cost one call.
  Consistent across two independent datasets now: the easy case doesn't
  favor either approach.
- **Task 2 (ambiguous): a genuine mixed result, not a clean win this
  time — reported because it's true, not adjusted to look better.**
  `close()` is really defined four times in `src/requests/` (two
  `Adapter` classes, `Response`, `Session`); `grep` finds all 6 real
  invocation sites in one call but tells you nothing about which class's
  `close()` each one calls. `callers("close")` resolves through the
  actual call graph and is right every time it answers — zero false
  positives, unlike the first benchmark's precision-only framing this
  extends — but it only resolved **3 of the 6** real call sites in this
  codebase (`self.close()` inside `Session.close()` itself, and three
  local-variable receivers — `r.close()`, `resp.close()`, `v.close()` —
  went unresolved, silently absent from the result rather than flagged
  as unresolved). That's a real recall gap in the receiver-type
  heuristic on this codebase, not present in the first benchmark's
  cleaner 2-file case. Filed as a known gap, not fixed here.
- **Task 3 (large file): a clear win, the opposite of the first
  benchmark's loss on this exact task shape.** `models.py` skeletonizes
  to 43% of its raw size (and every other file checked in this repo —
  `sessions.py`, `utils.py`, `adapters.py`, `auth.py`, `structures.py` —
  won too, margins from 57% smaller down to a 2% win on the smallest,
  least-documented file). The first benchmark's loss case (`agent.py`, a
  densely-docstringed file with many small methods) is a real
  counter-example that stands as published, not superseded by this
  result — the honest read across both datasets is "usually a real win,
  shrinking toward break-even as a file's own documentation density
  rises," matching the same shape CodeGraph's own disclosed range
  ("25–40% on small codebases, near break-even on response-heavy ones")
  already described.

## What this benchmark does not yet cover

Same limitations `docs/benchmarks.md` already names, still true here: no
token counts, no real LLM-in-the-loop measurement, one pass, and this is
now two repos, not the "7 real repos" a citable distribution needs. The
`close()` recall gap found in Task 2 is a real next investigation
(why did 3 of 6 receivers resolve and 3 didn't — likely the same
constructor-assignment receiver-type heuristic
`docs/competitor-benchmarks.md` already flagged a related bug in), not
attempted as a fix in this pass.

## Reproducing this

```bash
git clone --depth 1 https://github.com/psf/requests.git
cd requests
pip install "cie-mcp[mcp]"
cie index .
cie-mcp . --embedded --policy inspector
# then call search_symbol / callers / file_skeleton over MCP and compare
# against the equivalent grep/read commands yourself
```
