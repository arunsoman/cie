"""In-memory symbol index (moved from forge/tools.py's LocalDiskBackend,
2026-08-07 — see cie.tools.heuristic's module docstring for why).

Pure AST/regex indexing, no database or network. Python via `ast` (exact);
JS/TS/Java via regex (heuristic, confidence INFERRED when surfaced).
"""

from __future__ import annotations

import ast
import re
from dataclasses import dataclass
from pathlib import Path

#: Suffixes SymbolIndex.build() walks the project tree for. One list, not
#: independently-hardcoded copies (that already drifted once — see
#: forge/tools.py's git history / this module's own move) — a fourth stack
#: (Go) will only need one edit here.
INDEXABLE_SUFFIXES = (".py", ".ts", ".tsx", ".js", ".jsx", ".java")

#: Shared response cap for every heuristic query in cie.tools.heuristic —
#: lives here (not there) since it's a property of "how much of the index
#: to walk," the same reasoning INDEXABLE_SUFFIXES already lives here.
LIST_CAP = 30


@dataclass
class Symbol:
    name: str
    kind: str            # function | class | method
    signature: str
    file: str
    start: int
    end: int
    doc: str = ""

    def as_hit(self, confidence: str = "EXTRACTED") -> dict:
        return {"name": self.name, "kind": self.kind, "signature": self.signature,
                "source_file": self.file, "line_range": [self.start, self.end],
                "confidence": confidence}


class SymbolIndex:
    """Python via ast (exact); JS/TS/Java via regex (heuristic, INFERRED)."""

    TS_PATTERNS = [
        ("function", re.compile(r"(?:export\s+)?(?:async\s+)?function\s+(\w+)\s*\(([^)]*)\)")),
        ("class",    re.compile(r"(?:export\s+)?class\s+(\w+)")),
        ("function", re.compile(r"(?:export\s+)?const\s+(\w+)\s*=\s*(?:async\s*)?\(([^)]*)\)\s*=>")),
        ("method",   re.compile(r"^\s+(?:async\s+)?(\w+)\s*\(([^)]*)\)\s*[:{]")),
    ]

    JAVA_PATTERNS = [
        ("class", re.compile(r"^\s*(?:@\w+(?:\([^)]*\))?\s*)*"
                              r"(?:public|private|protected)?\s*(?:static\s+)?(?:final\s+)?"
                              r"(?:abstract\s+)?class\s+(\w+)")),
        ("method", re.compile(r"^\s*(?:@\w+(?:\([^)]*\))?\s*)*"
                               r"(?:public|private|protected)\s+(?:static\s+)?(?:final\s+)?"
                               r"[\w<>\[\],\s]+?\s+(\w+)\s*\(([^)]*)\)\s*(?:throws\s+[\w,\s]+)?\s*\{")),
    ]

    def __init__(self, project_dir: Path):
        self.project_dir = Path(project_dir)
        self.symbols: list[Symbol] = []
        self.build()

    def _parsers(self) -> dict:
        return {
            ".py": self._parse_py, ".ts": self._parse_ts, ".tsx": self._parse_ts,
            ".js": self._parse_ts, ".jsx": self._parse_ts, ".java": self._parse_java,
        }

    def build(self) -> None:
        self.symbols = []
        parsers = self._parsers()
        for ext in INDEXABLE_SUFFIXES:
            parser = parsers[ext]
            for f in sorted(self.project_dir.rglob(f"*{ext}")):
                if any(part.startswith(".") or part in ("node_modules", "__pycache__", ".venv")
                       for part in f.parts):
                    continue
                rel = str(f.relative_to(self.project_dir))
                try:
                    parser(rel, f.read_text(errors="replace"))
                except Exception:
                    continue  # unparseable file: index skips it, lint gate will catch it

    def reindex_file(self, rel: str, content: str) -> None:
        """Incrementally refresh one file's symbols in place — a write-path
        counterpart to `build()`'s full rescan, so a caller keeping this
        index alive across writes (see `cie.tools.ToolService`'s own write
        hooks) doesn't have to pay for a full project rebuild per file
        change. Drops the file's stale symbols first regardless of whether
        the re-parse succeeds, so a file that became unparseable (or was
        deleted — pass `content=""`) doesn't leave orphaned entries behind.
        """
        self.symbols = [s for s in self.symbols if s.file != rel]
        parser = self._parsers().get(Path(rel).suffix)
        if parser is None or not content:
            return
        try:
            parser(rel, content)
        except Exception:
            pass

    # -- python (exact) -------------------------------------------------
    def _parse_py(self, rel: str, text: str) -> None:
        tree = ast.parse(text)

        def sig_of(node) -> str:
            try:
                args = ast.unparse(node.args)
                ret = f" -> {ast.unparse(node.returns)}" if node.returns else ""
                prefix = "async def" if isinstance(node, ast.AsyncFunctionDef) else "def"
                return f"{prefix} {node.name}({args}){ret}"
            except Exception:
                return f"def {node.name}(...)"

        def doc_of(node) -> str:
            d = ast.get_docstring(node) or ""
            return d.splitlines()[0] if d else ""

        for node in ast.walk(tree):
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                kind = "method"
                # methods: parents tracked below
                self.symbols.append(Symbol(node.name, kind, sig_of(node), rel,
                                           node.lineno, node.end_lineno or node.lineno, doc_of(node)))
            elif isinstance(node, ast.ClassDef):
                self.symbols.append(Symbol(node.name, "class", f"class {node.name}", rel,
                                           node.lineno, node.end_lineno or node.lineno, doc_of(node)))
        # mark module-level functions correctly (walk lacks parents)
        for node in tree.body:
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                for s in self.symbols:
                    if s.name == node.name and s.file == rel and s.start == node.lineno:
                        s.kind = "function"

    # -- ts/js (heuristic) -----------------------------------------------
    def _parse_ts(self, rel: str, text: str) -> None:
        for i, line in enumerate(text.splitlines(), start=1):
            for kind, pat in self.TS_PATTERNS:
                m = pat.search(line)
                if m:
                    name = m.group(1)
                    params = m.group(2) if m.lastindex and m.lastindex >= 2 else ""
                    sig = f"{kind} {name}({params})" if kind != "class" else f"class {name}"
                    self.symbols.append(Symbol(name, kind, sig, rel, i, i))
                    break

    # -- java (heuristic, analogous to _parse_ts — not real AST parsing) --
    def _parse_java(self, rel: str, text: str) -> None:
        for i, line in enumerate(text.splitlines(), start=1):
            for kind, pat in self.JAVA_PATTERNS:
                m = pat.search(line)
                if m:
                    name = m.group(1)
                    params = m.group(2) if kind == "method" else ""
                    sig = f"class {name}" if kind == "class" else f"{name}({params})"
                    self.symbols.append(Symbol(name, kind, sig, rel, i, i))
                    break

    # -- queries ----------------------------------------------------------
    def find(self, name: str, kind: str = "") -> list[Symbol]:
        exact = [s for s in self.symbols if s.name == name and (not kind or s.kind == kind)]
        if exact:
            return exact
        return [s for s in self.symbols if name.lower() in s.name.lower() and (not kind or s.kind == kind)]

    def in_file(self, rel: str) -> list[Symbol]:
        return [s for s in self.symbols if s.file == rel]
