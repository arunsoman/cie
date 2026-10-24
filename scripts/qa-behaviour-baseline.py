#!/usr/bin/env python3
"""qa-behaviour-baseline.py — Stage 0 read-only oracle replay for the SMPP
behavioural baseline (tests/qa-contracts/smpp-behaviour-baseline.json).

Reads a captured legacy bundle (a revision snapshot artifact) and evaluates
six source-oracle expectations against it. The oracle lives in the fixture;
this harness is generic — no application-specific allowlists.

Statuses are distinct and absence is never a pass:
  pass     the bundle carries the oracle-required records and satisfies the claim
  fail     the evidence is present and demonstrably violates the claim
  invalid  the bundle lacks the evidence needed to evaluate the claim

Read-only guarantee: the harness opens and parses files; it never writes,
never re-ingests, never relabels.

Usage:
  python3 scripts/qa-behaviour-baseline.py <bundle-path> [--fixture F] [--json]
  python3 scripts/qa-behaviour-baseline.py --self-test

Exit codes: 0 results == fixture expected_result; 1 results differ;
2 harness/usage error.
"""

import argparse
import json
import re
import sys
from pathlib import Path

FIXTURE_DEFAULT = Path(__file__).resolve().parent.parent / "tests" / "qa-contracts" / "smpp-behaviour-baseline.json"

PASS, FAIL, INVALID = "pass", "fail", "invalid"


class HarnessError(Exception):
    """Fatal harness/usage problem (exit 2)."""


# --------------------------------------------------------------------------
# Fixture
# --------------------------------------------------------------------------

def load_fixture(path: Path) -> dict:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise HarnessError(f"fixture not found: {path}")
    except json.JSONDecodeError as e:
        raise HarnessError(f"fixture is not valid JSON: {path}: {e}")
    if data.get("schema") != "cie.qa.behaviour-baseline/1":
        raise HarnessError(f"fixture schema mismatch: {data.get('schema')!r} in {path}")
    if not isinstance(data.get("expectations"), list) or not data["expectations"]:
        raise HarnessError(f"fixture has no expectations: {path}")
    for exp in data["expectations"]:
        for key in ("id", "check", "expected"):
            if key not in exp:
                raise HarnessError(f"expectation missing {key!r}: {exp.get('id', '?')}")
        if exp["expected"] not in (PASS, FAIL, INVALID):
            raise HarnessError(f"expectation {exp['id']} has invalid expected={exp['expected']!r}")
    return data


# --------------------------------------------------------------------------
# Bundle loading (read-only)
# --------------------------------------------------------------------------

def _is_snapshot(obj) -> bool:
    return (
        isinstance(obj, dict)
        and "summaries" in obj
        and "symbols" in obj
        and ("rev" in obj or "files" in obj)
    )


def _read_json(path: Path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, UnicodeDecodeError) as e:
        raise HarnessError(f"unreadable JSON in bundle: {path}: {e}")


def load_bundle(bundle_path: Path):
    """Resolve the bundle to exactly one snapshot dict (+ provenance string).

    Accepted shapes (documented in the fixture's bundle_contract):
      * file: snapshot-shaped object
      * file: {"snapshots": [ ... ]} with exactly one snapshot
      * dir:  snapshot-shaped *.json file (stack/concepts merged from
              sibling artifacts when the snapshot lacks them)
    """
    if not bundle_path.exists():
        raise HarnessError(f"bundle path does not exist: {bundle_path}")

    if bundle_path.is_file():
        data = _read_json(bundle_path)
        if isinstance(data, dict) and "snapshots" in data:
            snaps = data["snapshots"]
            if not isinstance(snaps, list) or len(snaps) != 1:
                raise HarnessError(
                    f"bundle wraps {len(snaps) if isinstance(snaps, list) else '?'} snapshots; "
                    "exactly one is required (ambiguous captures are never guessed)"
                )
            return snaps[0], str(bundle_path)
        if _is_snapshot(data):
            return data, str(bundle_path)
        # multi-artifact web bundle: exactly one snapshot-shaped value among
        # the top-level artifact keys (e.g. under 'report')
        if isinstance(data, dict):
            snap_keys = [k for k, v in data.items() if _is_snapshot(v)]
            if len(snap_keys) == 1:
                snapshot = dict(data[snap_keys[0]])
                merged = []
                for key in ("stack", "concepts"):
                    if key not in snapshot and isinstance(data.get(key), dict):
                        snapshot[key] = data[key]
                        merged.append(key)
                provenance = f"{bundle_path} (snapshot under '{snap_keys[0]}'"
                return snapshot, provenance + (f"; {', '.join(merged)} merged from siblings" if merged else ")")
            if len(snap_keys) > 1:
                raise HarnessError(
                    f"bundle contains {len(snap_keys)} snapshot-shaped artifacts "
                    f"({', '.join(snap_keys)}); exactly one is required"
                )
        raise HarnessError(f"bundle file is not snapshot-shaped: {bundle_path}")

    # directory of artifacts
    snapshot = None
    extras: dict = {}
    snap_files, artifact_files = [], []
    for p in sorted(bundle_path.rglob("*.json")):
        obj = _read_json(p)
        if _is_snapshot(obj):
            snap_files.append((p, obj))
        elif isinstance(obj, dict):
            artifact_files.append((p, obj))
    if len(snap_files) == 1:
        snapshot, snap_src = snap_files[0]
    elif len(snap_files) > 1:
        raise HarnessError(
            f"bundle directory contains {len(snap_files)} snapshot-shaped files "
            f"({', '.join(str(p) for p, _ in snap_files)}); exactly one is required"
        )
    for p, obj in artifact_files:
        for key in ("stack", "concepts"):
            if key in obj and key not in (snapshot or {}):
                extras[key] = obj[key]
    if snapshot is None:
        # No snapshot anywhere: still resolvable — checks will report invalid
        # against an empty snapshot. Distinguish provenance honestly.
        if artifact_files:
            return {}, f"{bundle_path} (no snapshot-shaped file; {len(artifact_files)} non-snapshot artifact(s) only)"
        raise HarnessError(f"bundle directory contains no JSON at all: {bundle_path}")
    for key, val in extras.items():
        snapshot.setdefault(key, val)
    return snapshot, f"{snap_src} (+{len(extras)} merged artifact file(s))" if extras else str(snap_src)


