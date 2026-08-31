"""Per-project AST mirror — the in-process source of truth behind the
``get_meta`` and ``get_function`` tools.

The contract (what "cie is the single source" means here):

1. **Reads never touch the filesystem** — after a file's first ingestion,
   `get_meta`/`get_function` serve ONLY from this mirror: file type, line
   count, function signatures, and function bodies all come from the
   parsed AST snapshot, never a disk read. A `view_file` that races a
   foreign writer can serve different bytes than `get_function`; these
   two tools cannot, because they share one snapshot.

2. **Every cie write path refreshes the mirror in the SAME call, only
   after the filesystem write has landed** — `write_file`/
   `write_files_atomic`/`edit_file`/`apply_patch` all funnel through
   `_sync_graph_after_write` (apply_patch only reaches that helper AFTER
   its atomic write + post-write integrity re-hash, so a rolled-back
   patch never touches the mirror), `delete_file` drops the entry, and
   `reindex_file`/`sync_ast_delta` re-ingest explicitly. The mirror can
   therefore never hold content the filesystem never accepted — the
   file system, the graph, and the AST move as one unit per call.

3. **External writers are not auto-detected, by design.** A change made
   outside cie (a human editing on disk, another agent, a git merge)
   leaves the mirror serving cie's view until `reindex_file` /
   `sync_ast_delta` explicitly refreshes it. That is the price of reads
   that never stat or re-read the file — and the flip side is the
   guarantee above: no tool can ever see a half-applied write. Code that
   bypasses `ToolService` and calls `cie.tools.edit`/`cie.tools.view`
   module functions directly also bypasses this mirror; the ToolService
   methods are the contract.

Parsing is Python-AST-exact (``ast.parse``, tier ``"python-ast"``):
signatures, line spans (decorators included), nested definitions, and
docstrings come off the real tree. Every other suffix is ingested as
tier ``"text"`` — file type and line count are served, the function table
is honestly empty with a hint pointing at `file_skeleton`'s heuristic
tier, rather than a regex approximation pretending to be an AST. A file
that does not parse (mid-edit syntax error) keeps its line count and
type with a `parse_error` hint — the mirror degrades, it never lies.
"""
from __future__ import annotations

import ast
import threading
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

#: Default `get_function` window, in lines of the FUNCTION (not the
#: file) — same 100-line budget as `view_file`'s window. A 3,000-line
#: function therefore returns its first 100 lines plus its nested-
#: definition map and a paging hint, never a 3,000-line dump: the nested
#: map is how an agent navigates a huge function without reading all of
#: it, the window bounds are how it reads exactly the slice it needs.
WINDOW = 100

#: Max signatures in one `get_meta` envelope (LIST_CAP-style truncation
#: contract: honest totals + truncated flag, never a silent cut).
META_CAP = 200

#: Max nested-definition rows in one `get_function` envelope.
NESTED_CAP = 100

#: suffix -> file type. Deliberately a flat map, not a language registry:
#: this is a display field for `get_meta`, while the parse tier is
#: decided by _PY_SUFFIXES alone (everything else is honest "text").
_FILE_TYPES = {
    ".py": "python", ".pyw": "python", ".pyi": "python",
    ".js": "javascript", ".mjs": "javascript", ".cjs": "javascript",
    ".jsx": "jsx", ".ts": "typescript", ".tsx": "typescript",
    ".java": "java", ".go": "go", ".rs": "rust", ".zig": "zig",
    ".c": "c", ".h": "c", ".cpp": "cpp", ".cc": "cpp", ".hpp": "cpp",
    ".cs": "csharp", ".kt": "kotlin", ".swift": "swift",
    ".rb": "ruby", ".php": "php",
    ".md": "markdown", ".rst": "rst", ".txt": "text", ".json": "json",
    ".toml": "toml", ".yaml": "yaml", ".yml": "yaml", ".ini": "ini",
    ".cfg": "ini", ".html": "html", ".css": "css", ".sql": "sql",
    ".sh": "shell", ".bash": "shell", ".dockerfile": "dockerfile",
    ".proto": "protobuf",
}

_PY_SUFFIXES = {".py", ".pyw", ".pyi"}


def file_type_for(suffix: str) -> str:
    """Suffix -> the display file type `get_meta` reports — shared with
    `cie.file_index`'s navigation payloads so `ls` and `get_meta` can
    never disagree about what kind of file something is."""
    return _FILE_TYPES.get((suffix or "").lower(), "text")


