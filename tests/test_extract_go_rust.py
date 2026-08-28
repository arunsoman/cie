"""Tests for Go and Rust support added to cie.extract (out-of-box language
breadth was cie's largest documented gap vs. every competitor surveyed
in docs/competitive-landscape.md).

Every assertion here is checked against a real tree-sitter parse of real
source, the same standard the existing Python/JS/TS/Java extraction
tests hold themselves to — no mocking the parser.
"""

from __future__ import annotations

from cie import extract
from cie.models import NodeKind

_GO_SOURCE = '''package main

import "fmt"

// Add adds two integers.
func Add(a int, b int) int {
	return a + b
}

type Server struct {
	Name string
}

func (s *Server) Greet() string {
	x := s.helper()
	return fmt.Sprintf("hi %s", x)
}

func (s *Server) helper() string {
	return s.Name
}
'''

_RUST_SOURCE = '''use std::fmt;

fn add(a: i32, b: i32) -> i32 {
    a + b
}

struct Server {
    name: String,
}

impl Server {
    fn greet(&self) -> String {
        let x = self.helper();
        format!("hi {}", x)
    }

    fn helper(&self) -> String {
        self.name.clone()
    }
}
'''


def test_go_suffix_is_registered():
    assert ".go" in extract.lang_adapter.all_supported_suffixes()


def test_rust_suffix_is_registered():
    assert ".rs" in extract.lang_adapter.all_supported_suffixes()


def test_go_extracts_function_and_receiver_methods_with_correct_signatures(tmp_path):
    """Go has no class node (_CLASS_TYPES has no Go entry — structs
    aren't extracted as classes, verified below), so a receiver method
    like `func (s *Server) Greet()` has no enclosing class to attach to
    and surfaces as kind=FUNC, not METHOD — a real, deliberate scope
    limit (extract.py's module docstring), not a bug. It's still fully
    searchable/callable/signature-correct, and the receiver is visible
    in the signature itself ('Greet(s *Server) -> string')."""
    path = tmp_path / "server.go"
    path.write_text(_GO_SOURCE)
    extraction = extract.extract_file(path)

    funcs = {n["label"]: n for n in extraction.nodes if n["kind"] == NodeKind.FUNC.value}
    assert funcs["Add"]["signature"] == "Add(a int, b int) -> int"
    assert funcs["Greet"]["signature"] == "Greet(s *Server) -> string"
    assert funcs["helper"]["signature"] == "helper(s *Server) -> string"


def test_go_docstring_is_empty_not_a_crash(tmp_path):
    """A real, documented gap (extract.py's own module docstring) — Go's
    leading `//` comment isn't extracted as a docstring. Verifies the
    degradation is graceful (empty string), not an exception."""
    path = tmp_path / "server.go"
    path.write_text(_GO_SOURCE)
    extraction = extract.extract_file(path)
    add = next(n for n in extraction.nodes if n["label"] == "Add")
    assert add["docstring"] == ""


def test_go_call_sites_resolve_receiver_method_calls_correctly(tmp_path):
    """The real correctness risk this change had to get right: Go's
    selector_expression (`s.helper()`) must resolve to the method name
    'helper', not the receiver 's' — see extract.py's _call_target
    generalization."""
    path = tmp_path / "server.go"
    path.write_text(_GO_SOURCE)
    extraction = extract.extract_file(path)

    call_names = {c["called_name"] for c in extraction.call_sites}
    assert "helper" in call_names
    assert "Sprintf" in call_names
    # The bug this would have been without the fix: "s" (the receiver)
    # showing up as a called name instead of the real method.
    assert "s" not in call_names
    assert "fmt" not in call_names


def test_rust_extracts_free_function_and_impl_methods(tmp_path):
    """Same scope limit as Go's receiver methods above: an `impl` block
    isn't a _CLASS_TYPES match, so its methods surface as kind=FUNC, not
    METHOD — still fully searchable/callable/signature-correct."""
    path = tmp_path / "server.rs"
    path.write_text(_RUST_SOURCE)
    extraction = extract.extract_file(path)

    funcs = {n["label"]: n for n in extraction.nodes if n["kind"] == NodeKind.FUNC.value}
    assert funcs["add"]["signature"] == "add(a: i32, b: i32) -> i32"
    assert funcs["greet"]["signature"] == "greet(&self) -> String"
    assert funcs["helper"]["signature"] == "helper(&self) -> String"


def test_rust_call_sites_resolve_receiver_method_calls_correctly(tmp_path):
    path = tmp_path / "server.rs"
    path.write_text(_RUST_SOURCE)
    extraction = extract.extract_file(path)

    call_names = {c["called_name"] for c in extraction.call_sites}
    assert "helper" in call_names
    assert "clone" in call_names
    assert "self" not in call_names


def test_go_and_rust_produce_no_import_edges_a_documented_gap_not_a_crash(tmp_path):
    """extract.py's module docstring states this plainly: _collect_imports
    has no Go/Rust branch, so imports stays empty for these two languages
    — verified here as the actual (harmless) behavior, not assumed."""
    go_path = tmp_path / "a.go"
    go_path.write_text(_GO_SOURCE)
    rs_path = tmp_path / "b.rs"
    rs_path.write_text(_RUST_SOURCE)
    assert extract.extract_file(go_path).imports == []
    assert extract.extract_file(rs_path).imports == []


def test_go_struct_is_not_treated_as_a_class(tmp_path):
    """Go has no class construct — extract.py's _CLASS_TYPES deliberately
    has no Go entry (module comment). A struct declaration should not
    show up as a NodeKind.CLASS node."""
    path = tmp_path / "server.go"
    path.write_text(_GO_SOURCE)
    extraction = extract.extract_file(path)
    classes = [n for n in extraction.nodes if n["kind"] == NodeKind.CLASS.value]
    assert classes == []