# --------------------------------------------------------------------------
# Snapshot helpers
# --------------------------------------------------------------------------

def sym_index(snapshot: dict) -> dict:
    return {s.get("id"): s for s in snapshot.get("symbols", []) if isinstance(s, dict)}


def summaries_of(snapshot: dict) -> list:
    return [s for s in snapshot.get("summaries", []) if isinstance(s, dict)]


def qual_name(sym: dict) -> str:
    return str(sym.get("qual_name") or sym.get("qualName") or sym.get("name") or "")


def match_qual(candidate: str, anchor: str) -> bool:
    """Dotted-boundary suffix match, case-insensitive:
    'SmsController.submitSms' matches 'com.x.SmsController.submitSms'."""
    c, a = candidate.casefold(), anchor.casefold()
    return c == a or c.endswith("." + a) or a.endswith("." + c)


def find_anchor_summaries(snapshot: dict, syms: dict, anchor: str) -> list:
    out = []
    for s in summaries_of(snapshot):
        sym = syms.get(s.get("sym"))
        if sym and match_qual(qual_name(sym), anchor):
            out.append((s, sym))
    return out


def find_summary_by_name_fragment(snapshot: dict, syms: dict, fragment: str) -> list:
    frag = fragment.casefold()
    out = []
    for s in summaries_of(snapshot):
        sym = syms.get(s.get("sym"))
        if sym and frag in qual_name(sym).casefold():
            out.append((s, sym))
    return out


def callee_texts(summary: dict, syms: dict) -> list:
    """Every callee string the summary exposes: resolved callee symbol names
    plus unresolved callee texts — the complete lexical surface."""
    texts = list(summary.get("unresolved_callees") or [])
    for cid in summary.get("callees", []):
        sym = syms.get(cid)
        if sym:
            texts.append(sym.get("name") or "")
            qn = qual_name(sym)
            if qn:
                texts.append(qn)
    return [t for t in texts if t]


def find_records_with_fields(node, fields: list, found=None):
    """Recursively locate dict records containing ALL named fields.
    Returns list of (record, path) — path is a JSON-ish breadcrumb."""
    if found is None:
        found = []
    if isinstance(node, dict):
        if all(f in node for f in fields):
            found.append((node, path_of(node, fields)))
        for k, v in node.items():
            find_records_with_fields(v, fields, found)
    elif isinstance(node, list):
        for item in node:
            find_records_with_fields(item, fields, found)
    return found


def path_of(record, fields):
    keys = []
    for f in fields:
        keys.append(f)
    return "{" + ", ".join(keys) + "}"


# --------------------------------------------------------------------------
# Check kinds (generic — all specifics come from fixture params)
# --------------------------------------------------------------------------