class AstLookupError(Exception):
    """`get_function` could not resolve the requested signature — carries
    the SPEC §0 error `kind` (``not_found``/``ambiguous``/``validation``)
    and the actionable hint separately from the message, so the
    ToolService wrapper can build a proper error envelope."""

    def __init__(self, kind: str, message: str, hint: Optional[str] = None):
        super().__init__(message)
        self.kind = kind
        self.hint = hint


@dataclass(frozen=True)
class FunctionEntry:
    """One function/class definition, taken straight off the AST."""
    signature: str            # e.g. "def charge(amount: float) -> str:" (trailing ":")
    name: str                 # bare name
    qualname: str             # "Class.method" / "outer.inner" / "name"
    kind: str                 # "function" | "method" | "class"
    start_line: int           # 1-based, INCLUDING decorator lines
    end_line: int             # inclusive
    doc: Optional[str] = None  # first line of the docstring, if any
    nested: tuple = field(default_factory=tuple)  # nested FunctionEntry, absolute lines


@dataclass
class FileAst:
    """One file's parsed snapshot — immutable once stored."""
    path: str                       # project-relative, posix separators
    file_type: str                  # "python", "markdown", "text", ...
    suffix: str
    tier: str                       # "python-ast" | "text"
    total_lines: int
    lines: tuple
    functions: tuple                # top-level FunctionEntry, source order
    parse_error: Optional[str] = None


# ---------------------------------------------------------------------------
# AST -> function table (Python-exact)
# ---------------------------------------------------------------------------

def _doc_first_line(node: ast.AST) -> Optional[str]:
    doc = ast.get_docstring(node, clean=True)
    if not doc:
        return None
    return doc.splitlines()[0][:160]


def _arg_text(arg: ast.arg) -> str:
    if arg.annotation is not None:
        return f"{arg.arg}: {ast.unparse(arg.annotation)}"
    return arg.arg


def _signature_text(node: ast.FunctionDef | ast.AsyncFunctionDef) -> str:
    """Reconstruct a copy-pasteable signature from the AST node — the
    same string `get_function` later accepts back as its `signature`
    argument (normalization on both sides makes the round trip exact)."""
    a = node.args
    parts: list[str] = []
    positional = list(a.posonlyargs) + list(a.args)
    defaults: list[ast.expr] = list(a.defaults)
    defaults = [None] * (len(positional) - len(defaults)) + defaults
    pending_slash = bool(a.posonlyargs)
    for arg, default in zip(positional, defaults):
        parts.append(_arg_text(arg) + (f" = {ast.unparse(default)}" if default is not None else ""))
        if pending_slash and arg is a.posonlyargs[-1]:
            parts.append("/")
            pending_slash = False
    if a.vararg is not None:
        parts.append(f"*{_arg_text(a.vararg)}")
    elif a.kwonlyargs:
        parts.append("*")
    for arg, default in zip(a.kwonlyargs, a.kw_defaults):
        parts.append(_arg_text(arg) + (f" = {ast.unparse(default)}" if default is not None else ""))
    if a.kwarg is not None:
        parts.append(f"**{_arg_text(a.kwarg)}")
    prefix = "async def " if isinstance(node, ast.AsyncFunctionDef) else "def "
    returns = f" -> {ast.unparse(node.returns)}" if node.returns is not None else ""
    return f"{prefix}{node.name}({', '.join(parts)}){returns}:"


def _decorated_start(node: ast.AST) -> int:
    decorators = getattr(node, "decorator_list", [])
    starts = [d.lineno for d in decorators if getattr(d, "lineno", None)]
    starts.append(node.lineno)  # type: ignore[attr-defined]
    return min(starts)


def _nested_entries(body: list[ast.stmt], prefix: str, in_class: bool,
                    out: list) -> None:
    """Every function/class defined INSIDE `body`, recursively — including
    defs hidden behind non-def statements (`if TYPE_CHECKING:`, try/except
    fallbacks, platform branches), which a naive `tree.body` scan misses."""
    for stmt in body:
        if isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef)):
            out.append(_func_entry(stmt, prefix, in_class))
            continue  # def/class bodies are visited by their own entries —
        if isinstance(stmt, ast.ClassDef):  # a generic body recursion here
            out.append(_class_entry(stmt, prefix))  # would double-report every
            continue  # method at the outer level too (confirmed by the
        # nested-map test: 'giant.Inner.twice' AND a bogus 'giant.twice')
        for attr in ("body", "orelse", "finalbody"):
            inner = getattr(stmt, attr, None)
            if isinstance(inner, list):
                _nested_entries(inner, prefix, in_class, out)
        for handler in getattr(stmt, "handlers", []) or []:
            if isinstance(getattr(handler, "body", None), list):
                _nested_entries(handler.body, prefix, in_class, out)
        for case in getattr(stmt, "cases", []) or []:
            if isinstance(getattr(case, "body", None), list):
                _nested_entries(case.body, prefix, in_class, out)


