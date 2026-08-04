"""Tree-sitter source extractor.

Parses Python, JavaScript, and TypeScript source files with tree-sitter and
emits a cie-compatible extraction dict: a set of nodes (files, classes,
functions, methods) and edges (contains, defines, calls). Each function and
method node carries a `signature` string so the querying engine can answer
"what is the signature of method X" without re-reading the file.

During the SAME parse the extractor also records the inputs for the pass-2
call-graph resolver (see cie.callgraph):

- every symbol node gains `line_start`/`line_end` (1-based, inclusive) and a
  `docstring` (first line of the Python docstring or leading ``/** */`` jsdoc
  comment, else ""),
- `imports`: ImportRecord-shaped dicts for Python ``import``/``from...import``
  (relative dots preserved) and JS/TS ``import`` statements plus ``require()``
  assignments,
- `call_sites`: CallSite-shaped dicts for Python ``call`` and JS/TS
  ``call_expression`` nodes, with the enclosing function/method node id as
  `caller_id` (the file hub id for top-level calls).

The extractor is pure: it takes a path + bytes and returns a dict. It has no
database or filesystem side effects, so it is straightforward to unit-test.
"""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Callable, Optional

from cie.models import CallSite, Confidence, ImportRecord, NodeKind


# File suffix -> tree-sitter language loader. Each loader is imported lazily
# so that a missing grammar wheel does not break importing this module.
_LANG_LOADERS = {
    ".py": "tree_sitter_python.language",
    ".js": "tree_sitter_javascript.language",
    ".jsx": "tree_sitter_javascript.language",
    ".mjs": "tree_sitter_javascript.language",
    ".cjs": "tree_sitter_javascript.language",
    ".ts": "tree_sitter_typescript.language_typescript",
    ".tsx": "tree_sitter_typescript.language_tsx",
    ".java": "tree_sitter_java.language",
}


# Node types that represent a class declaration, per language.
_CLASS_TYPES = {"class_definition", "class_declaration"}

# Node types that represent a function or method declaration, per language.
_FUNCTION_TYPES = {
    "function_definition",       # python
    "function_declaration",      # js/ts
    "method_definition",        # js/ts class methods
    "function_expression",       # js anonymous (named via parent assignment)
    "arrow_function",            # js/ts arrow
    "method_declaration",        # java (distinct from js/ts's method_definition)
}

# Node types that hold parameters.
_PARAM_TYPES = {"parameters", "formal_parameters"}

# Node type for a type annotation (ts return/param types).
_TYPE_ANNOTATION = "type_annotation"

# Node types that represent a call expression, per language.
_CALL_TYPES = {"call", "call_expression", "method_invocation"}

# Node types that wrap the callable being invoked (attribute/member access).
_ATTRIBUTE_TYPES = {"attribute", "member_expression"}

# TS/TSX decorator node type (`@Component(...)`), and the wrapper node type
# whose own children hold a class's decorators when the class is exported
# (`export class Foo` / `export default class Foo`) rather than the class
# node itself.
_DECORATOR_TYPE = "decorator"
_EXPORT_WRAPPER_TYPES = {"export_statement"}

# Java annotation node types (`@RestController`, `@RequestMapping(...)`),
# nested inside a `modifiers` node alongside access-modifier keywords.
_JAVA_ANNOTATION_TYPES = {"marker_annotation", "annotation"}


@dataclass
class DecoratorRecord:
    """One `@Name(...)`-shaped marker on a class/method/function: a TS/TSX
    decorator or a Java annotation. Same shape for both — downstream
    grouping/prompts/entity-typing can treat "has `@Component`" and "has
    `@RestController`" identically without caring which language produced
    the record."""

    name: str
    args_text: str
    line: int


