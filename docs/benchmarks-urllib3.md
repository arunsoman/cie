# Benchmarks — a third dataset (urllib3), from the reproducible harness

**Run:** 2026-08-30, via [`scripts/benchmark_tasks.py`] — the third
independent public repo, and the first whose numbers regenerate
end-to-end from the script (roadmap R9's bar: a fresh-clone reader can
reproduce every number below without asking anyone).

## Methodology

**Target:** [`urllib3`](https://github.com/urllib3/urllib3) at commit
`85a8a9cf` — `src/urllib3/`, 36 files, ~21k lines of real,
in-production Python (the HTTP library most of the Python ecosystem
depends on under the hood). Indexed via `cie index src`: 667 nodes,
1,307 edges (598 resolved `calls` edges).

**Comparison:** same two-column shape as the first two benchmark docs —
naive (`grep`/file read, commands actually run against the clone) vs cie
(the equivalent ToolService call against the actual indexed graph via
`benchmark_tasks.py --db`). The three task shapes mirror the earlier
datasets (easy definition, ambiguous callers, large file). **Reproduce:**

```bash
git clone https://github.com/urllib3/urllib3 && cd urllib3
git checkout 85a8a9cf
python -m cie.cli index src
python scripts/benchmark_tasks.py . --db src/.cie/graph.db \
  --src-glob '/path/to/clone/src/urllib3/*.py' \
  --class-name PoolManager --ambiguous-name close \
  --big-file src/urllib3/connectionpool.py --out bench.json
```

**Token-per-query metric:** chars of response payload, naive vs cie
(the honest, tokenizer-free proxy — no tokenizer is pinned, so this is
a BYTES comparison, labeled as such, not a vendor-style "120× fewer
tokens" claim).

## Results

| Task | Naive | cie | Naive | cie |
|---|---|---|---|---|
| Find `class PoolManager` | `grep -rn "class PoolManager"` → **2 files** (`poolmanager.py` + an `__init__` re-export line — grep counts both, no kind info) | `search_symbol("PoolManager", kind="class")` → **1 definition**, line range, signature | 1 call, 172 B | 1 call, 664 B |
| Every real caller of `close()` | `grep -rn "close(" src/urllib3/*.py` → **28 raw matches**, no receiver/definition info | `callers("close")` → **12 receiver/matching-resolved edges** with provenance; `resolution` shows the rest | 28 matches | 1 call, resolution `{total: 40, unresolved: 28, resolved: 12}` |
| Understand `connectionpool.py` (44,977 B) | Read the whole file | `file_skeleton(...)` | 44,977 B | 20,055 B (**2.24×** compression) |

## What this actually shows

- **Task 1: a modest win.** `PoolManager` is both defined AND
  re-referenced in `__init__.py`; grep returns both files with no way
  to tell which is the definition; `search_symbol` with a kind filter
  returns exactly the definition with its line range. One call each —
  the bytes number favors grep (172 B vs 664 B); the DISAMBIGUATION, not
  the size, is the win. Honest framing: tie on calls, win on
  disambiguation, loss on bytes.
- **Task 2: a real, nuanced win — with a published recall miss.** The
  graph attributes 12 of the 40 real call sites of `close()` to their
  actual callers (receiver/same-file/import resolution, per-edge
  confidence + provenance) where grep's 28 matches mix definitions,
  unrelated `close()` calls on other objects, and comments. The
  unresolved 28 are IN the response (`resolution`), never hidden. The
  receiver-heuristic recall gap documented on psf/requests is bigger
  here (28/40 unresolved) — this repo makes heavier use of
  same-name-different-receiver patterns; publishing both is the
  honesty stance, not a caveat to bury.
- **Task 3: the same pattern as the first two repos** — a 2.24× smaller,
  structure-only skeleton of a 45KB file — but weaker than requests'
  2.32× (urllib3's connectionpool.py carries more docstrings, which the
  skeleton keeps; reported as found).

## Honest-loss ledger (updated)

- ambiguous-caller recall remains the known gap; on urllib3 it's 28/40
  unresolved (worse than requests' doc-table 3-of-6 framing — see that
  doc's reconciliation note about name-keyed vs adapter-keyed
  denominators).
- file_skeleton compression is real but varies by file (~2.2–2.3× on
  both public repos; 43% of size on requests, 45% here).
- bytes-per-response on exact-match searches favors grep; the tools'
  value is disambiguation and structured follow-ups, not payload size.

**Treat the three benchmark docs together as the current evidence** —
docs/benchmarks.md (private first repo), docs/benchmarks-requests.md
(psf/requests), this file (urllib3, reproducible from the script).