def _func_entry(node: ast.FunctionDef | ast.AsyncFunctionDef,
                prefix: str, in_class: bool) -> FunctionEntry:
    nested: list[FunctionEntry] = []
    _nested_entries(node.body, f"{prefix}{node.name}.",
                    in_class=False, out=nested)
    return FunctionEntry(
        signature=_signature_text(node),
        name=node.name,
        qualname=f"{prefix}{node.name}",
        kind="method" if in_class else "function",
        start_line=_decorated_start(node),
        end_line=node.end_lineno or node.lineno,  # type: ignore[attr-defined]
        doc=_doc_first_line(node),
        nested=tuple(nested),
    )


def _class_entry(node: ast.ClassDef, prefix: str) -> FunctionEntry:
    bases = ", ".join(ast.unparse(b) for b in node.bases)
    bases += "".join(f", {ast.unparse(k)}={ast.unparse(v)}" for k, v in node.keywords)
    nested: list[FunctionEntry] = []
    _nested_entries(node.body, f"{prefix}{node.name}.", in_class=True, out=nested)
    return FunctionEntry(
        signature=f"class {node.name}({bases}):" if bases else f"class {node.name}:",
        name=node.name,
        qualname=f"{prefix}{node.name}",
        kind="class",
        start_line=_decorated_start(node),
        end_line=node.end_lineno or node.lineno,
        doc=_doc_first_line(node),
        nested=tuple(nested),
    )


def _parse_python(text: str) -> tuple[list[FunctionEntry], Optional[str]]:
    """AST-parse Python source into the function table. Returns
    `([], parse_error)` on a syntax error — the caller keeps line/type
    data and serves the honest `parse_error` hint instead of guessing."""
    try:
        tree = ast.parse(text)
    except SyntaxError as exc:
        return [], f"{exc.msg or 'invalid syntax'} (line {exc.lineno})"
    functions: list[FunctionEntry] = []
    _nested_entries(tree.body, prefix="", in_class=False, out=functions)
    return functions, None


def _ingest_text(path: str, suffix: str, text: str) -> FileAst:
    lines = tuple(text.splitlines())
    file_type = _FILE_TYPES.get(suffix.lower(), "text" if suffix else "text")
    if suffix.lower() in _PY_SUFFIXES:
        functions, parse_error = _parse_python(text)
        return FileAst(path=path, file_type=file_type, suffix=suffix,
                       tier="python-ast", total_lines=len(lines), lines=lines,
                       functions=tuple(functions), parse_error=parse_error)
    return FileAst(path=path, file_type=file_type, suffix=suffix,
                   tier="text", total_lines=len(lines), lines=lines,
                   functions=())


# ---------------------------------------------------------------------------
# The per-project store
# ---------------------------------------------------------------------------

class _ProjectMirror:
    """One project root's mirror. Entries are keyed by project-relative
    posix path, so the same file reached via different absolute roots
    still has exactly one entry per project."""

    def __init__(self, root: Path):
        self.root = root
        self._entries: dict[str, FileAst] = {}
        self._lock = threading.RLock()

    def _rel(self, resolved: Path) -> str:
        try:
            return resolved.relative_to(self.root).as_posix()
        except ValueError:
            return resolved.name

    def get(self, resolved: Path) -> Optional[FileAst]:
        with self._lock:
            return self._entries.get(self._rel(resolved))

    def store(self, entry: FileAst) -> None:
        with self._lock:
            self._entries[entry.path] = entry

    def drop(self, resolved: Path) -> None:
        with self._lock:
            self._entries.pop(self._rel(resolved), None)


_STORES: dict[str, _ProjectMirror] = {}
_STORES_LOCK = threading.Lock()


def _mirror_for(root: Path) -> _ProjectMirror:
    key = str(Path(root).resolve())
    with _STORES_LOCK:
        mirror = _STORES.get(key)
        if mirror is None:
            mirror = _ProjectMirror(Path(key))
            _STORES[key] = mirror
        return mirror


# ---------------------------------------------------------------------------
# Write-path hooks (never raise — the write already landed on disk)
# ---------------------------------------------------------------------------

def sync_path(root: Path, resolved: Path, max_bytes: int) -> Optional[str]:
    """Write-path hook: refresh (or drop, if the file is gone) the mirror
    for one just-written file. Never raises and never fails the write —
    a mirror hiccup degrades to a hint, exactly like the graph sync it
    sits next to. Returns that hint, or None."""
    try:
        if not resolved.is_file():
            _mirror_for(root).drop(resolved)
            return None
        raw = resolved.read_bytes()
        return ingest_bytes(root, resolved, raw, max_bytes)
    except OSError as exc:
        return f"ast mirror not refreshed: {exc}"


