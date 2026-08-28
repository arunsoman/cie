"""CF-10/11: Semantic Diff — spec vs code (spec section 14.5).

Explicitly narrowed per the spec's OWN caveat on CF-10 ("a significant
research effort — prototype before committing to a timeline"): this is
NOT graph isomorphism (VF2) over normalized semantic graphs of spec and
code. It's a cheap, real, pattern-matching heuristic: for each PRD clause
matching a KNOWN rule shape (retry counts, sync/async requirements,
validation requirements — `_RULE_PATTERNS`), check the target code
node's docstring + signature (the only "code semantics" available
without executing anything) for corroborating or contradicting evidence.

High false-negative rate by design (misses anything not matching a known
pattern) — kept deliberately conservative, since a wrong "contradiction"
finding surfaced to a human is worse than a missed one. This is a
prototype-level heuristic, not the research-grade engine the spec
describes; expanding `_RULE_PATTERNS` is the natural next iteration, not
a rewrite.
"""

from __future__ import annotations

import re

from cie.models import Node

_RETRY_SPEC_RE = re.compile(r"retry\s+(\d+)\s+times?", re.IGNORECASE)
_RETRY_CODE_RE = re.compile(r"retr\w*\D{0,10}(\d+)", re.IGNORECASE)
_ASYNC_REQUIRED_RE = re.compile(r"\b(must be|should be)\s+async\b", re.IGNORECASE)
_SYNC_REQUIRED_RE = re.compile(r"\b(must be|should be)\s+sync(hronous)?\b", re.IGNORECASE)
_VALIDATION_RE = re.compile(r"\bvalidat\w+\b[^.\n]*", re.IGNORECASE)


def detect_contradictions(spec_text: str, target_node: Node) -> list[dict]:
    """Scan `spec_text` for known rule patterns; check `target_node`'s
    docstring+signature for matching/contradicting evidence. Returns
    findings `{pattern, clause, finding_type, detail}` where
    `finding_type` is `"omission"` (spec states a rule, code shows no
    trace of it) or `"divergence"` (both mention it, but the specifics
    disagree — only implemented for the retry-count and sync/async
    patterns, where "disagree" is unambiguous to detect).
    """
    findings: list[dict] = []
    haystack = f"{target_node.docstring}\n{target_node.signature}".lower()

    retry_match = _RETRY_SPEC_RE.search(spec_text)
    if retry_match:
        expected = retry_match.group(1)
        code_match = _RETRY_CODE_RE.search(haystack)
        if code_match is None:
            findings.append({
                "pattern": "retry_count", "clause": retry_match.group(0),
                "finding_type": "omission",
                "detail": f"spec requires retrying {expected} times; no retry "
                          "logic found in docstring/signature",
            })
        elif code_match.group(1) != expected:
            findings.append({
                "pattern": "retry_count", "clause": retry_match.group(0),
                "finding_type": "divergence",
                "detail": f"spec says retry {expected} times; code mentions "
                          f"{code_match.group(1)}",
            })

    async_match = _ASYNC_REQUIRED_RE.search(spec_text)
    if async_match and "async" not in haystack:
        findings.append({
            "pattern": "async_required", "clause": async_match.group(0),
            "finding_type": "omission",
            "detail": "spec requires async; no 'async' marker in signature/docstring",
        })

    sync_match = _SYNC_REQUIRED_RE.search(spec_text)
    if sync_match and "async" in haystack:
        findings.append({
            "pattern": "sync_required", "clause": sync_match.group(0),
            "finding_type": "divergence",
            "detail": "spec requires synchronous behavior; signature mentions async",
        })

    validation_match = _VALIDATION_RE.search(spec_text)
    if validation_match and "valid" not in haystack:
        findings.append({
            "pattern": "validation_required", "clause": validation_match.group(0).strip(),
            "finding_type": "omission",
            "detail": f"spec requires validation ('{validation_match.group(0).strip()}'); "
                      "no validation mentioned in docstring/signature",
        })

    return findings
