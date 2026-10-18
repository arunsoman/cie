# Runtime evidence — what this sample project measures, and how

This folder is CIE's **playground**: a real, runnable sample project whose
runtime behaviour is actually measured and stored, so every chart the
dashboard shows can be traced back to a real capture on this machine —
and so a real project knows exactly what to collect and how.

**The honesty rule:** nothing in `runtime-data/` is simulated, faked or
extrapolated. Every artifact names the tool that produced it, the command
shape that ran it, the timestamp, and the real-world tools that collect the
same evidence on production projects. Where a measurement came up empty or
failed, the artifact says so instead of inventing a number.

## Collect everything

```bash
python3 scripts/collect_runtime.py     # → runtime-data/*.json + runtime.json
```

## The six datasets

| Dataset | What is measured here | Tool used in this demo | On a real project |
|---|---|---|---|
| **CPU** | which functions burned the time | `cProfile` (stdlib) | py-spy, Java Flight Recorder, perf + flamegraphs |
| **Memory** | where the bytes went, peak tracked live | `tracemalloc` (stdlib) | heaptrack / memray, jemalloc stats, JFR memory events |
| **Concurrency** | thread-by-thread span timeline | `threading` + `time.monotonic` | JFR thread events, async-profiler, eBPF run-queue traces |
| **Deadlock** | a real two-lock deadlock, stacks captured, cycle computed | `faulthandler.dump_traceback_later` + waits-for cycle detection | jstack + thread-dump analysers, py-spy dump |
| **Race** | lost updates counted under concurrent read-add-write | observation trials (natural + disclosed widened window) | ThreadSanitizer, Go `-race`, Helgrind |
| **Test coverage** | which lines the project's own tests really touch | `coverage.py` + pytest | JaCoCo, cargo-tarpaulin/llvm-cov, `go test -cover`, nyc/vitest |

## How each capture works (plain words)

- **CPU** — `cProfile` counts time inside every function while
  `cpu_burn_mixed()` runs. The profile names the function, its self time and
  cumulative time, measured on the collecting machine.
- **Memory** — `tracemalloc` traces every allocation `allocate_blocks()`
  makes and keeps the top allocation sites plus the live and peak totals.
- **Concurrency** — every task in `pipeline()` records its start and end
  timestamps per thread. The overlap number (sum of task time ÷ wall time)
  is measured, not asserted.
- **Deadlock** — `deadlock_probe.py` really deadlocks: two threads take
  `lock_a`/`lock_b` in opposite orders and block forever. The probe records
  what each thread holds and wants; `faulthandler` dumps every thread's
  stack to prove they were blocked at the acquire lines; the collector
  builds the waits-for graph and computes the cycle from that captured
  data. Nothing is hand-declared.
- **Race** — four threads do read-add-write on one shared integer with no
  lock. Natural-window trials show what this exact run lost (often zero —
  that is honest; races are timing-dependent). Widened trials yield the GIL
  between read and write — the same window real code leaves open, fully
  disclosed — and reliably lose updates, proving the sequence is not atomic.
- **Coverage** — the project's own tests run under `coverage.py`. Files no
  test imports are listed as 0 **with that reason**, never as a fake number.

## Limits (stated up front)

- Everything is measured on one machine in one window; a rerun gives
  different numbers. That is what honest measurement looks like.
- `tracemalloc` only sees allocations made while it is tracing, and slows
  the program a little. `cProfile` adds bookkeeping overhead.
- One captured deadlock proves this run deadlocked; proving recurring
  deadlocks needs production thread-dump monitoring.
- Coverage here is line coverage of the sample's own tests — not a claim
  that any behaviour is correct.

## Why this matters for real projects

The demo store (`public/data/runtime.json` in the CIE dashboard) is built
by exactly this collector. When you connect a real project, CIE can accept
the same shapes of evidence — a profile export, a thread dump, a coverage
report — and will keep the same honesty contract: tool named, command
named, gaps named, nothing invented.
