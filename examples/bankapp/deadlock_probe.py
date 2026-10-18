"""Deadlock probe — a REAL two-lock deadlock, captured live.

Two threads take locks in opposite order:
  thread-one: lock_a → (pause) → lock_b
  thread-two: lock_b → (pause) → lock_a
Both block forever. faulthandler.dump_traceback_later dumps every thread's
stack to stderr after DEADLOCK_DUMP_SECONDS and exits the process — that
dump is the captured evidence. Before blocking, each thread records what it
holds and what it wants into a JSON file, so the collector can build the
waits-for graph and detect the cycle from real data.

The collector (scripts/collect_runtime.py) runs this as a child process.
"""

import faulthandler
import json
import sys
import threading
import time

DUMP_SECONDS = 1.5
INTERLEAVE_SECONDS = 0.4

state_path = sys.argv[1] if len(sys.argv) > 1 else "deadlock-state.json"
faulthandler.dump_traceback_later(DUMP_SECONDS, exit=True)

lock_a = threading.Lock()
lock_b = threading.Lock()
state: dict[str, dict] = {}
state_lock = threading.Lock()


def record(thread_label: str, holds: str, wants: str | None) -> None:
    with state_lock:
        state[thread_label] = {"holds": holds, "wants": wants}


def flush_state() -> None:
    with state_lock:
        with open(state_path, "w") as f:
            json.dump(state, f, indent=1)


def thread_one() -> None:
    with lock_a:
        record("thread-one", "lock_a", "lock_b")
        time.sleep(INTERLEAVE_SECONDS)  # the window where the interleave happens
        with lock_b:  # blocks forever — thread-two holds lock_b
            record("thread-one", "lock_a+lock_b", None)


def thread_two() -> None:
    with lock_b:
        record("thread-two", "lock_b", "lock_a")
        time.sleep(INTERLEAVE_SECONDS)
        with lock_a:  # blocks forever — thread-one holds lock_a
            record("thread-two", "lock_a+lock_b", None)


t1 = threading.Thread(target=thread_one, name="thread-one")
t2 = threading.Thread(target=thread_two, name="thread-two")
t1.start()
t2.start()

# keep writing the holds/wants table while the threads block
while True:
    flush_state()
    time.sleep(0.2)
