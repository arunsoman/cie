"""R10 — the first-party OpenAI-compatible embeddings fallback.

`cie.embed`'s dispatch has four tiers now: host `core.llm` › registered
override › THIS env-gated stdlib client › raise. The contract pinned
here:

- **No accidental network.** `NVIDIA_API_KEY` alone (the be-v2 host's
  gate) must NOT enable the standalone fallback — only an explicit
  `CIE_EMBED_DSN` plus a key does. Bare `pytest -q` stays HTTP-free,
  even on a shell carrying provider keys.
- Config resolution: `CIE_EMBED_API_KEY` wins over `NVIDIA_API_KEY`;
  `CIE_EMBED_MODEL` overrides the NIM default; empty DSN is no config.
- Wire shape: POST `<dsn>/embeddings`, Bearer auth, `input_type`
  pass-through (nv-embedqa requires it), vectors re-ordered by the API's
  `index`, count-mismatch raises.
- Embedded integration: `EmbeddedRepository.load_extraction` enriches
  extraction rows with real vectors when supported (persisted by the
  normal flush; dense signal then flows into `semantic_search`), and
  skips silently when not.
- Host-registered override beats the env client (tier order).
"""

from __future__ import annotations

import pytest

import cie.embed as embed
from cie.embedded_repository import EmbeddedRepository


@pytest.fixture(autouse=True)
def _clean_env(monkeypatch):
    """Every test starts from an ambient-env-clean state (this machine
    carries a real NVIDIA_API_KEY in some shells — the no-DSN rule must
    hold in exactly that environment)."""
    for var in ("CIE_EMBED_DSN", "CIE_EMBED_API_KEY", "CIE_EMBED_MODEL",
                "NVIDIA_API_KEY"):
        monkeypatch.delenv(var, raising=False)


@pytest.fixture
def real_vars(monkeypatch):
    """Simulate THIS machine's shell: a provider key set, nothing else."""
    monkeypatch.setenv("NVIDIA_API_KEY", "nvapi-real-key")
    return monkeypatch


# -- config resolution ------------------------------------------------------


def test_no_dsn_means_no_config_even_with_a_provider_key(real_vars):
    """The deliberate no-accidental-network rule: a bare provider key in
    the environment never turns HTTP calls on."""
    assert embed._env_fallback_config() is None
    assert embed.supports_embeddings() is False


def test_dsn_plus_fallback_key_enables(real_vars):
    real_vars.setenv("CIE_EMBED_DSN", "https://integrate.api.nvidia.com/v1/")
    dsn, key, model = embed._env_fallback_config()
    assert (dsn, key) == ("https://integrate.api.nvidia.com/v1", "nvapi-real-key")
    assert model == "nvidia/nv-embedqa-e5-v5"  # NIM retrieval model default
    assert dsn.rstrip("/") == dsn  # no double-slash on the POST URL


def test_explicit_key_wins_over_provider_key(monkeypatch):
    monkeypatch.setenv("CIE_EMBED_DSN", "https://api.example.com/v1")
    monkeypatch.setenv("CIE_EMBED_API_KEY", "explicit")
    monkeypatch.setenv("NVIDIA_API_KEY", "ambient")
    assert embed._env_fallback_config()[1] == "explicit"
    assert embed.supports_embeddings() is True


def test_dsn_without_any_key_is_not_configured(monkeypatch):
    monkeypatch.setenv("CIE_EMBED_DSN", "https://api.example.com/v1")
    assert embed._env_fallback_config() is None


def test_model_override(monkeypatch):
    monkeypatch.setenv("CIE_EMBED_DSN", "https://api.example.com/v1")
    monkeypatch.setenv("CIE_EMBED_API_KEY", "k")
    monkeypatch.setenv("CIE_EMBED_MODEL", "baai/bge-m3")
    assert embed._env_fallback_config()[2] == "baai/bge-m3"


# -- dispatch / wire shape --------------------------------------------------


def test_unconfigured_raise_mentionsevery_alternative(monkeypatch, real_vars):
    with pytest.raises(RuntimeError, match="CIE_EMBED_DSN"):
        embed.embed_text("hello")


def test_env_client_serves_the_dispatch(monkeypatch):
    monkeypatch.setenv("CIE_EMBED_DSN", "https://api.example.com/v1")
    monkeypatch.setenv("CIE_EMBED_API_KEY", "k")
    captured = {}

    def fake_post(dsn, key, model, texts, input_type):
        captured.update(dsn=dsn, key=key, model=model,
                        texts=texts, input_type=input_type)
        return [[0.1, 0.2, 0.3] for _ in texts]

    monkeypatch.setattr(embed, "_post_embeddings", fake_post)
    vec = embed.embed_text("query text", input_type="query")
    assert vec == [0.1, 0.2, 0.3]
    assert captured["input_type"] == "query"  # call-site hint preserved
    assert captured["texts"] == ["query text"]
    vecs = embed.embed_texts(["a", "b"])
    assert vecs == [[0.1, 0.2, 0.3], [0.1, 0.2, 0.3]]


