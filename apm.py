"""Section 16.3: APM Integration & Telemetry Distribution (spec section
16.3) — TE-08/09/10/11.

TE-08's `build_apm_metric` ingests an ALREADY-MEASURED value — a caller
already has a timing/count number from wherever it came from. On top of
that, `collect_apm_from_pytest` (added as a follow-up, see
`cie-test-execution-apm-implementation.md`) does REAL automatic
collection: it runs pytest with its own built-in `--junitxml` reporter
(stdlib-parseable, no extra dependency) and turns each test's reported
wall-clock `time` into a `latency` metric directly — no caller has to
pre-compute anything. This is still narrower than the spec's own
OpenTelemetry-span framing (a full request -> handler -> DB query ->
response span tree): pytest's own per-test timing only, not
instrumenting a target project's internals, which would mean modifying
the actual generated project under test — out of scope for a generic
graph-layer tool. TE-09 is a real, lightweight, IN-PROCESS pub/sub — NOT
a new message broker (NATS/Kafka), the same "do not build a real broker
from scratch" restraint CF-13's ConsensusBus documents, applied here for
the same reason. It IS durable, though (2026-08-10 follow-up): pass a
`Repository` + `project` to `TelemetryBus.__init__` and every `publish()`
also appends the event to a real, persistent, per-`(project, channel)`
Neo4j log (`Repository.publish_telemetry_event`/`read_telemetry_events`,
see `cie/repository.py`) — the EXISTING database serves as the
append-only log, not a new piece of infra. A subscriber that starts
after an event was published (a fresh process, a restarted subsystem)
calls `TelemetryBus.replay(channel, after_seq=...)` to catch up from the
durable log through the exact same dispatch path `publish()` uses, so
replay isn't a second, divergent delivery mechanism.
"""

from __future__ import annotations

import hashlib
import statistics
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Optional, Sequence

from cie.models import Node, NodeKind


def _stable_id(*parts: str) -> str:
    return hashlib.sha256("::".join(parts).encode()).hexdigest()[:16]


# -- TE-08: APM Data Ingestion (already-collected data only) -----------------

#: The spec's own metric_type vocabulary.
VALID_METRIC_TYPES = frozenset({"latency", "throughput", "error_rate", "memory", "cpu"})


def build_apm_metric(
    code_node_id: str, metric_type: str, value: float, percentile: str = "", measured_at: str = "",
) -> dict:
    """TE-08: one `ApmMetric` node from an ALREADY-MEASURED value.
    `percentile` is `"p50"`/`"p95"`/`"p99"` for a `latency` metric, `""`
    for a non-percentile metric (throughput/error_rate/memory/cpu)."""
    measured_at = measured_at or datetime.now(timezone.utc).isoformat()
    metric_id = "apmmetric::" + _stable_id(code_node_id, metric_type, percentile, measured_at)
    return {
        "id": metric_id, "label": f"{metric_type}{'.' + percentile if percentile else ''} = {value}",
        "kind": NodeKind.APM_METRIC.value, "source_file": "", "code_node_id": code_node_id,
        "metric_type": metric_type, "value": value, "percentile": percentile, "measured_at": measured_at,
    }


def collect_apm_from_pytest(
    target_files: Sequence[str], project_root: Path, timeout: int = 300,
) -> list[dict]:
    """TE-08, REAL automatic collection (see module docstring for how
    this differs from `build_apm_metric`'s "caller already has a
    number" contract): runs pytest against `target_files` with
    `--junitxml`, then turns each `<testcase>` element's `time`
    attribute (seconds) into a `latency` `ApmMetric` in milliseconds,
    keyed by `{classname}::{name}`. Returns `[]` (never raises) when
    pytest crashes, times out, or produces no parseable XML — a failed
    collection attempt is a "nothing measured" result, not an error the
    caller must handle specially.
    """
    import os
    import tempfile
    import xml.etree.ElementTree as ET

    from cie.tools.runner import run_command

    if not target_files:
        return []

    fd, junit_path_str = tempfile.mkstemp(suffix=".xml")
    os.close(fd)
    junit_path = Path(junit_path_str)
    try:
        cmd = f'python -m pytest -q --junitxml="{junit_path}" ' + " ".join(
            f'"{f}"' for f in target_files
        )
        try:
            run_command(cmd, cwd=project_root, timeout=timeout, allowed_root=project_root)
            tree = ET.parse(junit_path)
        except Exception:  # noqa: BLE001 - pytest crashed/timed out/no XML; nothing to ingest
            return []
    finally:
        junit_path.unlink(missing_ok=True)

    metrics: list[dict] = []
    for testcase in tree.getroot().iter("testcase"):
        name = testcase.get("name", "")
        time_str = testcase.get("time")
        if not name or time_str is None:
            continue
        try:
            seconds = float(time_str)
        except ValueError:
            continue
        classname = testcase.get("classname", "")
        code_node_id = f"{classname}::{name}" if classname else name
        metrics.append(build_apm_metric(code_node_id, "latency", round(seconds * 1000, 3)))
    return metrics