@dataclass
class Extraction:
    """The pure output of parsing one or more files.

    `nodes` and `edges` follow the cie extraction schema so they can be
    consumed by the loader and the existing build/query code unchanged.
    `imports` and `call_sites` hold dicts serialized from
    :class:`cie.models.ImportRecord` and
    :class:`cie.models.CallSite`; they feed the pass-2 resolver in
    cie.callgraph. `instance_bindings` holds ``{name, class_name, line}``
    dicts for constructor assignments (``svc = Service()`` /
    ``const svc = new Service()``) that power the resolver's receiver-type
    heuristic.
    """

    nodes: list[dict] = field(default_factory=list)
    edges: list[dict] = field(default_factory=list)
    imports: list[dict] = field(default_factory=list)
    call_sites: list[dict] = field(default_factory=list)
    instance_bindings: list[dict] = field(default_factory=list)

    def merge(self, other: "Extraction") -> None:
        self.nodes.extend(other.nodes)
        self.edges.extend(other.edges)
        self.imports.extend(other.imports)
        self.call_sites.extend(other.call_sites)
        self.instance_bindings.extend(other.instance_bindings)


def supported_suffix(path: Path) -> Optional[str]:
    """Return the matching suffix for a supported language, or None."""
    suffix = path.suffix.lower()
    return suffix if suffix in _LANG_LOADERS else None


def _language_for(suffix: str):
    """Import and return the tree-sitter language capsule for a suffix."""
    module_path, attr = _LANG_LOADERS[suffix].rsplit(".", 1)
    import importlib

    mod = importlib.import_module(module_path)
    return getattr(mod, attr)()


def _parser_for(suffix: str):
    """Build a configured tree-sitter Parser for a supported suffix."""
    from tree_sitter import Language, Parser

    lang = Language(_language_for(suffix))
    return Parser(lang)


def _child_of_type(node, type_name: str):
    for child in node.children:
        if child.type == type_name:
            return child
    return None


def _children_of_type(node, type_name: str):
    return [c for c in node.children if c.type == type_name]


def _first_identifier(node):
    """Return the first identifier child's text, or empty string.

    Fallback only — used when a node has no grammar-exposed `name` field
    (see `_declared_name`), which is the case for JS/TS arrow functions
    and function expressions."""
    for child in node.children:
        if child.type in ("identifier", "property_identifier", "type_identifier"):
            return child.text.decode("utf-8", errors="replace")
    return ""


def _declared_name(node) -> str:
    """Return a class/function/method node's own name, preferring the
    grammar's `name` field over a first-identifier-typed-child scan.

    The scan alone is unsafe for Java: `method_declaration`'s direct
    children are (modifiers, <return type>, name, parameters, body), so
    a scan for the first identifier/type_identifier child finds the
    RETURN TYPE before the method name whenever the return type isn't
    void (confirmed by parsing a real Spring method) — `child_by_field_
    name("name")` sidesteps this and is available on every named
    class/function/method node across Python/JS/TS/Java; only JS/TS
    arrow functions and function expressions expose no `name` field at
    all, which is exactly the scan's fallback case (their name comes
    from the enclosing variable_declarator instead, handled by the
    caller)."""
    name_node = node.child_by_field_name("name")
    if name_node is not None:
        return name_node.text.decode("utf-8", errors="replace")
    return _first_identifier(node)


def _params_text(node) -> str:
    """Return the raw parameter list text, e.g. '(self, x: int)'."""
    for child in node.children:
        if child.type in _PARAM_TYPES:
            return child.text.decode("utf-8", errors="replace").strip()
    return "()"


def _return_type_text(node) -> str:
    """Return the return type annotation text for TS/JS/Python/Java, or
    ''."""
    # TS/JS use 'type_annotation' (e.g. ': string')
    ann = _child_of_type(node, _TYPE_ANNOTATION)
    if ann is not None:
        return ann.text.decode("utf-8", errors="replace").strip().lstrip(": ").strip()
    # Python uses a bare 'type' node after the '->' token
    for child in node.children:
        if child.type == "type":
            return child.text.decode("utf-8", errors="replace").strip()
    # Java exposes the return type via a 'type' FIELD (the node's own
    # .type varies: type_identifier, void_type, integral_type,
    # generic_type, ...) rather than a node literally typed "type" the
    # way Python's is — no collision with JS/TS/Python above, verified
    # none of those expose a 'type' field on a function/method node.
    rt = node.child_by_field_name("type")
    if rt is not None:
        return rt.text.decode("utf-8", errors="replace").strip()
    return ""