def check_app_identity_evidence_split(snapshot, params, evidence):
    """BB1: one physical source root must not be split into one app per
    manifest. Two honest shapes satisfy the oracle: ONE app record whose
    evidence list carries both manifests (the fixed engine), or multiple
    records over one dir explicitly linked by one application identity.
    The legacy shape — two records over one dir with no link — fails."""
    stack = snapshot.get("stack")
    if not isinstance(stack, dict):
        return INVALID, "no 'stack' artifact in bundle — identity surface absent; cannot evaluate app identity"
    apps = stack.get("apps")
    if not isinstance(apps, list) or not apps:
        return INVALID, "stack carries no apps records — cannot evaluate app identity"
    builds = {b.casefold() for b in params.get("manifest_builds", [])}

    def tools_of(a):
        # every manifest tool the record evidences: the build field plus the
        # per-manifest evidence list (manifests are evidence, never identity)
        tools = set()
        b = str(a.get("build", "")).casefold()
        if b:
            tools.add(b)
        for m in a.get("manifests") or []:
            if isinstance(m, dict):
                mt = str(m.get("tool", "")).casefold()
                if mt:
                    tools.add(mt)
        return tools

    # evaluability: both fixture manifests must be evidenced over some root
    evidenced = set()
    for a in apps:
        if isinstance(a, dict):
            evidenced |= tools_of(a) & builds
    if evidenced != builds:
        return INVALID, (
            f"expected both manifests ({', '.join(sorted(builds))}) evidenced over one root; "
            f"bundle evidences {', '.join(sorted(evidenced)) or 'none'} — oracle precondition absent"
        )

    groups = {}
    for a in apps:
        if isinstance(a, dict) and tools_of(a) & builds:
            groups.setdefault(a.get("dir", ""), []).append(a)
    split_roots = {}
    for d, recs in groups.items():
        tools = set()
        for r in recs:
            tools |= tools_of(r)
        tools &= builds
        if len(tools) < 2:
            continue  # only one of the fixture manifests lives over this root
        if len(recs) == 1:
            continue  # ONE record whose evidence list carries both manifests
        identities = {
            str(r.get("application", "")).casefold()
            for r in recs
            if r.get("application")
        }
        if len(identities) == 1 and all(
            str(r.get("application", "")).casefold() == next(iter(identities))
            for r in recs
        ):
            continue  # per-manifest records explicitly linked as one application
        split_roots[d] = recs
    for d, recs in sorted(split_roots.items()):
        evidence.append(
            f"root dir '{d or '<root>'}': one physical source root split into "
            f"{len(recs)} app records — "
            + "; ".join(f"build={r.get('build')} evidence={r.get('evidence')}" for r in recs)
        )
    if split_roots:
        return FAIL, (
            f"{len(split_roots)} source root(s) recorded as one app PER MANIFEST — "
            "application identity is manifest-scoped, not source-root-scoped"
        )
    return PASS, "every manifest pair over a shared root is recorded as one application with both evidence records"


def check_call_resolution_dimensions(snapshot, params, evidence):
    """BB2: call records must carry declaration / dispatch / observation as
    independent fields. A two-state resolved/unresolved model fails."""
    syms = sym_index(snapshot)
    anchor = params["anchor"]
    hits = find_anchor_summaries(snapshot, syms, anchor)
    if not hits:
        return INVALID, f"anchor symbol '{anchor}' absent from bundle — resolution model not evaluable"
    required = params.get("required_fields", [])
    for summary, sym in hits:
        qn = qual_name(sym)
        callees = summary.get("callees", [])
        unresolved = summary.get("unresolved_callees", [])
        evidence.append(
            f"{qn}: {len(callees)} resolved callee id(s), {len(unresolved)} unresolved callee text(s) "
            f"(keys present: {', '.join(sorted(k for k in summary.keys() if k not in ('sym',))) or 'none beyond sym'})"
        )
        records = find_records_with_fields(summary, required)
        if records:
            ok_values = True
            for rec, _ in records:
                dv = str(rec.get("dispatch", "")).casefold()
                if dv and dv not in {v.casefold() for v in params.get("dispatch_values_acceptable", [])}:
                    ok_values = False
                    evidence.append(f"dispatch value {dv!r} is not in the acceptable vocabulary")
            if ok_values:
                return PASS, f"{qn}: call records carry declaration/dispatch/observation as independent fields"
        # legacy shape: resolved/unresolved lists, no dispatch dimension anywhere
    return FAIL, (
        f"anchor '{anchor}' present but its call records carry no independent "
        f"declaration/dispatch/observation fields — the two-state resolved/unresolved "
        "model cannot state a solid declaration with a qualified runtime dispatch"
    )


def norm_token(s: str) -> str:
    """Normalize an effect/tag spelling for comparison: 'DbRead', 'db_read'
    and 'db-read' are the same claim under different serializations."""
    return re.sub(r"[^a-z0-9]", "", str(s).casefold())