# -- TE-09: Telemetry Distribution Bus (in-process pub/sub) ------------------

class TelemetryBus:
    """TE-09: real, in-process pub/sub — see module docstring for the
    NOT-a-new-broker framing and how `repo`/`project` make this durable.
    Scoped to fan `ApmMetric` data out to whichever in-process
    subscribers (confidence scorer, drift detector, invariant monitor,
    ...) a single test run's orchestrator registers.
    """

    def __init__(self, repo: Optional[Any] = None, project: str = "") -> None:
        self._subscribers: dict[str, list[Callable[[dict], None]]] = {}
        self._repo = repo
        self._project = project

    def subscribe(self, channel: str, callback: Callable[[dict], None]) -> None:
        self._subscribers.setdefault(channel, []).append(callback)

    def publish(self, channel: str, metric: dict) -> int:
        """Durably appends `metric` first (when this bus was built with
        a `repo`), THEN calls every subscriber on `channel` synchronously;
        returns how many were notified. Persisting before delivery means
        a subscriber that crashes mid-callback still leaves the event
        recoverable via `replay()` — delivery failure never loses the
        write. A subscriber that raises does NOT stop the others — one
        broken subscriber must not silently swallow telemetry meant for
        every other one."""
        if self._repo is not None:
            self._repo.publish_telemetry_event(channel, metric, project=self._project)
        notified = 0
        for callback in self._subscribers.get(channel, []):
            try:
                callback(metric)
                notified += 1
            except Exception:  # noqa: BLE001 - isolate one bad subscriber
                continue
        return notified

    def replay(self, channel: str, after_seq: int = 0, limit: int = 100) -> list[dict]:
        """Re-deliver durably-persisted events on `channel` with
        `seq > after_seq` to every CURRENT subscriber, through the same
        per-callback try/except isolation `publish()` uses — replay is
        not a second, divergent delivery path. Returns the list of
        `{seq, channel, payload, published_at}` dicts consumed (a
        caller tracks `max(e["seq"] for e in ...)` as its own resume
        cursor for the next call). `[]` when this bus has no `repo` —
        replay is only meaningful for a durable bus."""
        if self._repo is None:
            return []
        events = self._repo.read_telemetry_events(
            channel, after_seq=after_seq, limit=limit, project=self._project,
        )
        for event in events:
            for callback in self._subscribers.get(channel, []):
                try:
                    callback(event["payload"])
                except Exception:  # noqa: BLE001 - isolate one bad subscriber
                    continue
        return events


# -- TE-10: Performance Baseline ----------------------------------------------

def compute_baseline(code_node_id: str, metrics: Sequence[dict]) -> Optional[dict]:
    """TE-10: p50/p95/p99 latency + mean error rate from already-ingested
    `latency`/`error_rate` `ApmMetric` rows for ONE `code_node_id`. `None`
    when there are no latency samples to baseline."""
    latencies = sorted(
        m["value"] for m in metrics
        if m.get("code_node_id") == code_node_id and m.get("metric_type") == "latency"
    )
    if not latencies:
        return None
    error_values = [
        m["value"] for m in metrics
        if m.get("code_node_id") == code_node_id and m.get("metric_type") == "error_rate"
    ]
    return {
        "id": "baseline::" + _stable_id(code_node_id),
        "kind": NodeKind.PERFORMANCE_BASELINE.value, "code_node_id": code_node_id,
        "p50": statistics.median(latencies),
        "p95": latencies[min(len(latencies) - 1, int(len(latencies) * 0.95))],
        "p99": latencies[min(len(latencies) - 1, int(len(latencies) * 0.99))],
        "error_rate": statistics.mean(error_values) if error_values else 0.0,
        "sample_count": len(latencies),
    }


def detect_regression(baseline: dict, current_p95: float, threshold: float = 1.5) -> Optional[dict]:
    """TE-10: a `PerformanceRegression` when `current_p95` exceeds the
    baseline's p95 by `threshold`x (default 1.5, the spec's own example
    number). `None` when the baseline's p95 is non-positive (nothing
    meaningful to compare against) or the regression threshold isn't hit.
    """
    if baseline["p95"] <= 0 or current_p95 <= baseline["p95"] * threshold:
        return None
    return {
        "id": "regression::" + _stable_id(baseline["id"], str(current_p95)),
        "kind": NodeKind.PERFORMANCE_REGRESSION.value, "code_node_id": baseline.get("code_node_id", ""),
        "baseline_p95": baseline["p95"], "current_p95": current_p95,
        "regression_ratio": round(current_p95 / baseline["p95"], 2),
    }


# -- TE-11: APM Query API ------------------------------------------------------

def filter_metrics(metrics: Sequence[Node], code_node_id: str = "", metric_type: str = "") -> list[Node]:
    """TE-11: filter already-loaded `ApmMetric` nodes."""
    out = list(metrics)
    if code_node_id:
        out = [m for m in out if m.properties.get("code_node_id") == code_node_id]
    if metric_type:
        out = [m for m in out if m.properties.get("metric_type") == metric_type]
    return out