def test_post_embeddings_orders_by_index(monkeypatch):
    """Server may return data out of order — the client re-sorts by the
    API's `index` field, never trusts response order."""
    monkeypatch.setenv("CIE_EMBED_DSN", "https://api.example.com/v1")
    sent = {}

    class FakeResp:
        def __enter__(self): return self
        def __exit__(self, *a): return False
        def read(self):
            import json as _json
            return _json.dumps({"data": [
                {"index": 1, "embedding": [3.0]},
                {"index": 0, "embedding": [2.0]},
            ]}).encode()

    def fake_urlopen(req, timeout=None):
        sent["url"] = req.full_url
        sent["headers"] = dict(req.header_items())
        sent["body"] = req.data
        return FakeResp()

    from urllib import request as _urllib_request
    monkeypatch.setattr(_urllib_request, "urlopen", fake_urlopen)
    vecs = embed._post_embeddings(
        "https://api.example.com/v1", "sekrit", "m/m", ["t0", "t1"], "passage",
    )
    assert vecs == [[2.0], [3.0]]
    assert sent["url"] == "https://api.example.com/v1/embeddings"
    assert sent["headers"].get("Authorization") in ("Bearer sekrit", "bearer sekrit")
    import json as _json
    body = _json.loads(sent["body"])
    assert body["model"] == "m/m" and body["input_type"] == "passage"
    assert body["input"] == ["t0", "t1"]


def test_post_embeddings_count_mismatch_raises(monkeypatch):
    monkeypatch.setenv("CIE_EMBED_DSN", "https://api.example.com/v1")

    class FakeResp:
        def __enter__(self): return self
        def __exit__(self, *a): return False
        def read(self):
            import json as _json
            return _json.dumps({"data": [{"index": 0, "embedding": [1.0]}]}).encode()

    from urllib import request as _urllib_request
    monkeypatch.setattr(_urllib_request, "urlopen", lambda req, timeout=None: FakeResp())
    with pytest.raises(RuntimeError, match="1 vectors for 2"):
        embed._post_embeddings(
            "https://api.example.com/v1", "k", "m", ["a", "b"], "passage",
        )


def test_registered_override_beats_env_client(monkeypatch):
    """Tier order: a host-registered override beats the config-derived
    client (core.llm beats both — untestable here without the host)."""
    monkeypatch.setenv("CIE_EMBED_DSN", "https://api.example.com/v1")
    monkeypatch.setattr(embed, "_post_embeddings", lambda *a, **k: pytest.fail("env client used"))
    try:
        embed.register_embed_functions(
            lambda text, model_name=None, input_type="passage": [42.0],
            lambda texts, model_name=None, input_type="passage": [[42.0]] * len(texts),
        )
        assert embed.embed_text("x") == [42.0]
        assert embed.embed_texts(["x"]) == [[42.0]]
    finally:
        embed.register_embed_functions(None, None)  # restore the env path


# -- embedded integration ---------------------------------------------------


def test_embedded_load_enriches_and_persists_embeddings(tmp_path, monkeypatch):
    monkeypatch.setenv("CIE_EMBED_DSN", "https://api.example.com/v1")
    try:
        # deterministic fake transport — no HTTP
        embed.register_embed_functions(
            lambda text, model_name=None, input_type="passage": [1.0, 0.0],
            lambda texts, model_name=None, input_type="passage": [
                [1.0, 0.0] for _ in texts
            ],
        )
        root = tmp_path / "proj"
        root.mkdir()
        (root / "app.py").write_text("def alpha():\n    return 1\n")

        from cie.extract import extract_many
        from cie.callgraph import resolve_call_edges

        per_file = extract_many(root)
        nodes = [dict(n) for ext in per_file for n in ext.nodes]
        edges = [e for ext in per_file for e in ext.edges]
        edges += resolve_call_edges(per_file)

        db = tmp_path / "graph.db"
        repo = EmbeddedRepository(db)
        count = repo.load_extraction(nodes, edges)
        assert count == len(nodes)

        with_embedding = [
            n for n in repo._nodes.values() if n.embedding  # noqa: SLF001
        ]
        assert with_embedding, "enrichment should have filled vectors"
        assert all(tuple(n.embedding) == (1.0, 0.0) for n in with_embedding)

        # persisted: a fresh repo over the same file sees the vectors
        reloaded = EmbeddedRepository(db)
        reloaded_nodes = [n for n in reloaded._nodes.values() if n.embedding]  # noqa: SLF001
        assert len(reloaded_nodes) == len(with_embedding)

        # dense signal flows into semantic_search (lexical-only would
        # match "alpha" too, but the dense path must not silently die)
        matches = reloaded.semantic_search("text that.embeds like a passage", top_k=3)
        assert matches, "semantic_search should return dense matches now"
        assert all(m.score > 0 for m in matches)
    finally:
        embed.register_embed_functions(None, None)


def test_embedded_load_skips_silently_when_unconfigured(tmp_path):
    """No DSN, no override, no core.llm → load proceeds with empty
    embeddings, zero network attempts (there is nothing to call)."""
    root = tmp_path / "proj"
    root.mkdir()
    (root / "app.py").write_text("def alpha():\n    return 1\n")

    from cie.extract import extract_many

    per_file = extract_many(root)
    nodes = [dict(n) for ext in per_file for n in ext.nodes]
    edges = [e for ext in per_file for e in ext.edges]

    repo = EmbeddedRepository(tmp_path / "graph.db")
    with pytest.raises(RuntimeError, match="CIE_EMBED_DSN"):
        # direct probe: the embed path itself must raise, not degrade
        embed.embed_text("anything")
    repo.load_extraction(nodes, edges)
    assert all(not n.embedding for n in repo._nodes.values())  # noqa: SLF001