def check_effect_provenance(snapshot, params, evidence):
    """BB3: every db_* effect claim must carry receiver/library provenance.
    Legacy effects are bare strings (spelling-classified, no basis)."""
    effect_names = {norm_token(e) for e in params.get("effect_spellings", [])}
    lexical = [l.casefold() for l in params.get("lexical_hint_spellings", [])]
    basis_fields = params.get("required_basis_fields", [])
    syms = sym_index(snapshot)
    claims = []
    no_text = []
    for summary in summaries_of(snapshot):
        effects = summary.get("effects", [])
        if isinstance(effects, list) and any(norm_token(e) in effect_names for e in effects):
            claims.append((summary, syms.get(summary.get("sym"))))
    if not claims:
        return INVALID, "no db_read/db_write effect claims anywhere in bundle — provenance requirement not evaluable"
    with_basis = [s for s, _ in claims if all(f in s for f in basis_fields)]
    lexical_only = []
    for summary, sym in claims:
        texts = callee_texts(summary, syms)
        if texts and any(any(sp in str(t).casefold() for sp in lexical) for t in texts):
            lexical_only.append(qual_name(sym) if sym else f"sym#{summary.get('sym')}")
        elif not texts:
            # builtin-classified calls leave NO callee text anywhere in the
            # bundle — the provenance gap is total: the effect cannot even be
            # traced back to the call that produced it
            no_text.append(qual_name(sym) if sym else f"sym#{summary.get('sym')}")
    if lexical_only:
        evidence.append(
            f"{len(lexical_only)} db effect claim(s) whose only DB-shaped evidence is a lexical "
            f"spelling ({', '.join(lexical[:3])}...) — the data.find false-positive shape: "
            + ", ".join(lexical_only[:5])
        )
    if no_text:
        evidence.append(
            f"{len(no_text)} db effect claim(s) record NO callee text at all "
            "(builtin-classified calls leave no trace of the call that produced them): "
            + ", ".join(no_text[:5])
        )
    if len(with_basis) == len(claims):
        return PASS, f"all {len(claims)} db effect claim(s) carry receiver/resolution-basis provenance"
    evidence.append(
        f"{len(claims) - len(with_basis)}/{len(claims)} db effect claim(s) are bare strings with no "
        f"{', '.join(basis_fields)} provenance — spelling-classified, receiver-blind"
    )
    return FAIL, (
        "db effect claims lack receiver/library provenance; the bundle cannot separate "
        "a real database API binding from a lexical .find/.save/.get hint"
    )


def check_auth_evidence(snapshot, params, evidence):
    """BB4: auth TAGS must carry non-lexical evidence; name-only matches must
    stay typed lexical hints (with their basis and gap note); FCA and
    contracts must not promote either. verify_* status checks are the
    standing counterexample."""
    tag = params.get("tag", "auth")
    ann_patterns = [p.casefold() for p in params.get("annotation_patterns", [])]
    callee_patterns = [p.casefold() for p in params.get("callee_patterns", [])]
    fca_intents = [i.casefold() for i in params.get("fca_intent_translations", [])]
    syms = sym_index(snapshot)
    tagged = []
    hinted = []
    for summary in summaries_of(snapshot):
        tags = summary.get("tags", [])
        if isinstance(tags, list) and tag in {str(t).casefold() for t in tags}:
            tagged.append((summary, syms.get(summary.get("sym"))))
        hints = summary.get("lexical_hints") or []
        if isinstance(hints, list) and any(
            isinstance(h, dict) and str(h.get("hint", "")).casefold() == tag
            for h in hints
        ):
            hinted.append((summary, syms.get(summary.get("sym"))))
    if not tagged and not hinted:
        return INVALID, (
            f"no '{tag}' tag and no typed lexical hint in bundle — "
            "auth-claim provenance not evaluable"
        )
    evidence_bearing, lexical_only = [], []
    for summary, sym in tagged:
        qn = qual_name(sym) if sym else f"sym#{summary.get('sym')}"
        annotations = [str(a).casefold() for a in (sym or {}).get("annotations", []) or []]
        texts = callee_texts(summary, syms)
        has_ann = any(p in a for p in ann_patterns for a in annotations)
        has_callee = any(p in str(t).casefold() for p in callee_patterns for t in texts)
        if has_ann or has_callee:
            evidence_bearing.append(qn)
        else:
            lexical_only.append(qn)
    if lexical_only:
        evidence.append(
            f"{len(lexical_only)} auth-tagged summary(ies) with ZERO non-lexical evidence "
            "(no authorization annotation, no authz-library callee): " + ", ".join(lexical_only[:5])
        )
    # typed hints must say what they are: a lexical basis and an honest gap note
    unlabeled = []
    for summary, sym in hinted:
        qn = qual_name(sym) if sym else f"sym#{summary.get('sym')}"
        recs = [
            h
            for h in (summary.get("lexical_hints") or [])
            if isinstance(h, dict) and str(h.get("hint", "")).casefold() == tag
        ]
        for h in recs:
            basis = str(h.get("basis", "")).casefold()
            note = str(h.get("note", ""))
            if "lexical" not in basis or not note.strip():
                unlabeled.append(qn)
    if unlabeled:
        evidence.append(
            f"{len(unlabeled)} '{tag}' lexical hint(s) do not state their lexical basis "
            "and gap note — the hint is not honestly labeled: " + ", ".join(unlabeled[:5])
        )
    promoted = []
    label_patterns = [p.casefold() for p in params.get("fca_label_patterns", [])]
    concepts = snapshot.get("concepts") or {}
    for node in concepts.get("nodes", []) if isinstance(concepts, dict) else []:
        if not isinstance(node, dict):
            continue
        intent = {str(i).casefold() for i in node.get("intent", []) or []}
        label = str(node.get("label", "")).casefold()
        if (
            any(i in intent for i in fca_intents)
            or any(p in label for p in label_patterns)
        ):
            promoted.append(str(node.get("label") or node.get("id")))
    # normative contracts derived from the same lexical match (MustAuthorize …)
    contract_patterns = [p.casefold() for p in params.get("contract_text_patterns", [])]
    contract_hits = []
    for contract in snapshot.get("contracts", []) or []:
        if not isinstance(contract, dict):
            continue
        text = str(contract.get("expr", "")).casefold()
        if any(p in text for p in contract_patterns):
            contract_hits.append(
                f"{contract.get('obligation', 'contract')}: {contract.get('expr')}"
            )
    if promoted:
        evidence.append(
            "FCA promoted the bare tag into concept claims: " + ", ".join(promoted[:5])
        )
    if contract_hits:
        evidence.append(
            f"{len(contract_hits)} normative contract(s) assert an authorization obligation "
            "derived from the same spelling: " + " | ".join(contract_hits[:2])
        )
    if not lexical_only and not unlabeled and not promoted and not contract_hits:
        return PASS, (
            f"{len(tagged)} auth tag(s) carry non-lexical validation evidence "
            f"(or none exist), {len(hinted)} lexical hint(s) are typed with basis and gap note; "
            "no FCA/contract promotion"
        )
    return FAIL, (
        "authorization claims exist without validation evidence — lexical name matches "
        f"('verify_'-style) were promoted into the '{tag}' tag"
        + (" and FCA 'authorization' concepts" if promoted else "")
        + (" and normative MustAuthorize-style contracts" if contract_hits else "")
        + ", creating security claims the source never made"
    )