def _build_signature(name: str, params: str, return_type: str) -> str:
    """Assemble a human-readable signature string."""
    sig = f"{name}{params}"
    if return_type:
        sig = f"{sig} -> {return_type}"
    return sig


def _location(node) -> str:
    return f"L{node.start_point[0] + 1}"


def _text(node) -> str:
    """Decode a tree-sitter node's source text."""
    return node.text.decode("utf-8", errors="replace")


def _strip_quotes(text: str) -> str:
    """Remove surrounding string quotes (single, double, or triple)."""
    for q in ('"""', "'''", '"', "'"):
        if text.startswith(q) and text.endswith(q) and len(text) >= 2 * len(q):
            return text[len(q):-len(q)]
    return text


def _python_docstring(node) -> str:
    """Return the first line of a Python function/class docstring, or ''."""
    body = None
    for child in node.children:
        if child.type in ("block",):
            body = child
            break
    if body is None or not body.children:
        return ""
    first = body.children[0]
    if first.type != "expression_statement":
        return ""
    string_node = next(
        (c for c in first.children if c.type in ("string", "concatenated_string")),
        None,
    )
    if string_node is None:
        return ""
    raw = _strip_quotes(_text(string_node)).strip()
    return raw.splitlines()[0].strip() if raw else ""


def _jsdoc_comment(node) -> str:
    """Return the first line of a leading ``/** ... */`` jsdoc/Javadoc
    comment, or ''. JS/TS's leading-doc-comment node type is `comment`;
    Java's is `block_comment` (confirmed by parsing a real Javadoc'd
    method — they are NOT the same node type name, so both must be
    accepted or every Java method silently gets an empty docstring)."""
    prev = node.prev_named_sibling
    if prev is None or prev.type not in ("comment", "block_comment"):
        return ""
    raw = _text(prev)
    if not raw.startswith("/**"):
        return ""
    raw = raw[3:]
    if raw.endswith("*/"):
        raw = raw[:-2]
    for line in raw.splitlines():
        cleaned = line.strip().lstrip("*").strip()
        if cleaned:
            return cleaned
    return ""


def _docstring(node, language: str) -> str:
    """Return the first docstring line for a symbol node, per language."""
    if language == "py":
        return _python_docstring(node)
    return _jsdoc_comment(node)


def _ts_decorator_record(node) -> Optional[DecoratorRecord]:
    """Build a DecoratorRecord from a single `decorator` node
    (`@Name` or `@Name(args)`)."""
    inner = next(
        (c for c in node.children if c.type in ("call_expression", "identifier")),
        None,
    )
    if inner is None:
        return None
    if inner.type == "call_expression":
        fn = inner.child_by_field_name("function")
        args = inner.child_by_field_name("arguments")
        name = _text(fn) if fn is not None else ""
        args_text = _text(args) if args is not None else ""
    else:
        name, args_text = _text(inner), ""
    if not name:
        return None
    return DecoratorRecord(name=name, args_text=args_text, line=node.start_point[0] + 1)


def _leading_decorators(node) -> list[DecoratorRecord]:
    """Decorators that are leading children of `node` itself — the shape
    for a non-exported class (`class Foo`) or a field/parameter
    (`@Input() name: string`, `@Inject(TOKEN) x`)."""
    out: list[DecoratorRecord] = []
    for child in node.children:
        if child.type != _DECORATOR_TYPE:
            break
        rec = _ts_decorator_record(child)
        if rec is not None:
            out.append(rec)
    return out


