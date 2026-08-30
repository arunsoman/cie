"""R12 — tree-sitter C extraction, with the naive-skip guard.

D1 (2026-08-29) evaluated C and DEFERRED it with a real reason: "its
function name/params sit two levels deep inside a `function_declarator`
with no direct `name`/`parameters` field, so the existing field-based
extraction helpers would silently return `""`/skip the function entirely
rather than partially work". R12 ships the declarator-unwrapping logic
(`cie.extract._c_declarator_name` and friends), and THIS file is the
guard: every test asserts EXACT symbol sets from known-by-inspection
truth, and test_extract_never_silently_skips would fail if extraction
ever started returning nothing for a well-formed C file — the exact bug
class D1 named.

Grammar facts verified against real tree-sitter-c parses (and encoded in
the helpers), not assumed:
- `function_definition` has NO `declarator`-adjacent `name` field; the
  name is the innermost identifier of the `declarator` chain;
  declarators nest (`pointer_declarator`, `parenthesized_declarator`,
  `init_declarator` wrapping `function_declarator`).
- parameters live in `function_declarator`'s `parameter_list` (not a
  direct child of the definition) — and parameter names are identifiers
  too, so the "first identifier anywhere" shortcut would return the
  first PARAM name (a wrong-but-plausible answer; asserted against).
"""

from __future__ import annotations

from pathlib import Path

import pytest

from cie.callgraph import resolve_call_edges
from cie.extract import extract_many, supported_suffix

C_SOURCE = """\
#include "app.h"

/* alpha: the entry point under test */
static int alpha(int x) {
    return helper(x) + 1;
}

int *make_buffer(size_t n) {
    return 0;
}

int helper(int y) {
    return y * -1;
}

int main(void) {
    struct Config c;
    return alpha(3);
}
"""

#: KNOWN-BY-INSPECTION truth for C_SOURCE's functions (name, signature):
EXPECTED_FUNCTIONS = {
    "alpha",       # nested once: function_declarator -> declarator: identifier
    "make_buffer", # nested twice: function_declarator -> pointer_declarator -> identifier
    "helper",      # plain
    "main",        # void params
}


@pytest.fixture()
def c_project(tmp_path):
    (tmp_path / "app.c").write_text(C_SOURCE)
    (tmp_path / "app.h").write_text(
        "#ifndef APP_H\n#define APP_H\nint alpha(int x);\n#endif\n"
    )
    return tmp_path


# ---------------------------------------------------------------------------
# The naive-skip guard (the reason R12 exists)
# ---------------------------------------------------------------------------


def test_extract_never_silently_skips_well_formed_functions(c_project):
    """Would fail the moment extraction ever silently returned nothing —
    D1's named bug class. Positive set-equality, not merely non-crash."""
    per_file = extract_many(c_project)
    extracted = {
        n["label"]
        for ext in per_file
        for n in ext.nodes
        if n["kind"] == "function"
    }
    assert EXPECTED_FUNCTIONS <= extracted, (
        f"extraction silently skipped(s) {[e for e in EXPECTED_FUNCTIONS if e not in extracted]} "
        "— the declarator-unwrap regressed (R12's whole point)"
    )


def test_extracted_names_are_not_parameter_names(c_project):
    """The MIRROR bug: a 'first identifier inside the declarator anywhere'
    shortcut returns the first PARAM name (x/y/n) — a wrong-but-plausible
    answer. Pinned here so the unwrap never regresses that way."""
    per_file = extract_many(c_project)
    names = {
        n["label"] for ext in per_file for n in ext.nodes if n["kind"] == "function"
    }
    assert not {"x", "y", "n", "c", "self"} & names


def test_c_suffix_is_registered(c_project):
    c_file = c_project / "app.c"
    h_file = c_project / "app.h"
    assert supported_suffix(c_file) == ".c"
    assert supported_suffix(h_file) == ".h"


def test_signatures_carry_c_types_and_params(c_project):
    per_file = extract_many(c_project)
    sigs = {
        n["label"]: n["signature"]
        for ext in per_file
        for n in ext.nodes
        if n["kind"] == "function"
    }
    # _build_signature's convention: name(params) -> return-type
    assert sigs["alpha"] == "alpha(int x) -> static int"
    assert sigs["make_buffer"] == "make_buffer(size_t n) -> int"
    assert sigs["main"] == "main(void) -> int"


def test_call_sites_resolve_through_the_c_call_graph(c_project):
    """Known truth: alpha calls helper (same-file, line 6); main calls
    alpha (line 17); alpha@main is called by nobody."""
    per_file = extract_many(c_project)
    edges = resolve_call_edges(per_file)
    resolved = {
        e["target"].split("::")[-1].split("@")[0] for e in edges
    }
    assert "helper" in resolved and "alpha" in resolved
    # every caller-side attribution names the calling function's node
    for e in edges:
        assert e["source"].split("::")[0].endswith(".c")
        assert e["confidence"] in ("EXTRACTED", "INFERRED", "AMBIGUOUS")


def test_headers_extract_as_file_hubs_prototypes_documented_out(c_project):
    """A `.h` extracts (file hub present in the graph) — and PROTOTYPE
    declarations deliberately do NOT become separate FUNC nodes: the
    definition carries graph identity, and a duplicate would fork
    same-name resolution (documented v1 scope in `_LANG_LOADERS`'s R12
    note). This test pins that decision so it can't flip silently."""
    per_file = extract_many(c_project)
    header_files = [
        n for ext in per_file for n in ext.nodes
        if n["kind"] == "file" and n["id"].endswith(".h")
    ]
    assert header_files, "the .h file must be extracted, not skipped"
    # and the definition (not the prototype) is the one alpha node
    alpha_nodes = [
        (n["id"], n["kind"]) for ext in per_file for n in ext.nodes
        if n["label"] == "alpha" and n["kind"] == "function"
    ]
    assert len(alpha_nodes) == 1
    assert alpha_nodes[0][0].endswith("app.c::alpha@4")


def test_structs_are_not_classes_documented_scope(c_project):
    """v1 scope (documented, matching the Go/Rust precedent): C structs
    produce NO class nodes — `struct Config` is invisible to class
    extraction rather than wrongly shaped as one."""
    per_file = extract_many(c_project)
    assert not any(n["kind"] == "class" for ext in per_file for n in ext.nodes)


def test_c_docstring_is_empty_not_a_crash(c_project):
    """Same graceful treatment as Go/Rust's documented empty-docstring
    gap (the /* comment */ is not attached in v1)."""
    per_file = extract_many(c_project)
    for ext in per_file:
        for n in ext.nodes:
            if n["kind"] == "function":
                assert n["docstring"] == ""