def ingest_bytes(root: Path, resolved: Path, raw: bytes,
                 max_bytes: int) -> Optional[str]:
    """Ingest content the caller already holds (e.g. reindex_file's own
    read) — one read feeds graph AND mirror. Never raises."""
    try:
        if len(raw) > max_bytes:
            return (f"ast mirror skipped: file over the {max_bytes}-byte "
                    "ceiling; get_meta/get_function will re-ingest lazily "
                    "or refuse oversized files")
        mirror = _mirror_for(root)
        rel = mirror._rel(resolved)  # noqa: SLF001 - same-package mirror
        mirror.store(_ingest_text(rel, resolved.suffix, raw.decode(errors="replace")))
        return None
    except Exception as exc:  # noqa: BLE001 - a write path never fails on the mirror
        return f"ast mirror not refreshed: {exc}"


def drop(root: Path, resolved: Path) -> None:
    """Write-path hook for delete_file: the mirror must forget the file
    in the same call that unlinks it."""
    _mirror_for(root).drop(resolved)


def lookup_or_ingest(root: Path, resolved: Path, max_bytes: int) -> FileAst:
    """READ-path entry: serve from the mirror; ingest from disk exactly
    ONCE on a first-ever miss (a file no cie write path has touched).
    After that the entry lives and dies by the write-path hooks — this
    function is the only place a read can touch the file, and only when
    it has never been seen.

    Raises FileNotFoundError (missing/oversized check first) / ValueError
    (over ceiling) — the ToolService wrapper converts via `_guard`."""
    mirror = _mirror_for(root)
    entry = mirror.get(resolved)
    if entry is not None:
        return entry
    if not resolved.is_file():
        raise FileNotFoundError(f"no such file under project root: {resolved.name}")
    size = resolved.stat().st_size
    if size > max_bytes:
        raise ValueError(
            f"{resolved.name!r} is {size} bytes, over the {max_bytes}-byte "
            "ceiling; get_meta/get_function are for source files, not "
            "large binaries/datasets"
        )
    entry = _ingest_text(mirror._rel(resolved), resolved.suffix,  # noqa: SLF001
                         resolved.read_text(errors="replace"))
    mirror.store(entry)
    return entry


# ---------------------------------------------------------------------------
# Payload shaping (pure — operates on a FileAst, never the filesystem)
# ---------------------------------------------------------------------------

def _normalize(signature: str) -> str:
    """Whitespace-insensitive, colon-insensitive comparison key —
    `get_function` accepts whatever `get_meta` printed, a bare `name`,
    or a hand-typed `name(args)` variant."""
    return "".join(signature.split()).rstrip(":")


def _descendants(entry: FunctionEntry) -> list[FunctionEntry]:
    """Every definition nested at ANY depth inside `entry` — the flat
    navigation map a huge function needs (a 3,000-line function's inner
    defs live three levels deep; a direct-children list would hide them).
    Source order: ast walks bodies in order and entries were appended in
    encounter order, so recursion preserves it."""
    out: list[FunctionEntry] = []
    stack = list(reversed(entry.nested))
    while stack:
        fn = stack.pop()
        out.append(fn)
        stack.extend(reversed(fn.nested))
    return out


def _find_function(entry: FileAst, signature: str) -> FunctionEntry:
    """Resolve `signature` to exactly one function, matching ladder:
    full normalized signature, then qualified name, then a unique bare
    name — ambiguity is an error listing candidates, never a guess."""
    wanted = _normalize(signature)
    if not wanted:
        raise AstLookupError("validation", "signature must not be empty")
    flat: list[FunctionEntry] = []
    for fn in entry.functions:
        flat.append(fn)
        flat.extend(_descendants(fn))
    for fn in flat:
        if _normalize(fn.signature) == wanted:
            return fn
    for fn in flat:
        if _normalize(fn.qualname) == wanted:
            return fn
    by_name = [fn for fn in flat if _normalize(fn.name) == wanted]
    if len(by_name) == 1:
        return by_name[0]
    if len(by_name) > 1:
        candidates = "\n".join(
            f"  - {fn.qualname}: {fn.signature} (lines {fn.start_line}-{fn.end_line})"
            for fn in by_name[:10]
        )
        raise AstLookupError(
            "ambiguous",
            f"'{signature}' matches {len(by_name)} definitions in this file",
            hint=f"disambiguate with the full signature or qualified name:\n{candidates}",
        )
    sample = "\n".join(
        f"  - {fn.qualname}: {fn.signature} (lines {fn.start_line}-{fn.end_line})"
        for fn in flat[:10]
    )
    raise AstLookupError(
        "not_found", f"no function matching '{signature}' in {entry.path}",
        hint=f"get_meta lists every signature; candidates include:\n{sample}"
             if flat else
             "no function/class definitions are parsed for this file — "
             "call get_meta to confirm (non-Python files carry no AST table)",
    )