def check_call_site_occurrence_identity(snapshot, params, evidence):
    """BB5: repeated calls must be distinct call sites with per-occurrence
    identity (span/occurrence/arguments/guard). Legacy callee lists carry
    no occurrence data."""
    syms = sym_index(snapshot)
    anchor = params["anchor"]
    hits = find_anchor_summaries(snapshot, syms, anchor)
    if not hits:
        return INVALID, f"anchor symbol '{anchor}' absent from bundle — occurrence identity not evaluable"
    required = params.get("required_occurrence_fields", [])
    for summary, sym in hits:
        qn = qual_name(sym)
        callees = summary.get("callees", [])
        dup = len(callees) - len(set(callees))
        evidence.append(
            f"{qn}: callee list length {len(callees)}"
            + (f" ({dup} bare duplicate id(s) — repeats survive only as adjacent duplicates)" if dup > 0 else "")
            + "; no per-occurrence record type exists"
        )
        records = find_records_with_fields(summary, required)
        if records:
            return PASS, f"{qn}: callee occurrences carry per-occurrence identity fields"
        # explicit negative evidence: none of span/occurrence/arguments/guard exists
        present = [f for f in required if f in summary]
        evidence.append(
            f"occurrence fields absent from summary (found {present or 'none of ' + ', '.join(required)})"
        )
    return FAIL, (
        f"anchor '{anchor}' present but repeated calls are not distinct call sites: "
        "no per-occurrence span/occurrence/arguments/guard records — occurrences are "
        "joined by caller plus callee text, exactly the diagnosed storage defect"
    )


def check_source_order_records(snapshot, params, evidence):
    """BB6: diagram derivations must be reconstructible from stored source
    order; repeats must survive; 'no evidenced interactions' must be a typed
    record. Legacy bundles store none of the three."""
    summaries = summaries_of(snapshot)
    if not summaries:
        return INVALID, "bundle carries no summaries — order/repeat/no-interaction expressiveness not evaluable"
    order_fields = params.get("required_order_fields", [])
    # any call record anywhere carrying source-order fields?
    ordered = []
    for summary in summaries:
        ordered.extend(find_records_with_fields(summary, order_fields))
    total_callees = sum(len(s.get("callees", []) or []) for s in summaries)
    evidence.append(
        f"{total_callees} resolved callee record(s) across {len(summaries)} summaries; "
        f"{len(ordered)} record(s) carry source-order fields ({', '.join(order_fields)})"
    )
    if not ordered:
        return FAIL, (
            "call records carry no source order (statement index / byte-span order): "
            "chronology can only be derived by BFS over a visited-symbol set with symbol-id "
            "tie-breaks, repeats collapse, and 'no evidenced interactions' has no typed record — "
            "the bundle cannot support a source-ordered sequence provider"
        )
    if params.get("require_repeat_expression") or params.get("require_no_interaction_record"):
        missing = []
        if params.get("require_repeat_expression") and not any(
            r.get("occurrence") is not None for r, _ in ordered
        ):
            missing.append("per-occurrence repetition")
        if params.get("require_no_interaction_record") and not find_records_with_fields(
            snapshot, ["reason"]
        ):
            missing.append("typed no-interaction record")
        if missing:
            return FAIL, "source order present but bundle cannot express: " + ", ".join(missing)
    return PASS, "call records carry source order, repeats and typed no-interaction records"


CHECK_KINDS = {
    "app_identity_evidence_split": check_app_identity_evidence_split,
    "call_resolution_dimensions": check_call_resolution_dimensions,
    "effect_provenance": check_effect_provenance,
    "auth_evidence": check_auth_evidence,
    "call_site_occurrence_identity": check_call_site_occurrence_identity,
    "source_order_records": check_source_order_records,
}


