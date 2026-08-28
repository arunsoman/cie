"""Pluggable language-adapter registry for `cie.extract`.

`cie/extract.py`'s tree-sitter based extraction (Python/JS/TS/Java/Go/Rust) is one
`LanguageAdapter` implementation among possibly many — see
`extract.TreeSitterAdapter`, registered as the built-in default at the
bottom of that module. A host project whose language has no tree-sitter
grammar (or that wants to reuse its own compiler's AST dump instead —
e.g. a `nirdosha`-style adapter wrapping `nirdosha emit-ast`) registers its
own adapter here via `register_adapter`, and `cie.extract`'s public
`supported_suffix`/`extract_file`/`extract_many`/`extract_tree` route
through this registry instead of a hardcoded per-language dict. No cie/
code change is needed to add a language — see `register_adapter` and
`_ensure_discovered` below for the two ways an adapter gets in.

Scope note (Phase 1 of docs/plans/cie-standalone-any-project-plan.md):
only `cie.extract`'s Extraction-producing path is adapter-based. A few
consumers (`cie.source_analysis`, `cie.sync`) call `cie.extract.parse_file`
directly for the raw tree-sitter tree, not just the walked `Extraction` —
those stay tree-sitter-only for now; a non-tree-sitter adapter's files are
simply invisible to them, same as any other unsupported suffix already is.
Closing that second seam is tracked separately, not silently assumed done
here.
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING, Optional, Protocol, runtime_checkable

if TYPE_CHECKING:  # avoid a runtime import cycle with cie.extract
    from cie.extract import Extraction


@runtime_checkable
class LanguageAdapter(Protocol):
    """One pluggable extractor: owns a set of file suffixes and knows how
    to turn one file's content into a cie-generic Extraction."""

    def supported_suffixes(self) -> set[str]:
        """Lowercased suffixes this adapter handles, e.g. ``{".py"}``."""
        ...

    def extract_file(self, path: Path) -> "Extraction":
        """Parse `path` and return its Extraction.

        Raises ValueError on an unreadable/unparseable file — same
        contract `cie.extract.extract_file` already documents; callers
        do not need to know which adapter served a given suffix."""
        ...


_adapters: list[LanguageAdapter] = []
_discovered_entry_points = False


def register_adapter(adapter: LanguageAdapter) -> None:
    """Register `adapter`. Later registrations win over earlier ones for
    an overlapping suffix (see `get_adapter_for`) — so a host project can
    override the built-in `TreeSitterAdapter` for a suffix it wants special
    handling for, just by registering after cie's own import-time
    registration runs, without editing `cie/extract.py`."""
    _adapters.append(adapter)


def get_adapter_for(suffix: str) -> Optional[LanguageAdapter]:
    """Return the most-recently-registered adapter that claims `suffix`
    (lowercased, with leading dot, e.g. ``".py"``), or None."""
    _ensure_discovered()
    suffix = suffix.lower()
    for adapter in reversed(_adapters):
        if suffix in adapter.supported_suffixes():
            return adapter
    return None


def all_supported_suffixes() -> set[str]:
    """Union of every registered adapter's suffixes."""
    _ensure_discovered()
    out: set[str] = set()
    for adapter in _adapters:
        out |= adapter.supported_suffixes()
    return out


def registered_adapters() -> list[LanguageAdapter]:
    """Snapshot of every registered adapter, registration order. Mainly
    for introspection/tests — most callers want `get_adapter_for`."""
    _ensure_discovered()
    return list(_adapters)


def _ensure_discovered() -> None:
    """Lazily pull in adapters registered via the ``cie.language_adapters``
    entry-point group (`importlib.metadata`), so an external package can
    add language support for its own project just by being installed on
    the same environment — no `cie/` code change, no explicit
    `register_adapter` call from application code either. Runs once; a
    broken or missing entry point is swallowed the same way a missing
    tree-sitter grammar wheel already is in `cie.extract` (see that
    module's docstring) — one bad plugin should not break importing this
    module for everyone else."""
    global _discovered_entry_points
    if _discovered_entry_points:
        return
    _discovered_entry_points = True
    try:
        from importlib.metadata import entry_points

        eps = entry_points(group="cie.language_adapters")
    except Exception:
        return
    for ep in eps:
        try:
            factory = ep.load()
            register_adapter(factory())
        except Exception:
            continue