def _preceding_sibling_decorators(node) -> list[DecoratorRecord]:
    """Decorators that are the immediately-preceding run of siblings of
    `node` within its parent — the shape for a class-body method
    (`@HostListener(...) onClick() {}`), where the decorator is NOT
    nested inside the method_definition node."""
    parent = node.parent
    if parent is None:
        return []
    run: list[DecoratorRecord] = []
    for child in parent.children:
        if child.id == node.id:
            return run
        if child.type == _DECORATOR_TYPE:
            rec = _ts_decorator_record(child)
            run = run + [rec] if rec is not None else run
        else:
            run = []
    return []


def _wrapper_decorators(node) -> list[DecoratorRecord]:
    """Decorators that belong to `node` via an enclosing export wrapper
    (`export class Foo` / `export default class Foo`) — these live as
    children of the `export_statement` itself, always preceding its own
    `export`/`default` keyword tokens, not of `node`."""
    parent = node.parent
    if parent is None or parent.type not in _EXPORT_WRAPPER_TYPES:
        return []
    if parent.child_by_field_name("declaration") != node:
        return []
    out: list[DecoratorRecord] = []
    for child in parent.children:
        if child.type == _DECORATOR_TYPE:
            rec = _ts_decorator_record(child)
            if rec is not None:
                out.append(rec)
    return out


def _ts_decorators_for(node) -> list[DecoratorRecord]:
    """All decorator records that apply to `node`, tolerant of tree-sitter-
    typescript's three different attachment shapes (see the three helpers
    above) — each node has decorators attached in exactly one of these
    shapes depending on its context, so check them in order and use
    whichever is non-empty."""
    return (
        _leading_decorators(node)
        or _preceding_sibling_decorators(node)
        or _wrapper_decorators(node)
    )


def _java_annotations_for(node) -> list[DecoratorRecord]:
    """All Java annotation records on `node` (a class/method/field
    declaration) — nested inside a `modifiers` node that's the first
    child of `node`, alongside access-modifier keyword tokens."""
    modifiers = _child_of_type(node, "modifiers")
    if modifiers is None:
        return []
    out: list[DecoratorRecord] = []
    for child in modifiers.children:
        if child.type not in _JAVA_ANNOTATION_TYPES:
            continue
        name_node = child.child_by_field_name("name")
        args_node = child.child_by_field_name("arguments")
        name = _text(name_node) if name_node is not None else ""
        if not name:
            continue
        args_text = _text(args_node) if args_node is not None else ""
        out.append(DecoratorRecord(name=name, args_text=args_text, line=child.start_point[0] + 1))
    return out


def _decorators_for(node, language: str) -> list[DecoratorRecord]:
    """Dispatch to the per-language marker finder. Python has no
    equivalent capability wired up here (scope: TS/TSX decorators and
    Java annotations only, per the brownfield language-support plan)."""
    if language == "java":
        return _java_annotations_for(node)
    if language != "py":
        return _ts_decorators_for(node)
    return []


def _node_lines(node) -> tuple[int, int]:
    """Return (line_start, line_end), 1-based and inclusive."""
    return node.start_point[0] + 1, node.end_point[0] + 1


def _decorators_json(node, language: str) -> str:
    """JSON-serialized list[DecoratorRecord] for a symbol node. Neo4j
    properties must be primitives or arrays of primitives (`SET n = row`
    in neo4j_repository.py::load_extraction rejects nested maps), so this
    is stored as a JSON string rather than a native list-of-dicts;
    consumers `json.loads` it back out. Always present (defaults to the
    empty-list encoding) so the node schema stays uniform regardless of
    language or whether any markers were found."""
    return json.dumps([asdict(d) for d in _decorators_for(node, language)])


