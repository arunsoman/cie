"""A minimal, from-scratch LanguageAdapter for a fictional ".toy" language —
no tree-sitter grammar, no LSP, just a regex over the file's own syntax.

This is the worked example `docs/adding-a-language.md` walks through. It's
deliberately toy-simple (one node kind, one relation, regex instead of a
real parser) so the *shape* of a LanguageAdapter is obvious without also
teaching a real grammar — the same registry call works whether the real
implementation behind it is a regex, a hand-rolled recursive-descent
parser, or (as with the real `nirdosha` adapter this pattern is proven on)
a wrapper around another compiler's own `emit-ast` output.

Run it for real (needs `cie` importable — either `pip install -e .` from
the repo root, or `PYTHONPATH=. python examples/adapters/toy_regex_adapter.py`):

    PYTHONPATH=. python examples/adapters/toy_regex_adapter.py

registers itself, indexes `examples/adapters/hello.toy` (generated on
first run), and prints the Extraction it produced — verified output is
in the module docstring at the bottom of this file, not just claimed.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path

from cie.lang_adapter import LanguageAdapter, register_adapter, get_adapter_for

# A ".toy" program is a flat list of `fn NAME(ARGS):` definitions, one per
# line, body ignored — enough syntax to have real functions to find, not
# enough to need an actual parser.
_FN_RE = re.compile(r"^fn\s+(\w+)\s*\(([^)]*)\)\s*:\s*$", re.MULTILINE)


@dataclass
class Extraction:
    """Minimal stand-in matching cie.extract.Extraction's shape — nodes +
    edges as plain dicts, same schema the real extraction layer emits."""

    nodes: list[dict] = field(default_factory=list)
    edges: list[dict] = field(default_factory=list)
    imports: list[dict] = field(default_factory=list)
    call_sites: list[dict] = field(default_factory=list)
    instance_bindings: list[dict] = field(default_factory=list)
    class_bases: list[dict] = field(default_factory=list)


class ToyRegexAdapter:
    """Implements cie.lang_adapter.LanguageAdapter for `.toy` files."""

    def supported_suffixes(self) -> set[str]:
        return {".toy"}

    def extract_file(self, path: Path) -> Extraction:
        text = path.read_text(encoding="utf-8")
        file_id = f"file::{path.name}"
        out = Extraction()
        out.nodes.append({
            "id": file_id,
            "label": path.name,
            "source_file": str(path),
            "source_location": "L1",
            "file_type": "code",
            "kind": "FILE",
            "signature": "",
            "line_start": 1,
            "line_end": text.count("\n") + 1,
            "docstring": "",
            "decorators": "[]",
        })
        for m in _FN_RE.finditer(text):
            name, params = m.group(1), m.group(2)
            line = text[: m.start()].count("\n") + 1
            node_id = f"{file_id}::{name}@{line}"
            out.nodes.append({
                "id": node_id,
                "label": name,
                "source_file": str(path),
                "source_location": f"L{line}",
                "file_type": "code",
                "kind": "FUNC",
                "signature": f"fn {name}({params}):",
                "line_start": line,
                "line_end": line,
                "docstring": "",
                "decorators": "[]",
            })
            out.edges.append({
                "source": file_id,
                "target": node_id,
                "relation": "defines",
                "confidence": "extracted",
            })
        return out


if __name__ == "__main__":
    register_adapter(ToyRegexAdapter())
    assert get_adapter_for(".toy") is not None, "registration failed"

    fixture = Path(__file__).with_name("hello.toy")
    fixture.write_text("fn greet(name):\n    pass\n\nfn main():\n    pass\n")

    adapter = get_adapter_for(".toy")
    extraction = adapter.extract_file(fixture)
    print(f"{len(extraction.nodes)} nodes, {len(extraction.edges)} edges")
    for n in extraction.nodes:
        print(f"  {n['kind']:6s} {n['label']:10s} {n['signature']}")

# Verified real output (29 Aug 2026, `python examples/adapters/toy_regex_adapter.py`):
#
#   3 nodes, 2 edges
#     FILE   hello.toy
#     FUNC   greet      fn greet(name):
#     FUNC   main       fn main():
#
# No tree-sitter grammar, no LSP, no change to any file under cie/ — the
# whole extension surface is `register_adapter(ToyRegexAdapter())` plus
# whatever parsing logic the adapter itself wants to own.
