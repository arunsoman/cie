"""Pass-2 call-graph resolution.

Turns the per-file extraction output (nodes + imports + call sites recorded by
cie.extract) into `calls` edges with a confidence tag, following the
resolution order from the integration spec:

(a) same-file definition -> EXTRACTED;
(b) resolved via the file's import map to a definition in another extracted
    file -> EXTRACTED (relative imports normalized against the importing
    file's path);
(c) receiver-type heuristic -> INFERRED (receiver names a class, or a class
    instance by naming convention, defined in the same file or bound through
    the file's import map);
(d) unresolved -> AMBIGUOUS when exactly one same-named function/method
    exists project-wide, otherwise the call site is skipped.

The module is pure: it consumes Extraction objects and returns edge dicts of
the shape ``{source, target, relation: "calls", confidence, line}``. Nothing
here touches the filesystem, a database, or the network.
"""

from __future__ import annotations

import posixpath
from typing import Optional

from cie.extract import Extraction
from cie.models import Confidence, NodeKind

# JS/TS extensions probed when resolving a module specifier to a file.
_JS_EXTENSIONS = (".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx")

# Receivers that mean "the enclosing class instance" (py self / js this).
_SELF_RECEIVERS = ("self", "this")


def _norm(path: str) -> str:
    """Normalize a path string for comparison (forward slashes, no dots)."""
    return posixpath.normpath(path.replace("\\", "/"))


def _file_hub_id(extraction: Extraction) -> str:
    """Return the file hub node id of a per-file extraction, or ''."""
    for node in extraction.nodes:
        if node.get("kind") == NodeKind.FILE.value:
            return node.get("id", "")
    return ""