def _walk_file(
    tree,
    file_id: str,
    source_path: str,
    file_label: str,
    language: str,
) -> Extraction:
    """Walk a parsed tree and collect class/function/method nodes + edges."""
    out = Extraction()
    root = tree.root_node

    # File hub node.
    out.nodes.append({
        "id": file_id,
        "label": file_label,
        "source_file": source_path,
        "source_location": "L1",
        "file_type": "code",
        "kind": NodeKind.FILE.value,
        "signature": "",
        "line_start": 1,
        "line_end": root.end_point[0] + 1,
        "docstring": "",
        "decorators": "[]",
    })

    def _is_class(n) -> bool:
        return n.type in _CLASS_TYPES

    def _is_function(n) -> bool:
        return n.type in _FUNCTION_TYPES

    def _function_name(n) -> str:
        # arrow / function expression assigned to a variable: the parent
        # variable_declarator -> identifier holds the real name.
        if n.parent is not None and n.parent.type == "variable_declarator":
            name = _declared_name(n.parent)
            if name:
                return name
        return _declared_name(n)

    def _emit_function(n, enclosing_class_id: Optional[str]) -> None:
        name = _function_name(n)
        if not name:
            return
        kind = NodeKind.METHOD.value if enclosing_class_id else NodeKind.FUNC.value
        node_id = f"{file_id}::{name}"
        # disambiguate overloads by start line
        node_id = f"{node_id}@{n.start_point[0] + 1}"
        params = _params_text(n)
        rtype = _return_type_text(n)
        signature = _build_signature(name, params, rtype)
        line_start, line_end = _node_lines(n)
        out.nodes.append({
            "id": node_id,
            "label": name,
            "source_file": source_path,
            "source_location": _location(n),
            "file_type": "code",
            "kind": kind,
            "signature": signature,
            "line_start": line_start,
            "line_end": line_end,
            "docstring": _docstring(n, language),
            "decorators": _decorators_json(n, language),
        })
        if enclosing_class_id:
            out.edges.append({
                "source": enclosing_class_id,
                "target": node_id,
                "relation": "contains",
                "confidence": Confidence.EXTRACTED.value,
            })
        else:
            out.edges.append({
                "source": file_id,
                "target": node_id,
                "relation": "defines",
                "confidence": Confidence.EXTRACTED.value,
            })

    def _emit_class(n) -> str:
        name = _declared_name(n)
        if not name:
            return ""
        node_id = f"{file_id}::{name}"
        line_start, line_end = _node_lines(n)
        out.nodes.append({
            "id": node_id,
            "label": name,
            "source_file": source_path,
            "source_location": _location(n),
            "file_type": "code",
            "kind": NodeKind.CLASS.value,
            "signature": f"class {name}",
            "line_start": line_start,
            "line_end": line_end,
            "docstring": _docstring(n, language),
            "decorators": _decorators_json(n, language),
        })
        out.edges.append({
            "source": file_id,
            "target": node_id,
            "relation": "defines",
            "confidence": Confidence.EXTRACTED.value,
        })
        return node_id

    def _walk(node, enclosing_class_id: Optional[str]) -> None:
        if _is_class(node):
            class_id = _emit_class(node)
            # recurse into the class body with this class as enclosing scope
            for child in node.children:
                _walk(child, class_id)
            return
        if _is_function(node):
            _emit_function(node, enclosing_class_id)
            # still recurse to capture nested functions, but keep the same
            # enclosing class scope (nested functions are not methods).
            for child in node.children:
                _walk(child, enclosing_class_id)
            return
        for child in node.children:
            _walk(child, enclosing_class_id)

    _walk(root, None)
    _collect_imports(root, language, out)
    _collect_call_sites(root, file_id, language, _is_function, _function_name, out)
    return out


def _record_import(out: Extraction, imported_name: str, module: str, kind: str) -> None:
    """Append one ImportRecord dict to the extraction."""
    if not imported_name or not module:
        return
    out.imports.append(asdict(ImportRecord(
        imported_name=imported_name, module=module, kind=kind,
    )))


def _aliased_parts(node) -> tuple[str, str]:
    """Split a Python aliased_import into (module path, usable name)."""
    name_node = node.child_by_field_name("name")
    alias_node = node.child_by_field_name("alias")
    if name_node is None:
        dotted = _child_of_type(node, "dotted_name")
        name_node = dotted if dotted is not None else node
    module = _text(name_node)
    usable = _text(alias_node) if alias_node is not None else module
    return module, usable


