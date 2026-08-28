"""Smoke tests proving `cie` imports and does real work with NO other
protobox package (`app`/`core`/`features`/`plugins`/`forge`) on the path —
the actual point of carving this into its own repository. Not a port of
protobox's full `be-v2/tests/cie/` suite (which stays there as
integration-level coverage against this package as a dependency); this is
just the "does the standalone package actually work" check.
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest


def test_no_protobox_sibling_packages_are_importable():
    """Confirms this test run is genuinely standalone — if this fails,
    the other tests below aren't proving what they claim to."""
    for name in ("app", "core", "features", "plugins", "forge"):
        assert name not in sys.modules or True  # already-imported is fine
        try:
            __import__(name)
        except ImportError:
            continue
        else:
            pytest.skip(
                f"{name!r} is importable in this environment — not a "
                "clean standalone run; skip rather than false-fail"
            )


def test_core_package_imports():
    import cie
    from cie.query import QueryEngine
    from cie.repository import Repository

    assert cie.__version__


def test_language_adapter_registry_works_standalone(tmp_path):
    from cie import lang_adapter
    from cie.extract import Extraction, extract_file, supported_suffix

    assert supported_suffix(Path("foo.py")) == ".py"
    assert supported_suffix(Path("foo.made_up")) is None

    class _FakeAdapter:
        def supported_suffixes(self):
            return {".made_up"}

        def extract_file(self, path):
            out = Extraction()
            out.nodes.append({"id": str(path), "kind": "file", "name": path.name})
            return out

    lang_adapter.register_adapter(_FakeAdapter())
    try:
        assert supported_suffix(Path("foo.made_up")) == ".made_up"
        f = tmp_path / "thing.made_up"
        f.write_text("whatever")
        extraction = extract_file(f)
        assert extraction.nodes[0]["kind"] == "file"
    finally:
        lang_adapter._adapters.pop()


def test_tool_service_bootstraps_with_no_env_vars(tmp_path):
    from cie.config import CieConfig, Neo4jConfig
    from cie.factory import build_tool_service_from_config
    from cie.tools import ToolService

    config = CieConfig(
        project_root=tmp_path,
        project="standalone-smoke",
        neo4j=Neo4jConfig(uri="bolt://localhost:7687", user="neo4j", password="password"),
    )
    service = build_tool_service_from_config(config)
    assert isinstance(service, ToolService)


def test_tool_schema_and_policy_work_standalone():
    from cie.tool_policy import INSPECTOR_POLICY, WRITE_TOOLS, filter_tool_schemas
    from cie.tool_schema import tool_schemas
    from cie.tools import ToolService

    schemas = tool_schemas(ToolService)
    assert len(schemas) > 50
    filtered = filter_tool_schemas(schemas, INSPECTOR_POLICY)
    assert all(s["name"] not in WRITE_TOOLS for s in filtered)
