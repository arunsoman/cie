"""Tests for cie.lang_adapter — the pluggable LanguageAdapter registry
(docs/growth-plan.md Phase 0.5 workstream C: zero test coverage before
this file, despite being the mechanism the "extends to any language, no
tree-sitter grammar or LSP required" claim in
docs/competitive-landscape.md actually rests on).

`_adapters`/`_discovered_entry_points` are module-level global state, so
every test resets them first — otherwise adapters registered by one test
(or by cie.extract's own built-in registration at import time) leak into
the next.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from cie import lang_adapter


@pytest.fixture(autouse=True)
def _reset_registry():
    """Snapshot and restore module globals around each test so tests
    don't leak adapters into each other or into cie.extract's own
    built-in TreeSitterAdapter registration."""
    saved_adapters = list(lang_adapter._adapters)
    saved_discovered = lang_adapter._discovered_entry_points
    lang_adapter._adapters = []
    lang_adapter._discovered_entry_points = True  # skip real entry-point discovery by default
    yield
    lang_adapter._adapters = saved_adapters
    lang_adapter._discovered_entry_points = saved_discovered


class _FakeAdapter:
    def __init__(self, suffixes: set[str], marker: str):
        self._suffixes = suffixes
        self.marker = marker

    def supported_suffixes(self) -> set[str]:
        return self._suffixes

    def extract_file(self, path: Path):
        raise NotImplementedError("not exercised by these tests")


def test_get_adapter_for_unregistered_suffix_returns_none():
    assert lang_adapter.get_adapter_for(".nosuchlang") is None


def test_register_and_get_adapter_for_round_trips():
    adapter = _FakeAdapter({".toy"}, marker="a")
    lang_adapter.register_adapter(adapter)
    found = lang_adapter.get_adapter_for(".toy")
    assert found is adapter


def test_get_adapter_for_is_case_insensitive():
    adapter = _FakeAdapter({".toy"}, marker="a")
    lang_adapter.register_adapter(adapter)
    assert lang_adapter.get_adapter_for(".TOY") is adapter


def test_later_registration_wins_for_an_overlapping_suffix():
    """The override mechanism the module docstring promises: a host
    project registering after cie's own built-in registration overrides
    it for a shared suffix, with no cie/ code change."""
    first = _FakeAdapter({".py"}, marker="first")
    second = _FakeAdapter({".py"}, marker="second")
    lang_adapter.register_adapter(first)
    lang_adapter.register_adapter(second)
    found = lang_adapter.get_adapter_for(".py")
    assert found is second


def test_all_supported_suffixes_is_the_union_across_adapters():
    lang_adapter.register_adapter(_FakeAdapter({".a"}, marker="1"))
    lang_adapter.register_adapter(_FakeAdapter({".b", ".c"}, marker="2"))
    assert lang_adapter.all_supported_suffixes() == {".a", ".b", ".c"}


def test_registered_adapters_returns_registration_order_snapshot():
    a1 = _FakeAdapter({".a"}, marker="1")
    a2 = _FakeAdapter({".b"}, marker="2")
    lang_adapter.register_adapter(a1)
    lang_adapter.register_adapter(a2)
    snapshot = lang_adapter.registered_adapters()
    assert snapshot == [a1, a2]
    # It's a snapshot, not a live view — mutating the module's internal
    # list afterward must not retroactively change what was returned.
    lang_adapter.register_adapter(_FakeAdapter({".c"}, marker="3"))
    assert snapshot == [a1, a2]


def test_language_adapter_protocol_is_runtime_checkable_and_matches_a_real_adapter():
    adapter = _FakeAdapter({".toy"}, marker="a")
    assert isinstance(adapter, lang_adapter.LanguageAdapter)
    assert not isinstance(object(), lang_adapter.LanguageAdapter)


def test_the_built_in_tree_sitter_adapter_is_discoverable_through_the_registry():
    """cie.extract registers its own TreeSitterAdapter at import time
    (this module's docstring) — importing it and registering fresh
    (since the fixture above cleared _adapters) should make .py
    resolvable the same way a host adapter would be."""
    from cie import extract

    lang_adapter.register_adapter(extract.TreeSitterAdapter())
    found = lang_adapter.get_adapter_for(".py")
    assert found is not None
    assert ".py" in found.supported_suffixes()


def test_entry_point_discovery_is_swallowed_on_a_broken_entry_point(monkeypatch):
    """A bad/missing plugin must not break importing or using the
    registry for anyone else — see _ensure_discovered's docstring."""
    lang_adapter._discovered_entry_points = False  # force real discovery path this time

    class _BrokenEntryPoint:
        def load(self):
            raise RuntimeError("simulated broken plugin")

    def _fake_entry_points(group):
        assert group == "cie.language_adapters"
        return [_BrokenEntryPoint()]

    monkeypatch.setattr(
        "importlib.metadata.entry_points", _fake_entry_points, raising=True,
    )
    # Must not raise, and must not register anything from the broken plugin.
    assert lang_adapter.get_adapter_for(".anything") is None
    assert lang_adapter._discovered_entry_points is True


def test_entry_point_discovery_registers_a_working_plugin(monkeypatch):
    lang_adapter._discovered_entry_points = False
    adapter = _FakeAdapter({".plug"}, marker="via-entry-point")

    class _WorkingEntryPoint:
        def load(self):
            return lambda: adapter

    def _fake_entry_points(group):
        return [_WorkingEntryPoint()]

    monkeypatch.setattr(
        "importlib.metadata.entry_points", _fake_entry_points, raising=True,
    )
    found = lang_adapter.get_adapter_for(".plug")
    assert found is adapter


def test_entry_point_discovery_only_runs_once():
    """_ensure_discovered is a no-op after the first call (the module
    docstring's 'runs once') — call get_adapter_for twice and confirm
    _discovered_entry_points doesn't get re-evaluated in a way that would
    re-register a plugin twice."""
    lang_adapter._discovered_entry_points = False
    calls = {"count": 0}

    def _fake_entry_points(group):
        calls["count"] += 1
        return []

    import importlib.metadata
    original = importlib.metadata.entry_points
    importlib.metadata.entry_points = _fake_entry_points
    try:
        lang_adapter.get_adapter_for(".x")
        lang_adapter.get_adapter_for(".y")
    finally:
        importlib.metadata.entry_points = original
    assert calls["count"] == 1