# --------------------------------------------------------------------------
# Evaluation + reporting
# --------------------------------------------------------------------------

def evaluate(fixture: dict, snapshot: dict, provenance: str):
    results = []
    for exp in fixture["expectations"]:
        check = exp["check"]
        kind, params = check.get("kind"), check.get("params", {})
        fn = CHECK_KINDS.get(kind)
        if fn is None:
            results.append({
                "id": exp["id"],
                "status": INVALID,
                "reason": f"unknown check kind {kind!r} — fixture references an unimplemented check",
                "expected": exp["expected"],
                "evidence": [],
            })
            continue
        try:
            evidence = []
            status, reason = fn(snapshot, params, evidence)
        except Exception as e:  # noqa: BLE001 — a check crash is an honest invalid, never a pass
            status, reason = INVALID, f"check raised {type(e).__name__}: {e}"
        results.append({
            "id": exp["id"],
            "status": status,
            "reason": reason,
            "expected": exp["expected"],
            "evidence": evidence,
        })
    counts = {s: sum(1 for r in results if r["status"] == s) for s in (PASS, FAIL, INVALID)}
    return results, counts, provenance


def render_human(fixture: dict, results, counts, provenance, bundle_arg: str):
    base = fixture["baseline"]
    exp = base["expected_result"]
    print("=" * 78)
    print("SMPP behavioural baseline — read-only oracle replay (Stage 0)")
    print("=" * 78)
    print(f"bundle   : {bundle_arg} -> {provenance}")
    print(f"baseline : CIE {base['cie_commit']} · {base['qa_date']}")
    print(f"subject  : {base['subject']}")
    print(f"oracle   : {base['oracle_basis']}")
    print("-" * 78)
    for r in results:
        marker = {"pass": "PASS", "fail": "FAIL", "invalid": "INVALID"}[r["status"]]
        print(f"[{marker:>7}] {r['id']}")
        print(f"          {r['reason']}")
        for ev in r["evidence"]:
            print(f"            · {ev}")
    print("-" * 78)
    print(
        f"result   : {counts[PASS]} pass / {counts[FAIL]} fail / {counts[INVALID]} invalid"
        f"   (fixture expected {exp['pass']} / {exp['fail']} / {exp['invalid']})"
    )
    match = (counts[PASS], counts[FAIL], counts[INVALID]) == (exp["pass"], exp["fail"], exp["invalid"])
    if match:
        if counts[FAIL] and not counts[PASS]:
            print("verdict  : baseline REPRODUCED — every checkable expectation fails for its diagnosed reason.")
            print("           These are recorded defects, not repaired ones. Stage 0 gate satisfied.")
        else:
            print("verdict  : oracle satisfied as the fixture currently expects.")
    else:
        print("verdict  : DRIFT — observed counts differ from the fixture's expected_result.")
        print("           Either the bundle's behaviour changed against the contract, or the")
        print("           oracle must be revisited by an explicit fixture revision. Never edit")
        print("           expectations to make a run pass.")
    return 0 if match else 1


# --------------------------------------------------------------------------
# Self-test — synthetic bundles proving the harness's distinct outcomes
# --------------------------------------------------------------------------

def _legacy_shaped_bundle() -> dict:
    """A synthetic bundle shaped exactly like a CIE 5267564 RevSnapshot,
    reproducing every diagnosed legacy defect — NOT the real capture (the
    real capture's numbers are recorded in the fixture's baseline block)."""
    return {
        "rev": "r-selftest",
        "files": ["pom.xml", "build.gradle", "src/main/java/sms/SmsController.java"],
        "symbols": [
            {"id": 1, "kind": "class", "name": "SmsController", "qual_name": "sms.SmsController",
             "loc": {"file": 2, "start": 0, "end": 900}, "lang": 4, "parent": None, "rev": "r-selftest",
             "annotations": ["@RestController"]},
            {"id": 2, "kind": "method", "name": "submitSms", "qual_name": "sms.SmsController.submitSms",
             "loc": {"file": 2, "start": 120, "end": 800}, "lang": 4, "parent": 1, "rev": "r-selftest",
             "annotations": ["@PostMapping(\"/sms\")"]},
            {"id": 3, "kind": "method", "name": "verify_all", "qual_name": "sms.RecordVerifier.verify_all",
             "loc": {"file": 2, "start": 810, "end": 890}, "lang": 4, "parent": None, "rev": "r-selftest",
             "annotations": []},
        ],
        "summaries": [
            {"sym": 2, "effects": ["db_read"], "tags": ["query"], "callees": [3, 3], "unresolved_callees": ["data.find"],
             "complete": True},
            {"sym": 3, "effects": [], "tags": ["auth"], "callees": [], "unresolved_callees": [], "complete": True},
        ],
        "stack": {
            "apps": [
                {"dir": "", "name": "sms-maven", "build": "maven", "framework": "spring", "evidence": "pom.xml"},
                {"dir": "", "name": "sms-gradle", "build": "gradle", "framework": "spring", "evidence": "build.gradle"},
            ]
        },
        "concepts": {
            "nodes": [
                {"id": 9001, "label": "Authorize verify all", "kind": "UseCase", "intent": ["authorization"],
                 "symbols": [3], "confidence": 0.8, "provenance": "fca"},
            ]
        },
    }


