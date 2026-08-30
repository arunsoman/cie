"""R13 — C++ extraction (#8), sharing R12's declarator path with the
C++-specific extensions: class_specifier/struct_specifier WITH name+body
are CLASS nodes (`.c`'s struct gate stays test-pinned in
test_extract_c.py), methods via field_identifier inside class bodies,
out-of-class definitions via qualified_identifier's `name` part, and
base_class_clause inheritance (access specifiers stripped — all bases
`extends`).

Grammar facts verified against real tree-sitter-cpp parses, not assumed
(see the R13 notes in cie/extract.py's tables).
"""

from __future__ import annotations

from pathlib import Path

from cie.callgraph import resolve_call_edges
import pytest

from cie.extract import extract_many

CPP_SOURCE = """\
#include "net/http_client.h"

namespace net {

class HttpClient : public Base, private Logger {
 public:
  void send(int retries) {
    log(retries);
    return;
  }
  int status();
};

}  // namespace net

int HttpClient::status() {
  return send(1), 0;
}

void free_function() {
  return;
}
"""


@pytest.fixture()
def cpp_project(tmp_path):
    (tmp_path / "http_client.cpp").write_text(CPP_SOURCE)
    (tmp_path / "net_http_client.hpp").write_text(
        "#pragma once\nnamespace net {\nclass HttpClient;\n}\n"
    )
    return tmp_path


def _funcs(cpp_project):
    per = extract_many(cpp_project)
    return per, {
        n["label"] for ext in per for n in ext.nodes if n["kind"] in ("function", "method")
    }


import pytest

from cie.extract import extract_many


def test_cpp_never_silently_skips_functions(cpp_project):
    """Positive set-equality guard (R12's bar): the inline method, the
    out-of-class definition, and the free function ALL extract."""
    _, names = _funcs(cpp_project)
    assert {"send", "status", "free_function"} <= names


def test_cpp_class_node_emitted_with_bases(cpp_project):
    per, _ = _funcs(cpp_project)
    classes = {n["label"] for ext in per for n in ext.nodes if n["kind"] == "class"}
    assert "HttpClient" in classes
    bases = [
        (b["name"], b["relation"])
        for ext in per for b in ext.class_bases
    ]
    assert ("Base", "extends") in bases
    assert ("Logger", "extends") in bases  # access specifiers stripped


def test_cpp_methods_are_class_scoped(cpp_project):
    per, _ = _funcs(cpp_project)
    method_rows = [
        (n["label"], n["kind"]) for ext in per for n in ext.nodes
        if n["label"] == "send"
    ]
    assert method_rows == [("send", "method")]


def test_cpp_out_of_class_definition_via_qualified_identifier(cpp_project):
    per, names = _funcs(cpp_project)
    # `int HttpClient::status()` — the name part of qualified_identifier
    assert "status" in names


def test_cpp_call_sites_resolve(cpp_project):
    """Known truth: status -> send resolves (same-file, EXTRACTED).
    `log` is called but never defined in the file — it stays UNRESOLVED
    (R7's resolution tallies count it; no phantom target is invented)."""
    per = extract_many(cpp_project)
    edges = resolve_call_edges(per)
    targets = {e["target"].split("::")[-1].split("@")[0] for e in edges}
    assert "send" in targets
    assert "log" not in targets  # undefined function: no phantom edge


def test_cpp_struct_with_body_is_a_class_forward_decl_is_not(tmp_path):
    (tmp_path / "s.cpp").write_text(
        "struct WithBody { int x; };\nstruct Forward;\n"
    )
    per = extract_many(tmp_path)
    classes = {n["label"] for ext in per for n in ext.nodes if n["kind"] == "class"}
    assert classes == {"WithBody"}