class _Index:
    """Project-wide symbol/import index built from per-file extractions."""

    def __init__(self, per_file: list[Extraction]) -> None:
        # file id -> {symbol name -> node id} (classes, functions, methods)
        self.symbols: dict[str, dict[str, str]] = {}
        # file id -> {class name -> {method name -> method node id}}
        self.class_methods: dict[str, dict[str, dict[str, str]]] = {}
        # file id -> {class name -> class node id}
        self.classes: dict[str, dict[str, str]] = {}
        # symbol name -> [func/method node ids] project-wide (rule d)
        self.global_funcs: dict[str, list[str]] = {}
        # known extracted file ids (path strings, normalized)
        self.known_files: dict[str, str] = {}  # normalized -> original id
        # file id -> {instance name -> class name} from constructor assignments
        self.bindings: dict[str, dict[str, str]] = {}
        self._build(per_file)

    def _class_scope(self, file_id: str, import_map: dict[str, str],
                     class_name: str) -> Optional[str]:
        """Return the file id where class_name is visible from file_id."""
        if class_name in self.classes.get(file_id, {}):
            return file_id
        target = import_map.get(class_name)
        if target and class_name in self.classes.get(target, {}):
            return target
        return None

    def _build(self, per_file: list[Extraction]) -> None:
        edges_by_file: dict[str, list[dict]] = {}
        nodes_by_file: dict[str, dict[str, dict]] = {}
        for ext in per_file:
            file_id = _file_hub_id(ext)
            if not file_id:
                continue
            self.known_files.setdefault(_norm(file_id), file_id)
            symbols = self.symbols.setdefault(file_id, {})
            classes = self.classes.setdefault(file_id, {})
            by_id = nodes_by_file.setdefault(file_id, {})
            for node in ext.nodes:
                kind = node.get("kind", "")
                name = node.get("label", "")
                node_id = node.get("id", "")
                if not name or not node_id:
                    continue
                by_id[node_id] = node
                if kind == NodeKind.CLASS.value:
                    classes.setdefault(name, node_id)
                    symbols.setdefault(name, node_id)
                elif kind in (NodeKind.FUNC.value, NodeKind.METHOD.value):
                    symbols.setdefault(name, node_id)
                    self.global_funcs.setdefault(name, []).append(node_id)
            edges_by_file[file_id] = ext.edges
            bindings = self.bindings.setdefault(file_id, {})
            for binding in getattr(ext, "instance_bindings", []) or []:
                name = binding.get("name", "")
                class_name = binding.get("class_name", "")
                if name and class_name:
                    bindings.setdefault(name, class_name)
        # class -> methods from contains edges (needed for receiver heuristics)
        for file_id, edges in edges_by_file.items():
            methods = self.class_methods.setdefault(file_id, {})
            id_to_node = nodes_by_file.get(file_id, {})
            for edge in edges:
                if edge.get("relation") != "contains":
                    continue
                class_node = id_to_node.get(edge.get("source", ""))
                method_node = id_to_node.get(edge.get("target", ""))
                if not class_node or not method_node:
                    continue
                if method_node.get("kind") != NodeKind.METHOD.value:
                    continue
                cls = methods.setdefault(class_node.get("label", ""), {})
                cls.setdefault(method_node.get("label", ""), method_node["id"])

    # -- module path resolution ------------------------------------------

    def _match_candidates(self, candidates: list[str], suffix_match: bool) -> Optional[str]:
        """Match candidate module paths against the known extracted files."""
        for cand in candidates:
            hit = self.known_files.get(_norm(cand))
            if hit is not None:
                return hit
        if not suffix_match:
            return None
        matches: list[str] = []
        for cand in candidates:
            norm = _norm(cand)
            for known_norm, original in self.known_files.items():
                if known_norm.endswith("/" + norm):
                    matches.append(original)
        if not matches:
            return None
        # deterministic: prefer the shortest (closest package root), then name
        matches.sort(key=lambda p: (len(p), p))
        return matches[0]

    def _resolve_python_module(self, module: str, importing_file: str) -> Optional[str]:
        """Resolve a Python module string (dots preserved) to a file id."""
        level = len(module) - len(module.lstrip("."))
        rest = module.lstrip(".")
        if level > 0:
            base = posixpath.dirname(_norm(importing_file))
            for _ in range(level - 1):
                base = posixpath.dirname(base)
            rel = rest.replace(".", "/")
            cand = posixpath.join(base, rel) if rel else base
            return self._match_candidates(
                [cand + ".py", cand + "/__init__.py"], suffix_match=False,
            )
        rel = rest.replace(".", "/")
        return self._match_candidates(
            [rel + ".py", rel + "/__init__.py"], suffix_match=True,
        )

    def _resolve_js_module(self, module: str, importing_file: str) -> Optional[str]:
        """Resolve a JS/TS module specifier to a file id."""
        candidates: list[str] = []
        if module.startswith("."):
            base = _norm(posixpath.join(posixpath.dirname(_norm(importing_file)), module))
            candidates.append(base)
            candidates.extend(base + ext for ext in _JS_EXTENSIONS)
            candidates.extend(base + "/index" + ext for ext in _JS_EXTENSIONS)
            return self._match_candidates(candidates, suffix_match=False)
        candidates.extend(module + ext for ext in _JS_EXTENSIONS)
        candidates.extend(module + "/index" + ext for ext in _JS_EXTENSIONS)
        return self._match_candidates(candidates, suffix_match=True)

    def resolve_import(self, record: dict, importing_file: str, is_python: bool) -> Optional[str]:
        """Resolve one ImportRecord dict to the defining file id, or None."""
        module = record.get("module", "")
        name = record.get("imported_name", "")
        kind = record.get("kind", "")
        if not module:
            return None
        if is_python:
            if kind == "from_import":
                # `from pkg import submod`: probe the submodule path first.
                level = len(module) - len(module.lstrip("."))
                rest = module.lstrip(".")
                dotted = f"{rest}.{name}" if rest else name
                sub = "." * level + dotted
                hit = self._resolve_python_module(sub, importing_file)
                if hit is not None:
                    return hit
            return self._resolve_python_module(module, importing_file)
        return self._resolve_js_module(module, importing_file)

    def import_map_for(self, file_id: str, imports: list[dict]) -> dict[str, str]:
        """Build imported_name -> defining file id for one file."""
        is_python = file_id.endswith(".py")
        out: dict[str, str] = {}
        for record in imports:
            name = record.get("imported_name", "")
            if not name or name in out:
                continue
            target = self.resolve_import(record, file_id, is_python)
            if target is not None:
                out[name] = target
        return out


def _enclosing_class_method(index: _Index, file_id: str, caller_id: str,
                            called_name: str) -> Optional[str]:
    """Resolve `self.name()`/`this.name()` through the caller's own class."""
    for methods in index.class_methods.get(file_id, {}).values():
        if caller_id in methods.values():
            return methods.get(called_name)
    return None