def meta_payload(entry: FileAst) -> tuple[dict, bool, Optional[str]]:
    """`get_meta`'s result row: file type, total lines, and the file's
    function signatures — every field straight off the AST snapshot."""
    functions = [
        {
            "signature": fn.signature, "qualname": fn.qualname,
            "kind": fn.kind, "start_line": fn.start_line,
            "end_line": fn.end_line, "doc": fn.doc,
        }
        for fn in entry.functions[:META_CAP]
    ]
    truncated = len(entry.functions) > META_CAP
    hint: Optional[str] = None
    if entry.parse_error:
        hint = (f"file does not parse ({entry.parse_error}); signatures "
                "unavailable — total_lines/file_type still served from the mirror")
    elif entry.tier == "text":
        hint = ("function signatures are AST-exact for Python only; other "
                "languages serve file_type/total_lines here — use "
                "file_skeleton for the heuristic symbol tier")
    elif not entry.functions:
        hint = "no function/class definitions found in this file"
    elif truncated:
        hint = (f"{META_CAP} of {len(entry.functions)} signatures shown; "
                "call get_function on any of them for its body")
    payload = {
        "path": entry.path, "file_type": entry.file_type, "suffix": entry.suffix,
        "tier": entry.tier, "total_lines": entry.total_lines,
        "functions": functions, "function_count": len(entry.functions),
    }
    return payload, truncated, hint


def function_payload(entry: FileAst, signature: str,
                     start: int = 1, end: int = 0) -> tuple[dict, bool, Optional[str]]:
    """`get_function`'s result row: the function's ACTUAL content, sliced
    from the AST snapshot's line tuple — never a disk read.

    The 3,000-line-function answer: `start`/`end` are FUNCTION-RELATIVE
    (1-based, inclusive; `end=0` means WINDOW lines from `start`), so
    any slice is addressable; the payload always carries the function's
    full span, length, and nested-definition map, so a huge function is
    navigated (nested map -> narrow window) instead of dumped.
    `content` lines keep `view_file`'s exact `{n:>5}\\t{line}` format
    with ABSOLUTE file line numbers, so windows from the two tools can be
    cross-referenced.

    Raises AstLookupError (not_found/ambiguous/validation) — the
    ToolService wrapper converts it into a SPEC §0 error envelope."""
    fn = _find_function(entry, signature)
    descendants = _descendants(fn)
    total = fn.end_line - fn.start_line + 1
    if start < 1:
        raise AstLookupError("validation", f"start must be >= 1, got {start}")
    if start > total:
        raise AstLookupError(
            "validation", f"start={start} is past the function's last line ({total})",
            hint=f"'{fn.qualname}' spans {total} function lines; use start=1..{total}",
        )
    wstart = start
    wend = end if end >= wstart else min(wstart + WINDOW - 1, total)
    wend = min(wend, total)
    content = "\n".join(
        f"{n:>5}\t{entry.lines[n - 1]}"
        for n in range(fn.start_line + wstart - 1, fn.start_line + wend)
    )
    nested_rows = [
        {"signature": n.signature, "qualname": n.qualname, "kind": n.kind,
         "start_line": n.start_line, "end_line": n.end_line, "doc": n.doc}
        for n in descendants[:NESTED_CAP]
    ]
    truncated = wend < total
    hint: Optional[str] = None
    if truncated:
        hint = (f"window {wstart}-{wend} of {total} function lines; call "
                f"get_function with start={wend + 1} for the next window")
    if descendants and len(nested_rows) < len(descendants):
        nested_note = f"nested map truncated to {NESTED_CAP} of {len(descendants)}"
        hint = f"{hint} — {nested_note}" if hint else nested_note
    payload = {
        "path": entry.path, "signature": fn.signature, "qualname": fn.qualname,
        "kind": fn.kind, "doc": fn.doc,
        "start_line": fn.start_line, "end_line": fn.end_line,
        "total_lines": total,
        "window": {"start": wstart, "end": wend},
        "content": content,
        "nested": nested_rows,
        "nested_count": len(descendants),
    }
    return payload, truncated, hint