def _degenerate_bundle() -> dict:
    """A bundle with the SMPP evidence stripped: every expectation must come
    out INVALID (or FAIL-on-structure), and NOT ONE may silently pass."""
    return {
        "rev": "r-empty",
        "files": ["README.md"],
        "symbols": [{"id": 1, "kind": "fn", "name": "helper", "qual_name": "util.helper",
                     "loc": {"file": 0, "start": 0, "end": 10}, "lang": 1, "parent": None, "rev": "r-empty",
                     "annotations": []}],
        "summaries": [{"sym": 1, "effects": [], "tags": [], "callees": [], "unresolved_callees": [],
                       "complete": True}],
    }


def _fixed_shaped_bundle() -> dict:
    """A synthetic bundle shaped like the FIXED engine's capture (post
    behavioural-evidence fixes): one merged app with both manifests, rich
    per-call records, provenance-bearing db claims, typed hints, no
    promotion. Every expectation must PASS — 6/0/0."""
    return {
        "rev": "r-fixed",
        "files": ["pom.xml", "build.gradle", "src/main/java/sms/SmsController.java",
                  "src/main/java/sms/SubmissionService.java"],
        "symbols": [
            {"id": 1, "kind": "class", "name": "SmsController", "qual_name": "sms.SmsController",
             "loc": {"file": 2, "start": 0, "end": 900}, "lang": 4, "parent": None, "rev": "r-fixed",
             "annotations": ["@RestController"]},
            {"id": 2, "kind": "method", "name": "submitSms", "qual_name": "sms.SmsController.submitSms",
             "loc": {"file": 2, "start": 120, "end": 800}, "lang": 4, "parent": 1, "rev": "r-fixed",
             "annotations": ["@PostMapping(\"/sms\")"]},
            {"id": 3, "kind": "method", "name": "verify_all", "qual_name": "sms.RecordVerifier.verify_all",
             "loc": {"file": 2, "start": 810, "end": 890}, "lang": 4, "parent": None, "rev": "r-fixed",
             "annotations": []},
            {"id": 4, "kind": "method", "name": "persist", "qual_name": "sms.SubmissionService.persist",
             "loc": {"file": 3, "start": 0, "end": 400}, "lang": 4, "parent": None, "rev": "r-fixed",
             "annotations": []},
        ],
        "summaries": [
            {"sym": 2, "effects": [], "tags": [], "callees": [], "unresolved_callees": [],
             "complete": True,
             "call_sites": [
                 {"callee": "submissionService.submit", "receiver": "submissionService",
                  "receiver_type": "sms.SubmissionService", "declaration": "resolved",
                  "dispatch": "candidate", "observation": "unobserved",
                  "span": {"start": 180, "end": 210}, "occurrence": 1, "arguments": 1,
                  "guard": "unconditional", "source_order": 0, "statement_index": 1,
                  "line": 14},
                 {"callee": "logger.info", "receiver": "logger",
                  "receiver_type": "org.slf4j.Logger", "declaration": "external",
                  "dispatch": "external", "observation": "unobserved",
                  "span": {"start": 240, "end": 262}, "occurrence": 1, "arguments": 1,
                  "guard": "if (resp == null)", "source_order": 1, "statement_index": 2,
                  "line": 15},
             ]},
            {"sym": 3, "effects": [], "tags": [], "callees": [], "unresolved_callees": [],
             "complete": True,
             "lexical_hints": [
                 {"hint": "auth", "basis": "lexical-name-match",
                  "note": "no authorization annotation and no authorization-library callee observed in this revision"},
             ]},
            {"sym": 4, "effects": ["DbWrite"], "tags": ["mutation"], "callees": [],
             "unresolved_callees": [], "complete": True,
             "receiver": ["SmsOutboundRepository"],
             "resolution_basis": ["type-resolved-api-identity"],
             "call_sites": [
                 {"callee": "outboundRepository.save", "receiver": "outboundRepository",
                  "receiver_type": "sms.SmsOutboundRepository", "declaration": "resolved",
                  "dispatch": "candidate", "observation": "unobserved",
                  "span": {"start": 90, "end": 122}, "occurrence": 1, "arguments": 1,
                  "guard": "if (existing.getRequestId() == null)", "source_order": 0,
                  "statement_index": 1, "line": 22,
                  "effect": {"kind": "DbWrite", "receiver": "SmsOutboundRepository",
                             "resolution_basis": "type-resolved-api-identity"}},
                 {"callee": "outboundRepository.save", "receiver": "outboundRepository",
                  "receiver_type": "sms.SmsOutboundRepository", "declaration": "resolved",
                  "dispatch": "candidate", "observation": "unobserved",
                  "span": {"start": 260, "end": 292}, "occurrence": 2, "arguments": 1,
                  "guard": "unconditional", "source_order": 1, "statement_index": 2,
                  "line": 41,
                  "effect": {"kind": "DbWrite", "receiver": "SmsOutboundRepository",
                             "resolution_basis": "type-resolved-api-identity"}},
             ]},
        ],
        "stack": {
            "apps": [
                {"dir": "", "name": "smpp-mls", "build": "gradle", "framework": "spring",
                 "evidence": "build.gradle",
                 "manifests": [
                     {"tool": "gradle", "evidence": "build.gradle"},
                     {"tool": "maven", "evidence": "pom.xml"},
                 ]},
            ]
        },
        "interaction_gaps": [
            {"sym": 3, "kind": "no_evidenced_interactions",
             "reason": "no call records reference this symbol and it invokes none in this revision"},
        ],
        "concepts": {"nodes": []},
    }