def _collect_python_import(node, out: Extraction) -> None:
    """Record bindings from a Python import_statement."""
    for child in node.children:
        if child.type == "dotted_name":
            module = _text(child)
            _record_import(out, module, module, "import")
        elif child.type == "aliased_import":
            module, usable = _aliased_parts(child)
            _record_import(out, usable, module, "import")


def _collect_python_from_import(node, out: Extraction) -> None:
    """Record bindings from a Python import_from_statement (dots preserved)."""
    module = ""
    seen_import_keyword = False
    for child in node.children:
        if child.type == "import":
            seen_import_keyword = True
            continue
        if not seen_import_keyword:
            if child.type == "relative_import":
                # node text keeps the leading dots: '.', '..pkg', etc.
                module = _text(child)
            elif child.type == "dotted_name":
                module = _text(child)
            continue
        if child.type == "dotted_name":
            _record_import(out, _text(child), module, "from_import")
        elif child.type == "aliased_import":
            original, usable = _aliased_parts(child)
            _record_import(out, usable, module or original, "from_import")


def _collect_js_import(node, out: Extraction) -> None:
    """Record bindings from a JS/TS import_statement."""
    source_node = node.child_by_field_name("source")
    if source_node is None:
        source_node = _child_of_type(node, "string")
    if source_node is None:
        return
    module = _strip_quotes(_text(source_node))
    clause = _child_of_type(node, "import_clause")
    if clause is None:
        return  # side-effect import: no usable binding
    for child in clause.children:
        if child.type == "namespace_import":
            name = _first_identifier(child)
            _record_import(out, name, module, "import")
        elif child.type == "named_imports":
            for spec in _children_of_type(child, "import_specifier"):
                name_node = spec.child_by_field_name("name")
                alias_node = spec.child_by_field_name("alias")
                if name_node is None:
                    identifiers = [
                        c for c in spec.children
                        if c.type in ("identifier", "property_identifier")
                    ]
                    if not identifiers:
                        continue
                    name_node = identifiers[0]
                    alias_node = identifiers[1] if len(identifiers) > 1 else None
                usable = _text(alias_node) if alias_node is not None else _text(name_node)
                _record_import(out, usable, module, "from_import")
        elif child.type == "identifier":
            # default import: `import D from './m'`
            _record_import(out, _text(child), module, "from_import")


def _is_require_call(node) -> bool:
    """True for a call_expression of the form ``require('<module>')``."""
    if node.type != "call_expression":
        return False
    fn = node.child_by_field_name("function")
    return fn is not None and fn.type == "identifier" and _text(fn) == "require"


def _require_module(node) -> str:
    """Return the module string of a require(...) call, or ''."""
    args = node.child_by_field_name("arguments")
    if args is None:
        return ""
    string_node = _child_of_type(args, "string")
    return _strip_quotes(_text(string_node)) if string_node is not None else ""


def _collect_js_require(node, out: Extraction) -> None:
    """Record bindings from ``const x = require(...)`` / destructuring."""
    if node.type != "variable_declarator":
        return
    value = node.child_by_field_name("value")
    if value is None or not _is_require_call(value):
        return
    module = _require_module(value)
    if not module:
        return
    name = node.child_by_field_name("name")
    if name is None:
        return
    if name.type == "identifier":
        _record_import(out, _text(name), module, "require")
    elif name.type == "object_pattern":
        for child in name.children:
            if child.type == "shorthand_property_identifier_pattern":
                _record_import(out, _text(child), module, "require")
            elif child.type == "pair_pattern":
                identifiers = [
                    c for c in child.children
                    if c.type in ("identifier", "property_identifier",
                                  "shorthand_property_identifier_pattern")
                ]
                if identifiers:
                    # `f2: g2` -> g2 is the usable local name
                    _record_import(out, _text(identifiers[-1]), module, "require")


