"""Pure import-statement formatting, shared by the two `resolve_import`
implementations (`cie.tools.ToolService`, the graph-backed one, and
`forge.tools.LocalDiskBackend`, the heuristic-only one) so the actual
Python-dotted-path / JS-TS-relative-path derivation exists in exactly one
place. Both callers already have their own `search_symbol` results (graph
or heuristic — same shape modulo an optional `score` key); this module
only turns those into ready-to-paste import statements.
"""
from __future__ import annotations

import posixpath

# Mirrors cie.callgraph._JS_EXTENSIONS — kept as its own copy since that
# one is a private module constant and the two lists are allowed to
# diverge (this one only gates import-statement FORMATTING, not call-edge
# resolution).
_JS_TS_EXTENSIONS = (".js", ".jsx", ".mjs", ".cjs", ".ts", ".tsx")


def format_import(candidates: list[dict], symbol: str, importing_file: str = "",
                  language: str = "") -> tuple[list[dict], str | None]:
    """`candidates`: `search_symbol`-shaped hits (`name`/`kind`/
    `source_file`/`confidence`, `score` optional). Returns
    `(results, hint)` — `results` already sorted best-match-first, `hint`
    explaining an empty or ambiguous result (`None` when there's exactly
    one unambiguous match)."""
    importing_file = importing_file.replace("\\", "/")
    importing_dir = posixpath.dirname(importing_file)

    results: list[dict] = []
    for c in candidates:
        source_file = (c.get("source_file") or "").replace("\\", "/")
        if not source_file or source_file == importing_file:
            continue
        ext = posixpath.splitext(source_file)[1]
        entry = {
            "symbol": c["name"], "kind": c["kind"], "source_file": source_file,
            "confidence": c.get("confidence"), "score": c.get("score", 1.0),
        }
        if ext == ".py":
            module_path = source_file[: -len(ext)].replace("/", ".")
            if module_path.endswith(".__init__"):
                module_path = module_path[: -len(".__init__")]
            entry["language"] = "python"
            entry["module_path"] = module_path
            entry["import_statement"] = f"from {module_path} import {c['name']}"
        elif ext in _JS_TS_EXTENSIONS:
            target_noext = source_file[: -len(ext)]
            if importing_file:
                rel = posixpath.relpath(target_noext, importing_dir or ".")
            else:
                rel = target_noext
            if not rel.startswith((".", "/")):
                rel = "./" + rel
            entry["language"] = "typescript"
            entry["relative_path"] = rel
            entry["import_statement"] = f"import {{ {c['name']} }} from '{rel}'"
            if not importing_file:
                entry["hint"] = (
                    "no importing_file given — this path is only project-"
                    "root-relative, not adjusted for where you're writing "
                    "the import; pass importing_file for a usable path"
                )
        else:
            entry["language"] = "unknown"
            entry["import_statement"] = None
            entry["hint"] = f"unrecognized source extension {ext!r}; import manually"
        results.append(entry)

    if language:
        narrowed = [r for r in results if r["language"] == language]
        if narrowed:
            results = narrowed

    if not results:
        hint = (
            f"'{symbol}' is only defined in {importing_file!r} itself — no import needed"
            if candidates and importing_file
            else f"no importable definition of '{symbol}' found; check the "
                 "spelling, or the defining file hasn't been written yet"
        )
        return [], hint

    results.sort(key=lambda r: r["score"], reverse=True)
    hint = None
    if len(results) > 1:
        files = sorted({r["source_file"] for r in results})
        hint = (
            f"{len(results)} definitions of '{symbol}' found across {files} "
            "— the highest-score match is listed first; verify it's the "
            "one you mean before using it"
        )
    return results, hint