def self_test() -> int:
    fixture = load_fixture(FIXTURE_DEFAULT)
    ok = True

    print("SELF-TEST 1: legacy-shaped synthetic bundle — must reproduce 0 pass / 6 fail / 0 invalid")
    results, counts, _ = evaluate(fixture, _legacy_shaped_bundle(), "synthetic legacy shape")
    for r in results:
        print(f"  [{r['status']:>7}] {r['id']}")
    if (counts[PASS], counts[FAIL], counts[INVALID]) != (0, 6, 0):
        print(f"  MISMATCH: got {counts[PASS]} pass / {counts[FAIL]} fail / {counts[INVALID]} invalid")
        ok = False
    else:
        print("  reproduced: 0 pass / 6 fail / 0 invalid (the legacy defects stay caught)")

    print("SELF-TEST 2: degenerate bundle (SMPP evidence stripped) — zero silent passes")
    results2, counts2, _ = evaluate(fixture, _degenerate_bundle(), "synthetic degenerate")
    for r in results2:
        print(f"  [{r['status']:>7}] {r['id']}")
    if counts2[PASS] != 0:
        print(f"  SILENT PASS DETECTED: {counts2[PASS]} expectation(s) passed on absent evidence")
        ok = False
    else:
        print(f"  no silent pass: {counts2[PASS]} pass / {counts2[FAIL]} fail / {counts2[INVALID]} invalid")

    print("SELF-TEST 3: fixed-shape synthetic bundle — must satisfy 6 pass / 0 fail / 0 invalid")
    results3, counts3, _ = evaluate(fixture, _fixed_shaped_bundle(), "synthetic fixed shape")
    for r in results3:
        print(f"  [{r['status']:>7}] {r['id']}  ({r['reason'][:96]})")
    if (counts3[PASS], counts3[FAIL], counts3[INVALID]) != (6, 0, 0):
        print(f"  MISMATCH: got {counts3[PASS]} pass / {counts3[FAIL]} fail / {counts3[INVALID]} invalid")
        ok = False
    else:
        print("  satisfied: 6 pass / 0 fail / 0 invalid (the fixed shape is recognizably honest)")

    print()
    if ok:
        print("SELF-TEST OK — legacy shape 0/6/0, absence never passes, fixed shape 6/0/0.")
        return 0
    print("SELF-TEST FAILED — harness must be fixed before use.")
    return 1


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------

def main(argv=None) -> int:
    ap = argparse.ArgumentParser(
        description="Read-only Stage 0 oracle replay: six SMPP behavioural expectations against a captured legacy bundle.",
    )
    ap.add_argument("bundle", nargs="?", help="captured bundle: snapshot JSON file, {'snapshots':[...]} wrapper, or artifact directory")
    ap.add_argument("--fixture", type=Path, default=FIXTURE_DEFAULT, help=f"oracle fixture (default: {FIXTURE_DEFAULT.name})")
    ap.add_argument("--json", action="store_true", help="emit machine-readable JSON instead of the human table")
    ap.add_argument("--self-test", action="store_true", help="run against embedded synthetic bundles and verify distinct statuses")
    args = ap.parse_args(argv)

    if args.self_test:
        return self_test()
    if not args.bundle:
        ap.error("bundle path is required (or use --self-test)")

    try:
        fixture = load_fixture(args.fixture)
        snapshot, provenance = load_bundle(Path(args.bundle))
    except HarnessError as e:
        print(f"harness error: {e}", file=sys.stderr)
        return 2

    results, counts, provenance = evaluate(fixture, snapshot, provenance)
    if args.json:
        exp = fixture["baseline"]["expected_result"]
        print(json.dumps({
            "bundle": args.bundle,
            "provenance": provenance,
            "counts": counts,
            "expected": {"pass": exp["pass"], "fail": exp["fail"], "invalid": exp["invalid"]},
            "matches_expected": (counts[PASS], counts[FAIL], counts[INVALID]) == (exp["pass"], exp["fail"], exp["invalid"]),
            "results": results,
        }, indent=2))
        return 0 if (counts[PASS], counts[FAIL], counts[INVALID]) == (exp["pass"], exp["fail"], exp["invalid"]) else 1
    return render_human(fixture, results, counts, provenance, args.bundle)


if __name__ == "__main__":
    sys.exit(main())
