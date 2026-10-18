"""Runtime probes for the bankapp sample project.

Every probe is a small, honest program whose behaviour can be MEASURED.
Nothing in runtime-data/ is simulated: the collector (scripts/collect_runtime.py)
runs these functions under real tools — cProfile, tracemalloc, threading
timestamps, faulthandler stack dumps, read-modify-write trials and coverage.py —
and stores what it measured, with the tool and command named on every artifact.

Each probe exists so the sample project can showcase one kind of runtime
evidence:
  cpu_work            → CPU profile (what burned the time)
  quick_task/pipeline → concurrency timeline (who ran when)
  allocate_blocks     → memory profile (where the bytes went)
  racing_counter      → a real data race you can watch lose updates
  (deadlock_probe.py) → a real two-lock deadlock, captured + detected
"""

import sys
import threading
import time
import tracemalloc


def cpu_work(n: int = 200_000) -> float:
    """CPU-bound arithmetic — the profiler's favourite patient."""
    total = 0.0
    for i in range(n):
        total += (i * 2654435761) % 97 / 3.0
    return total


def cpu_burn_mixed(rounds: int = 60) -> float:
    """Calls cpu_work in rounds and mixes in small sleeps — so a CPU profile
    shows both burning time (cpu_work) and waiting (time.sleep), not just one."""
    acc = 0.0
    for _ in range(rounds):
        acc += cpu_work(50_000)
        time.sleep(0.005)
    return acc


def quick_task(x: int) -> int:
    """One small unit of work for the pipeline (sleeps a beat so the
    concurrency timeline shows overlapping threads)."""
    time.sleep(0.03)
    return x * 2


def pipeline(tasks: int = 12, workers: int = 4) -> list[dict]:
    """Run `tasks` units across `workers` threads and record every span.

    Returns a list of {thread, task, start_ms, end_ms} measured with
    time.monotonic() — the raw material of the concurrency timeline chart.
    """
    spans: list[dict] = []
    lock = threading.Lock()
    t0 = time.monotonic()

    def run(i: int) -> None:
        start = time.monotonic()
        quick_task(i)
        end = time.monotonic()
        with lock:
            spans.append(
                {
                    "thread": threading.current_thread().name,
                    "task": i,
                    "start_ms": round((start - t0) * 1000, 2),
                    "end_ms": round((end - t0) * 1000, 2),
                }
            )

    threads_left = tasks
    next_id = 0
    live: list[threading.Thread] = []
    while threads_left > 0 or live:
        while threads_left > 0 and len(live) < workers:
            t = threading.Thread(target=run, args=(next_id,), name=f"worker-{next_id % workers}")
            t.start()
            live.append(t)
            next_id += 1
            threads_left -= 1
        for t in list(live):
            if not t.is_alive():
                live.remove(t)
        time.sleep(0.002)
    spans.sort(key=lambda s: s["start_ms"])
    return spans


def allocate_blocks(blocks: int = 400, size: int = 4096) -> dict:
    """Allocate `blocks` chunks of `size` bytes while tracemalloc watches —
    the memory profile's raw material (top sites + totals)."""
    tracemalloc.start()
    keep: list[bytes] = []

    def churn() -> None:
        for i in range(blocks):
            keep.append(bytes(size))

    churn()
    current, peak = tracemalloc.get_traced_memory()
    snap = tracemalloc.take_snapshot()
    top = [
        {
            "where": f"{frame.filename.split('/')[-1]}:{frame.lineno}",
            "bytes": stat.size,
            "count": stat.count,
        }
        for stat in snap.statistics("lineno")[:8]
        for frame in stat.traceback[:1]
    ]
    tracemalloc.stop()
    return {
        "allocated_bytes": current,
        "peak_bytes": peak,
        "blocks": blocks,
        "block_size": size,
        "top_sites": top,
    }


def racing_counter(threads: int = 4, per_thread: int = 100_000, widen: bool = False) -> dict:
    """A REAL data race: threads do read-modify-write on one integer with no
    lock. CPython's GIL makes each single bytecode atomic, but the three-step
    read-add-write is not — updates get lost whenever a thread switch lands
    between the read and the write.

    widen=False: the natural window. On CPython 3.12 the switch check lands
    on the loop backedge, so short runs may lose nothing — that is an honest
    result (races are timing-dependent), and the artifact says so.
    widen=True: the probe YIELDS the GIL between the read and the write,
    which is exactly the window real code leaves open. This reliably loses
    updates. The widening is disclosed in the artifact — what it proves is
    that the sequence is not atomic, which is the race.
    """
    counter = {"value": 0}
    barrier = threading.Barrier(threads)
    old_interval = sys.getswitchinterval()
    sys.setswitchinterval(0.001)  # more preemption → wider race window

    def worker() -> None:
        barrier.wait()
        for _ in range(per_thread):
            v = counter["value"]
            if widen:
                time.sleep(0)  # yield the GIL mid-sequence — the disclosed window
            v = v + 1  # deliberately not atomic
            counter["value"] = v

    ts = [threading.Thread(target=worker) for _ in range(threads)]
    t0 = time.monotonic()
    for t in ts:
        t.start()
    for t in ts:
        t.join()
    wall_ms = round((time.monotonic() - t0) * 1000, 2)
    sys.setswitchinterval(old_interval)
    expected = threads * per_thread
    final = counter["value"]
    return {
        "threads": threads,
        "per_thread": per_thread,
        "expected": expected,
        "final": final,
        "lost": expected - final,
        "wall_ms": wall_ms,
        "switch_interval_s": 0.001,
        "start_barrier": True,
        "widened": widen,
    }
