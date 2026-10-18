#!/usr/bin/env python3
"""stories_probes.py — real probes the story collector runs, for real.

Every function here does actual work on this machine and returns actual
numbers. Nothing is simulated: slow requests really sleep/really hash,
duplicate deliveries really double-charge a real in-memory ledger, the
sandbox A/B really runs both variants, and the replay really traces the
bankapp's own auth code line by line.

Where a probe mirrors a flaw that exists in bankapp's source (e.g. the
unguarded double-write in payments.py), the mirror is disclosed in the
honesty note of the artifact that reports it.
"""

from __future__ import annotations

import hashlib
import json
import threading
import time


# ------------------------------------------------------------ replay ----

def replay_authorize():
    """Trace bankapp's own good_auth("alice") line by line.

    Returns the raw event list (function, line, event, locals-at-return).
    This is a REAL execution of the real auth.py code in this project.
    """
    import sys

    import auth  # bankapp's own module — the real code, not a copy

    events: list[dict] = []

    def tracer(frame, event, arg):
        if frame.f_code.co_filename.endswith("auth.py"):
            if event == "call":
                return tracer
            if event in ("line", "return"):
                row = {
                    "event": event,
                    "function": frame.f_code.co_name,
                    "line": frame.f_lineno,
                }
                if event == "return":
                    try:
                        row["returned"] = json.dumps(arg) if not isinstance(arg, str) else arg
                    except Exception:
                        row["returned"] = "<unserializable>"
                    safe = {}
                    for k, v in frame.f_locals.items():
                        try:
                            json.dumps(v)
                            safe[k] = v
                        except Exception:
                            safe[k] = "<unserializable>"
                    row["locals"] = safe
                events.append(row)
            return tracer
        return None

    old = sys.gettrace()
    sys.settrace(tracer)
    try:
        result = auth.good_auth("alice")
    finally:
        sys.settrace(old)
    return {"result": result, "events": events}


# -------------------------------------------------------- slow request ----

def _external_call():
    """Stands in for a payment-processor API call: pure waiting, no CPU."""
    time.sleep(0.12)


def _db_query():
    """Stands in for a database round-trip: pure waiting, no CPU."""
    time.sleep(0.05)


def _password_check(user: str):
    """Real KDF work: pbkdf2 with 120k iterations — genuine CPU burn."""
    hashlib.pbkdf2_hmac("sha256", user.encode(), b"bankapp-stories-salt", 120_000)


def slow_request(user: str = "alice"):
    """One slow request, honestly composed of the three cost kinds:
    real CPU work (pbkdf2), waiting on an outside service (sleep), and
    waiting on the data store (sleep). cProfile separates them because
    sleep shows up as time spent inside time.sleep, not inside our code.
    """
    _password_check(user)
    _external_call()
    _db_query()
    return "ok"


# ------------------------------------------------- duplicate delivery ----

class Ledger:
    """A real in-memory ledger, deliberately written the way payments.py
    writes: every delivery appends, nothing checks whether the order was
    already charged. That flaw is the point of the probe."""

    def __init__(self):
        self.charges: list[dict] = []

    def charge(self, order_id: int, amount: float):
        # mirrors payments.py charge(): write first, ask never
        self.charges.append({"order_id": order_id, "amount": amount, "at": time.time()})
        return {"status": 200, "charged": amount}


def duplicate_delivery(order_id: int = 42, amount: float = 75.0, deliveries: int = 2):
    """Deliver the same order twice (what a retrying queue honestly does),
    with no idempotency key — then count what the ledger actually holds."""
    ledger = Ledger()
    for _ in range(deliveries):
        ledger.charge(order_id, amount)
    expected = 1
    actual_orders = len(ledger.charges)
    charged_total = sum(c["amount"] for c in ledger.charges)
    return {
        "deliveries": deliveries,
        "expected_charges": expected,
        "actual_charges": actual_orders,
        "ledger": ledger.charges,
        "customer_charged_total": charged_total,
        "customer_owed_total": amount,
        "overcharge": charged_total - amount,
    }


# ---------------------------------------------------- speed compare ----

def _locked_pipeline(tasks: int, workers: int):
    """A deliberately serialized pipeline: one global lock around each task.
    Used as the 'after the change' run in the slowdown story (US-084)."""
    lock = threading.Lock()

    def task(n: int) -> int:
        with lock:  # the change that slows everything down
            time.sleep(0.03)
            return n * 2

    from concurrent.futures import ThreadPoolExecutor

    with ThreadPoolExecutor(max_workers=workers) as ex:
        t0 = time.monotonic()
        list(ex.map(task, range(tasks)))
        return (time.monotonic() - t0) * 1000.0