def _method_of_class(index: _Index, file_id: str, class_name: str,
                     called_name: str) -> Optional[str]:
    """Return the node id of called_name as a method of class_name in file."""
    return index.class_methods.get(file_id, {}).get(class_name, {}).get(called_name)


def _resolve_receiver(index: _Index, file_id: str, import_map: dict[str, str],
                      receiver: str, called_name: str) -> tuple[Optional[str], Optional[Confidence]]:
    """Rules (b)/(c) for receiver calls: `receiver.name(...)`."""
    # (c) receiver matches a constructor assignment in the same file
    # (`svc = Service()` / `const svc = new Service()`).
    class_name = index.bindings.get(file_id, {}).get(receiver)
    if class_name:
        scope = index._class_scope(file_id, import_map, class_name)
        if scope is not None:
            hit = _method_of_class(index, scope, class_name, called_name)
            if hit is not None:
                return hit, Confidence.INFERRED
    # (b) receiver is a module/namespace binding from the import map.
    if receiver in import_map:
        target_file = import_map[receiver]
        if receiver not in index.classes.get(target_file, {}):
            hit = index.symbols.get(target_file, {}).get(called_name)
            if hit is not None:
                return hit, Confidence.EXTRACTED
    # (c) receiver names an imported (or same-file) class exactly.
    scope = index._class_scope(file_id, import_map, receiver)
    if scope is not None:
        hit = _method_of_class(index, scope, receiver, called_name)
        if hit is not None:
            return hit, Confidence.INFERRED
    # (c) receiver matches a class by instance naming convention
    # (`service.run()` for class Service), same file first, then imports.
    lowered = receiver.lower()
    candidate_files = [file_id, *import_map.values()]
    for scope_file in candidate_files:
        for cls in index.classes.get(scope_file, {}):
            if cls.lower() == lowered:
                hit = _method_of_class(index, scope_file, cls, called_name)
                if hit is not None:
                    return hit, Confidence.INFERRED
    return None, None


def resolve_call_edges(per_file: list[Extraction]) -> list[dict]:
    """Resolve pass-2 `calls` edges from per-file extractions.

    Args:
        per_file: one Extraction per source file, each carrying `imports`
            and `call_sites` (see cie.extract.extract_many). Merged
            multi-file extractions are not supported: import/call-site
            attribution requires one file hub per Extraction.

    Returns:
        Edge dicts ``{source, target, relation: "calls", confidence, line}``
        where confidence is one of EXTRACTED/INFERRED/AMBIGUOUS. Call sites
        that cannot be resolved (rule d with zero or several same-named
        symbols) are skipped.
    """
    index = _Index(per_file)
    edges: list[dict] = []
    seen: set[tuple[str, str, str, int]] = set()

    for ext in per_file:
        file_id = _file_hub_id(ext)
        if not file_id:
            continue
        import_map = index.import_map_for(file_id, ext.imports)
        local = index.symbols.get(file_id, {})

        for site in ext.call_sites:
            caller = site.get("caller_id", "")
            name = site.get("called_name", "")
            receiver = site.get("receiver", "")
            line = int(site.get("line", 0) or 0)
            if not caller or not name:
                continue

            target: Optional[str] = None
            confidence: Optional[Confidence] = None

            if receiver in _SELF_RECEIVERS:
                # self/this: method of the caller's own class -> same file.
                target = _enclosing_class_method(index, file_id, caller, name)
                if target is not None:
                    confidence = Confidence.EXTRACTED
            elif not receiver:
                # (a) same-file definition.
                target = local.get(name)
                if target is not None:
                    confidence = Confidence.EXTRACTED
                # (b) bare import binding -> definition in another file.
                if target is None and name in import_map:
                    target = index.symbols.get(import_map[name], {}).get(name)
                    if target is not None:
                        confidence = Confidence.EXTRACTED
            else:
                target, confidence = _resolve_receiver(
                    index, file_id, import_map, receiver, name,
                )

            # (d) project-wide unique same-named function/method.
            if target is None:
                candidates = index.global_funcs.get(name, [])
                if len(candidates) == 1:
                    target = candidates[0]
                    confidence = Confidence.AMBIGUOUS

            if target is None or confidence is None:
                continue
            key = (caller, target, confidence.value, line)
            if key in seen:
                continue
            seen.add(key)
            edges.append({
                "source": caller,
                "target": target,
                "relation": "calls",
                "confidence": confidence.value,
                "line": line,
            })

    return edges
