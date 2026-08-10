"""CI-15/16/17: Runtime Telemetry Ingestion (spec section 13.4).

Real OpenTelemetry span ingestion from a LIVE DEPLOYMENT's own OTel
SDK — distinct from TE-08 (test-time APM, already done via pytest's own
`--junitxml` reporter, see `cie/apm.py`). The spec draws this split
itself: "TE-08 covers test-time APM... [CI-15's] scope: production
runtime telemetry (OpenTelemetry from live deployments)."

Wire format: OTLP/HTTP with **JSON** encoding — the OpenTelemetry
Protocol's own documented `application/json` content-type for the trace
export request (https://opentelemetry.io/docs/specs/otlp/#otlphttp).
Every major OTel SDK's `OTLPSpanExporter` can be configured to send this
encoding, and the OTel Collector's `otlphttp` exporter supports it too.
Raw protobuf decoding is deliberately NOT attempted — it would need the
generated `opentelemetry-proto` message classes as a new dependency, a
distinct, security/dependency-review-worthy undertaking. Same scoping
call this session already made for TE-05 (real mock server, but no
TLS-interception proxy) — see `cie/mock_server.py`'s docstring.

Symbol resolution: OTel's Semantic Conventions for Code
(https://opentelemetry.io/docs/specs/semconv/attributes-registry/code/)
define `code.function.name`/`code.function` (legacy) and
`code.file.path`/`code.filepath` (legacy) span attributes — exactly the
`(source_file, label)` pair every FUNC/METHOD node already carries. A
span whose attributes carry neither simply can't be resolved to a graph
symbol and is skipped, not guessed at (no fuzzy name-only matching —
too many real projects have multiple same-named functions across
files).
"""

from __future__ import annotations

from collections import defaultdict
from typing import Any, Optional, Sequence

from cie.models import Edge, Node, NodeKind

#: OTLP `Status.StatusCode.STATUS_CODE_ERROR` — the numeric value the
#: OTLP JSON encoding uses for a failed span's `status.code`.
STATUS_CODE_ERROR = 2


def _attr_value(value: dict) -> Any:
    """OTLP JSON `AnyValue`: exactly one of these keys is present."""
    for key in ("stringValue", "intValue", "doubleValue", "boolValue"):
        if key in value:
            return value[key]
    return None


