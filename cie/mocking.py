"""Section 16.2: Third-Party Mock Orchestration (spec section 16.2) —
TE-04/05/06/07.

TE-05 (SmartMockServer) is narrowed to STUB-MODE RESPONSE GENERATION
only. A real HTTP-intercepting proxy server (record/replay against live
network traffic, smart mode with LLM augmentation) is substantial
infrastructure — starting an actual proxy, configuring a target project's
outbound calls to route through it, persisting recorded fixtures — this
slice does not build any of that. What IS built: a real, deterministic
stub-response generator + the `MockCall` audit-log shape, the pieces a
caller could wire a real proxy around later.
"""

from __future__ import annotations

import hashlib
import re
from datetime import datetime, timezone
from typing import Optional, Sequence

from cie.models import Node, NodeKind

_CALL_PATTERNS = [
    re.compile(r"requests\.(get|post|put|patch|delete)\s*\(\s*[\"']([^\"']+)[\"']", re.IGNORECASE),
    re.compile(r"httpx\.(get|post|put|patch|delete)\s*\(\s*[\"']([^\"']+)[\"']", re.IGNORECASE),
    re.compile(r"axios\.(get|post|put|patch|delete)\s*\(\s*[\"']([^\"']+)[\"']", re.IGNORECASE),
    re.compile(r"fetch\(\s*[\"'](https?://[^\"']+)[\"']"),
]


def _stable_id(*parts: str) -> str:
    return hashlib.sha256("::".join(parts).encode()).hexdigest()[:16]


def _service_name_from_url(url: str) -> str:
    match = re.match(r"https?://([^/]+)", url)
    return match.group(1) if match else url


# -- TE-04: Mock Registry ------------------------------------------------------

def scan_external_calls(source_text: str, source_file: str) -> list[dict]:
    """TE-04: regex scan for outbound HTTP calls to an EXTERNAL host.
    Relative paths and `localhost`/`127.0.0.1` are skipped — those are
    almost always calls into the project's own backend, not a
    third-party dependency. Best-effort: misses calls built from string
    concatenation/variables, the same class of limitation `cie.
    api_routes`'s own frontend-call scanner already documents.
    """
    found: list[dict] = []
    for pattern in _CALL_PATTERNS:
        for match in pattern.finditer(source_text):
            groups = match.groups()
            url = groups[-1]
            if not url.startswith("http") or "localhost" in url or "127.0.0.1" in url:
                continue
            method = groups[0].upper() if len(groups) > 1 and groups[0] else "GET"
            found.append({
                "service_name": _service_name_from_url(url), "method": method, "url": url,
                "source_file": source_file, "line": source_text[:match.start()].count("\n") + 1,
            })
    return found


def build_mock_endpoints(calls: Sequence[dict]) -> tuple[list[dict], list[dict]]:
    """TE-04: one `MockEndpoint` node per distinct service found by
    `scan_external_calls`. No `CALLS_EXTERNAL` edges — a bare
    source-text scan doesn't have the calling code's own node id (a
    caller wanting that edge would join this against already-indexed
    FUNC nodes by `source_file`+`line`). Returns `(nodes, [])`.
    """
    endpoints: dict[str, dict] = {}
    for call in calls:
        endpoint_id = "mockendpoint::" + _stable_id(call["service_name"])
        row = endpoints.setdefault(endpoint_id, {
            "id": endpoint_id, "label": call["service_name"], "kind": NodeKind.MOCK_ENDPOINT.value,
            "source_file": "", "service_name": call["service_name"], "endpoints": [], "mock_mode": "stub",
        })
        summary = f"{call['method']} {call['url']}"
        if summary not in row["endpoints"]:
            row["endpoints"].append(summary)
    return list(endpoints.values()), []


# -- TE-05: Smart Mock Server (stub mode only) --------------------------------

def generate_stub_response(mock_endpoint: Node, method: str, path: str) -> dict:
    """TE-05, STUB MODE ONLY: a canned, generic response for a call
    against `mock_endpoint`. No real request/response schema is known
    (`scan_external_calls` captures method+URL only) — this returns a
    generic 200 with an empty JSON body, the same "safe default" a real
    stub server falls back to for an endpoint with no recorded schema.
    Record/replay/smart modes need actual captured traffic or an OpenAPI
    spec, neither of which this pass has — not attempted."""
    return {
        "status_code": 200, "headers": {"content-type": "application/json"}, "body": {},
        "mock_mode": "stub",
        "note": f"generic stub for {method} {path} against {mock_endpoint.label} — no recorded schema",
    }


def log_mock_call(mock_endpoint_id: str, method: str, path: str, status_code: int, now: str = "") -> dict:
    """TE-05: one `MockCall` audit-log entry for an intercepted call."""
    now = now or datetime.now(timezone.utc).isoformat()
    call_id = "mockcall::" + _stable_id(mock_endpoint_id, method, path, now)
    return {
        "id": call_id, "label": f"{method} {path}", "kind": NodeKind.MOCK_CALL.value,
        "source_file": "", "mock_endpoint_id": mock_endpoint_id, "method": method, "path": path,
        "status_code": status_code, "called_at": now,
    }


# -- TE-06: Mock Coverage Query ------------------------------------------------

def mock_coverage(external_services: Sequence[str], mock_endpoints: Sequence[Node]) -> dict:
    """TE-06: which of `external_services` (distinct service names found
    by `scan_external_calls`) have a `MockEndpoint`, and which don't."""
    distinct = set(external_services)
    mocked_names = {e.label for e in mock_endpoints}
    unmocked = sorted(distinct - mocked_names)
    return {
        "total_services": len(distinct),
        "mocked_count": len(distinct) - len(unmocked),
        "unmocked_services": unmocked,
    }


# -- TE-07: Contract Mock Validation ------------------------------------------

def validate_mock_call(mock_endpoint: Node, method: str, path: str) -> Optional[dict]:
    """TE-07: does this call's method+URL match anything declared on
    `mock_endpoint`? No body-schema comparison (none is captured by
    TE-04's scan) — returns a `ContractViolation` dict when no declared
    endpoint starts with `"{METHOD} "`, `None` when one does."""
    declared = mock_endpoint.properties.get("endpoints", [])
    prefix = f"{method.upper()} "
    if any(d.startswith(prefix) for d in declared):
        return None
    return {
        "id": "contractviolation::" + _stable_id(mock_endpoint.id, method, path),
        "label": f"unexpected call: {method} {path}", "kind": NodeKind.CONTRACT_VIOLATION.value,
        "source_file": "", "mock_endpoint_id": mock_endpoint.id, "method": method, "path": path,
    }