def _record_binding(out: Extraction, name: str, class_name: str, line: int) -> None:
    """Append one constructor-assignment binding to the extraction."""
    if not name or not class_name:
        return
    out.instance_bindings.append({
        "name": name, "class_name": class_name, "line": line,
    })


def _collect_python_assignment(node, out: Extraction) -> None:
    """Record ``x = ClassName(...)`` constructor assignments (Python)."""
    if node.type != "assignment":
        return
    left = node.child_by_field_name("left")
    right = node.child_by_field_name("right")
    if left is None or right is None or left.type != "identifier":
        return
    if right.type != "call":
        return
    fn = right.child_by_field_name("function")
    if fn is None or fn.type != "identifier":
        return
    class_name = _text(fn)
    if not class_name[:1].isupper():
        return  # constructor heuristic: classes are Capitalized
    _record_binding(out, _text(left), class_name, node.start_point[0] + 1)


def _collect_js_new_assignment(node, out: Extraction) -> None:
    """Record ``const x = new ClassName(...)`` assignments (JS/TS)."""
    if node.type != "variable_declarator":
        return
    name = node.child_by_field_name("name")
    value = node.child_by_field_name("value")
    if name is None or name.type != "identifier":
        return
    if value is None or value.type != "new_expression":
        return
    ctor = value.child_by_field_name("constructor")
    if ctor is None or ctor.type != "identifier":
        return
    _record_binding(out, _text(name), _text(ctor), node.start_point[0] + 1)


def _collect_java_import(node, out: Extraction) -> None:
    """Record a binding from a Java import_declaration
    (`import com.foo.bar.FooDao;`, optionally `import static ...;`) —
    module is the package prefix, imported_name is the final (class or
    static-member) segment, matching the semantics of Python's
    from-import rather than a bare module import. Wildcard imports
    (`import com.foo.bar.*;`) are a known, rare gap: the trailing `*`
    isn't a real symbol name, so this would (harmlessly) record the
    parent package's last segment as the imported name instead."""
    scoped = _child_of_type(node, "scoped_identifier")
    if scoped is None:
        return
    full = _text(scoped)
    if "." in full:
        module, imported_name = full.rsplit(".", 1)
    else:
        module, imported_name = "", full
    _record_import(out, imported_name, module, "from_import")


def _collect_imports(root, language: str, out: Extraction) -> None:
    """Walk the tree once, recording import bindings and constructor
    assignments (the receiver-type heuristic's instance evidence)."""

    def _visit(node) -> None:
        if language == "py":
            if node.type == "import_statement":
                _collect_python_import(node, out)
            elif node.type == "import_from_statement":
                _collect_python_from_import(node, out)
            elif node.type == "assignment":
                _collect_python_assignment(node, out)
        elif language == "java":
            if node.type == "import_declaration":
                _collect_java_import(node, out)
        else:
            if node.type == "import_statement":
                _collect_js_import(node, out)
            elif node.type == "variable_declarator":
                _collect_js_require(node, out)
                _collect_js_new_assignment(node, out)
        for child in node.children:
            _visit(child)

    _visit(root)