def _flatten_attributes(attrs: Optional[Sequence[dict]]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for kv in attrs or []:
        key = kv.get("key")
        if key:
            out[key] = _attr_value(kv.get("value") or {})
    return out


def parse_otlp_spans(payload: dict) -> list[dict]:
    """Flatten an OTLP/HTTP JSON `ExportTraceServiceRequest` body into
    one flat dict per span. `scopeSpans` is the current field name;
    `instrumentationLibrarySpans` is the pre-1.0 OTLP name some older
    exporters still send — both are accepted."""
    out: list[dict] = []
    for rs in payload.get("resourceSpans") or []:
        resource_attrs = _flatten_attributes((rs.get("resource") or {}).get("attributes"))
        service_name = resource_attrs.get("service.name", "")
        scope_spans = rs.get("scopeSpans") or rs.get("instrumentationLibrarySpans") or []
        for ss in scope_spans:
            for span in ss.get("spans") or []:
                status = span.get("status") or {}
                out.append({
                    "trace_id": span.get("traceId", ""),
                    "span_id": span.get("spanId", ""),
                    "parent_span_id": span.get("parentSpanId", ""),
                    "name": span.get("name", ""),
                    "start_ns": int(span.get("startTimeUnixNano") or 0),
                    "end_ns": int(span.get("endTimeUnixNano") or 0),
                    "is_error": status.get("code") == STATUS_CODE_ERROR,
                    "attributes": _flatten_attributes(span.get("attributes")),
                    "service_name": service_name,
                })
    return out


def build_symbol_index(nodes: Sequence[Node]) -> dict[str, list[tuple[str, str]]]:
    """`label -> [(source_file, node_id), ...]` for every FUNC/METHOD
    node — bucketed by name first so span resolution doesn't scan the
    whole graph per span, then disambiguated by file path."""
    index: dict[str, list[tuple[str, str]]] = defaultdict(list)
    for n in nodes:
        if n.kind in (NodeKind.FUNC.value, NodeKind.METHOD.value) and n.source_file:
            index[n.label].append((n.source_file, n.id))
    return index


def _resolve_span_node(span: dict, index: dict[str, list[tuple[str, str]]]) -> Optional[str]:
    attrs = span["attributes"]
    filepath = attrs.get("code.filepath") or attrs.get("code.file.path")
    function = attrs.get("code.function") or attrs.get("code.function.name")
    if not filepath or not function:
        return None
    # Span filepaths are often absolute (the instrumented process's own
    # filesystem view); graph source_file is repo-relative — match by
    # suffix so either form resolves without requiring an exact string.
    for source_file, node_id in index.get(function, []):
        if source_file == filepath or filepath.endswith(source_file):
            return node_id
    return None


def spans_to_actual_calls(
    spans: Sequence[dict], index: dict[str, list[tuple[str, str]]],
) -> tuple[list[dict], int]:
    """CI-15: group spans by trace, resolve each parent/child span pair
    to graph node ids via `code.filepath`/`code.function`, and emit one
    `{"source", "target", "latency_ms", "is_error"}` row per resolvable
    pair — the input `accumulate_actual_calls` expects. Returns
    `(pairs, unresolved_count)`: `unresolved_count` is candidate parent-
    child span pairs (both endpoints exist in the same trace) where
    either endpoint couldn't be matched to a known symbol — reported
    back to the ingestion caller as an honest signal instead of being
    silently dropped."""
    by_trace: dict[str, dict[str, dict]] = defaultdict(dict)
    for span in spans:
        by_trace[span["trace_id"]][span["span_id"]] = span

    pairs: list[dict] = []
    candidate_count = 0
    for trace_spans in by_trace.values():
        cache: dict[str, Optional[str]] = {}

        def resolve(span_id: str) -> Optional[str]:
            if span_id not in cache:
                span = trace_spans.get(span_id)
                cache[span_id] = _resolve_span_node(span, index) if span else None
            return cache[span_id]

        for span in trace_spans.values():
            parent_id = span.get("parent_span_id")
            if not parent_id or parent_id not in trace_spans:
                continue
            candidate_count += 1
            callee_id = resolve(span["span_id"])
            caller_id = resolve(parent_id)
            if callee_id is None or caller_id is None:
                continue
            latency_ms = max(0.0, (span["end_ns"] - span["start_ns"]) / 1e6)
            pairs.append({
                "source": caller_id, "target": callee_id,
                "latency_ms": latency_ms, "is_error": span["is_error"],
            })
    return pairs, candidate_count - len(pairs)


def dead_code_confirm(nodes: Sequence[Node], edges: Sequence[Edge]) -> list[dict]:
    """CI-17: cross-reference the static call graph with runtime
    telemetry. Pure/offline — takes an already-fetched `(nodes, edges)`
    pair (e.g. from `Repository.project_graph()`) rather than querying
    the repository itself, so it's testable against a plain list of
    dataclasses without a fake repository.

    Classification (priority order):
    - `alive` — at least one `ACTUAL_CALL` edge targets this function
      (proof of runtime execution, regardless of static callers — this
      is what catches dynamic dispatch/reflection the static graph
      misses).
    - `unknown` — no `ACTUAL_CALL` edge exists ANYWHERE in this project
      (telemetry was never ingested), so a zero-runtime-calls reading
      for any one function can't be trusted as a signal either way.
    - `potentially_dead` — telemetry exists for this project, this
      function has static `calls` callers, but none of them were ever
      observed executing at runtime (the static call sites may
      themselves be dead code, or simply not yet exercised).
    - `confirmed_dead` — telemetry exists for this project, and this
      function has neither static nor runtime callers.
    """
    static_targets = {e.target for e in edges if e.relation == "calls"}
    actual_targets = {e.target for e in edges if e.relation == "ACTUAL_CALL"}
    has_any_telemetry = any(e.relation == "ACTUAL_CALL" for e in edges)

    out: list[dict] = []
    for n in nodes:
        if n.kind not in (NodeKind.FUNC.value, NodeKind.METHOD.value):
            continue
        if n.id in actual_targets:
            classification = "alive"
        elif not has_any_telemetry:
            classification = "unknown"
        elif n.id in static_targets:
            classification = "potentially_dead"
        else:
            classification = "confirmed_dead"
        out.append({
            "id": n.id, "label": n.label, "source_file": n.source_file,
            "classification": classification,
        })
    return out