def _free_pipeline(tasks: int, workers: int):
    """The unmodified pipeline shape (same work, no global lock)."""
    def task(n: int) -> int:
        time.sleep(0.03)
        return n * 2

    from concurrent.futures import ThreadPoolExecutor

    with ThreadPoolExecutor(max_workers=workers) as ex:
        t0 = time.monotonic()
        list(ex.map(task, range(tasks)))
        return (time.monotonic() - t0) * 1000.0


def bench(pipeline, tasks: int, workers: int, runs: int = 3) -> dict:
    """Run the same workload several times; keep EVERY run in the count,
    including failures. Median is computed over all completed runs."""
    walls: list[float] = []
    failures = 0
    errors: list[str] = []
    for _ in range(runs):
        try:
            walls.append(round(pipeline(tasks, workers), 2))
        except Exception as e:  # a failed run stays in the record
            failures += 1
            errors.append(f"{type(e).__name__}: {e}")
    ordered = sorted(walls)
    med = ordered[len(ordered) // 2] if ordered else None
    return {
        "runs": runs,
        "walls_ms": walls,
        "failures": failures,
        "errors": errors,
        "median_ms": med,
    }


# ------------------------------------------------ sandbox A/B compare ----

def price_variant_a(items: list[float]) -> float:
    """Round once at the end (what most ledgers do)."""
    if not items:
        raise ValueError("empty basket rejected by variant A policy")
    total = sum(items)
    return round(total, 2)


def price_variant_b(items: list[float]) -> float:
    """Round after every line (what some carts do)."""
    total = 0.0
    for it in items:
        total += round(it, 2)
    return round(total, 2)


def sandbox_compare(basket: list[float], runs: int = 3) -> dict:
    """Run both pricing variants on the same basket, in-process but
    logically isolated (same inputs, independent calls). Failures stay
    in the count — an empty basket is part of the contract, not noise."""
    out: dict = {"basket": basket, "a": {}, "b": {}}
    totals_a, totals_b, fail_a, fail_b, err_a, err_b = [], [], 0, 0, [], []
    for _ in range(runs):
        try:
            totals_a.append(price_variant_a(basket))
        except Exception as e:
            fail_a += 1
            err_a.append(f"{type(e).__name__}: {e}")
        try:
            totals_b.append(price_variant_b(basket))
        except Exception as e:
            fail_b += 1
            err_b.append(f"{type(e).__name__}: {e}")
    out["a"] = {"totals": totals_a, "failures": fail_a, "errors": err_a,
                "final": totals_a[0] if totals_a else None}
    out["b"] = {"totals": totals_b, "failures": fail_b, "errors": err_b,
                "final": totals_b[0] if totals_b else None}
    if out["a"]["final"] is not None and out["b"]["final"] is not None:
        out["difference"] = round(out["b"]["final"] - out["a"]["final"], 2)
    # the edge case both must survive: an empty basket
    try:
        price_variant_a([])
        out["empty_basket_a"] = "accepted"
    except Exception as e:
        out["empty_basket_a"] = f"refused: {type(e).__name__}"
    try:
        price_variant_b([])
        out["empty_basket_b"] = "accepted"
    except Exception as e:
        out["empty_basket_b"] = f"refused: {type(e).__name__}"
    return out


# ------------------------------------------- candidate: spend limit ----
# The walkthrough's candidate feature. These functions really run — the
# journeys (US-115) and the fix-and-recheck loop (US-121) execute them.

def can_transfer(amount, limit: float) -> bool:
    """must refuse transfers above the spending limit"""
    return amount <= limit


def transfer(amount, user: str, limit: float) -> dict:
    """must refuse transfers above the spending limit; must reject bad input"""
    if not isinstance(amount, (int, float)) or isinstance(amount, bool):
        raise ValueError("amount must be a number")
    if amount <= 0:
        raise ValueError("amount must be positive")
    if not can_transfer(amount, limit):
        return {"status": 402, "reason": f"over limit: {amount} > {limit}"}
    return {"status": 200, "charged": amount, "user": user}


def journey(kind: str, limit: float = 1000.0) -> dict:
    """Walk one real journey through the candidate: 'ok', 'denied' or 'error'."""
    try:
        if kind == "ok":
            return {"journey": "successful", "input": {"amount": 200, "user": "alice"},
                    "output": transfer(200, "alice", limit)}
        if kind == "denied":
            return {"journey": "denied", "input": {"amount": 1200, "user": "bob"},
                    "output": transfer(1200, "bob", limit)}
        if kind == "failure":
            return {"journey": "failure", "input": {"amount": "abc", "user": "carol"},
                    "output": {"raised": transfer("abc", "carol", limit)}}
    except ValueError as e:
        return {"journey": "failure", "input": {"amount": "abc", "user": "carol"},
                "output": {"raised": f"ValueError: {e}"}}
    raise ValueError(f"unknown journey {kind}")