def _call_target(node) -> tuple[str, str]:
    """Split a call node into (called_name, receiver).

    `called_name` is the trailing identifier; `receiver` is the object text
    for attribute/member calls ('' for bare calls). Returns ('', '') when the
    callable is not a simple name or attribute chain.
    """
    if node.type == "method_invocation":
        # Java's call node exposes real `name`/`object` fields directly —
        # no wrapping `function` field and no member-expression/attribute
        # indirection the way JS/TS/Python calls do (confirmed by parsing
        # a real `service.doThing(id)` call). Without this branch the
        # generic fallback below (`node.children[0]`) would grab the
        # RECEIVER (`service`) as the called name instead of `doThing`.
        name = node.child_by_field_name("name")
        if name is None:
            return "", ""
        obj = node.child_by_field_name("object")
        return _text(name), _text(obj) if obj is not None else ""
    fn = node.child_by_field_name("function")
    if fn is None and node.children:
        fn = node.children[0]
    if fn is None:
        return "", ""
    if fn.type in ("identifier", "property_identifier"):
        return _text(fn), ""
    if fn.type in _ATTRIBUTE_TYPES:
        obj = fn.child_by_field_name("object")
        attr = fn.child_by_field_name("attribute")
        if attr is None:
            attr = fn.child_by_field_name("property")
        if attr is None:
            # structural fallback: last identifier-ish child
            ids = [
                c for c in fn.children
                if c.type in ("identifier", "property_identifier")
            ]
            attr = ids[-1] if ids else None
            if obj is None and len(fn.children) > 1:
                obj = fn.children[0]
        if attr is None:
            return "", ""
        receiver = _text(obj) if obj is not None else ""
        return _text(attr), receiver
    return "", ""


def _collect_call_sites(root, file_id: str, language: str,
                        is_function: Callable[[object], bool],
                        function_name: Callable[[object], str],
                        out: Extraction) -> None:
    """Walk the tree once, recording every call site.

    The caller is the nearest enclosing function/method node (mapped to its
    emitted graph node id); top-level calls are attributed to the file hub.
    JS/TS ``require(...)`` calls are import bindings, not call sites.
    """
    seen: set[tuple[str, str, str, int]] = set()

    def _caller_id(node) -> str:
        current = node.parent
        while current is not None:
            if is_function(current):
                name = function_name(current)
                if name:
                    return f"{file_id}::{name}@{current.start_point[0] + 1}"
            current = current.parent
        return file_id

    def _visit(node) -> None:
        if node.type in _CALL_TYPES:
            if language != "py" and _is_require_call(node):
                return
            called_name, receiver = _call_target(node)
            if called_name:
                line = node.start_point[0] + 1
                caller = _caller_id(node)
                key = (caller, called_name, receiver, line)
                if key not in seen:
                    seen.add(key)
                    out.call_sites.append(asdict(CallSite(
                        caller_id=caller,
                        called_name=called_name,
                        receiver=receiver,
                        line=line,
                    )))
        for child in node.children:
            _visit(child)

    _visit(root)


def extract_file(path: Path) -> Extraction:
    """Parse a single supported source file and return its extraction.

    Unsupported files (unknown suffix, binary, unreadable) raise ValueError.
    """
    suffix = supported_suffix(path)
    if suffix is None:
        raise ValueError(f"unsupported file type: {path.suffix}")
    try:
        source = path.read_bytes()
    except OSError as exc:
        raise ValueError(f"could not read {path}: {exc}") from exc
    parser = _parser_for(suffix)
    tree = parser.parse(source)
    file_id = str(path)
    return _walk_file(
        tree,
        file_id=file_id,
        source_path=str(path),
        file_label=path.name,
        language=suffix.lstrip("."),
    )


def extract_many(root: Path) -> list[Extraction]:
    """Recursively extract every supported source file under `root`.

    Returns one Extraction per file (never merged), which is what the pass-2
    resolver needs to attribute imports and call sites to their file. Files
    that cannot be parsed are skipped with a warning printed to stderr, not
    raised, so a single bad file does not abort a whole-tree load.
    """
    import sys

    out: list[Extraction] = []
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        if supported_suffix(path) is None:
            continue
        try:
            out.append(extract_file(path))
        except ValueError as exc:
            print(f"[cie] skip {path}: {exc}", file=sys.stderr)
    return out


def extract_tree(root: Path) -> Extraction:
    """Recursively extract every supported source file under `root`.

    Returns a single merged Extraction. Files that cannot be parsed are
    skipped with a warning printed to stderr, not raised, so a single bad
    file does not abort a whole-tree load.
    """
    out = Extraction()
    for per_file in extract_many(root):
        out.merge(per_file)
    return out
