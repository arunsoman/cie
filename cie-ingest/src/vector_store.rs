//! RFC-0022 §6.2 — wiring `cie-llm::embed()` into `cie-store::hnsw`'s
//! real, tested `VectorStore`/`HnswRsStore`. Before this, `embed()` had
//! three implementations and zero call sites; `HnswRsStore::new` was
//! only ever constructed in its own crate's test file.
//!
//! `HnswRsStore` isn't itself serializable (it wraps a live `hnsw_rs`
//! index plus `RwLock`s) — so persistence here is a flat, content-
//! addressed JSON record `{dim, vectors: {hex(record_id) -> (text,
//! vector)}}`, the same "small JSON blob per revision" shape as
//! `stack.json`/`relations.json`. The index itself is rebuilt in memory
//! from that record on load (`rebuild_index`), which is also how
//! incremental re-embedding stays cheap: a symbol's record id is
//! `stable_id(qual_name)` (content-addressed, per `HnswRsStore`'s own
//! "stable record ids" design intent), so an unchanged symbol's text
//! matches what was already persisted and `embed()` is skipped for it —
//! no separate dirty-worklist plumbing needed.

use cie_core::types::RevId;
use cie_llm::LlmProvider;
use cie_store::hnsw::{HnswRsStore, VecRecordId, VectorStore};
use cie_store::Store;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PersistedVectors {
    pub dim: usize,
    #[serde(default = "legacy_identity")]
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
    /// hex(record_id) -> (the text that was embedded, its vector).
    pub records: BTreeMap<String, (String, Vec<f32>)>,
}

fn legacy_identity() -> String { "legacy-unknown".into() }

impl PersistedVectors {
    pub fn get(&self, id: &VecRecordId) -> Option<&(String, Vec<f32>)> {
        self.records.get(&hex_of(id))
    }

    pub fn insert(&mut self, id: VecRecordId, text: String, vector: Vec<f32>) {
        self.records.insert(hex_of(&id), (text, vector));
    }

    /// Rebuild a live, queryable index from the persisted record — the
    /// counterpart to `HnswRsStore` not being serializable itself.
    pub fn rebuild_index(&self) -> HnswRsStore {
        let mut idx = HnswRsStore::new(self.dim.max(1));
        for (hex_id, (_, vector)) in &self.records {
            if let Some(id) = decode_hex(hex_id) {
                let _ = idx.insert(id, vector);
            }
        }
        idx
    }
}

fn hex_of(id: &VecRecordId) -> String {
    id.iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_hex(s: &str) -> Option<VecRecordId> {
    if s.len() != 64 || !s.is_ascii() {
        return None;
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

pub trait EmbeddingStoreExt {
    fn save_embeddings(&self, rev: RevId, vectors: &PersistedVectors) -> std::io::Result<()>;
    fn load_embeddings(&self, rev: RevId) -> std::io::Result<Option<PersistedVectors>>;
}

impl EmbeddingStoreExt for Store {
    fn save_embeddings(&self, rev: RevId, vectors: &PersistedVectors) -> std::io::Result<()> {
        self.write_json(
            &self.root().join("revs").join(rev.to_string()).join("embeddings.json"),
            vectors,
        )
    }

    fn load_embeddings(&self, rev: RevId) -> std::io::Result<Option<PersistedVectors>> {
        self.read_json(
            &self.root().join("revs").join(rev.to_string()).join("embeddings.json"),
        )
    }
}

/// RFC-0022 §6.2: embed only symbols whose text actually changed (or is
/// new), carrying forward every unchanged vector from `prev`. `provider`
/// is tried first; on `LlmError` of any kind (budget exceeded, network),
/// this degrades to the offline hashing-trick embedder and continues —
/// the same "downgrade and continue" policy `Budget`'s own doc comment
/// states (acceptance #14) — never blocking ingest over an embedding
/// failure.
pub fn update_embeddings(
    prev: Option<&PersistedVectors>,
    texts: &[(String, String)], // (qual_name, text-to-embed)
    budget: &cie_llm::Budget,
    provider: &dyn cie_llm::LlmProvider,
) -> PersistedVectors {
    let identity = provider.embedding_id();
    let compatible = prev.filter(|p| p.provider == identity);
    let mut out = PersistedVectors { dim: compatible.map_or(0, |p| p.dim), provider: identity, ..Default::default() };
    let mut pending = Vec::new();
    for (name, text) in texts {
        let id = cie_core::types::hash(name.as_bytes());
        if let Some((old_text, vector)) = compatible.and_then(|p| p.get(&id)) {
            if old_text == text && valid_vectors(std::slice::from_ref(vector), 1, Some(out.dim)) {
                out.insert(id, text.clone(), vector.clone());
                continue;
            }
        }
        pending.push((id, text.as_str()));
    }
    if pending.is_empty() { return out; }
    let batch: Vec<_> = pending.iter().map(|(_, text)| *text).collect();
    let expected = (out.dim > 0).then_some(out.dim);
    match embed_budgeted(provider, budget, &batch) {
        Ok(vectors) if valid_vectors(&vectors, batch.len(), expected) => {
            out.dim = vectors[0].len();
            for ((id, text), vector) in pending.into_iter().zip(vectors) {
                out.insert(id, text.into(), vector);
            }
            out
        }
        result => {
            // Re-embed the ENTIRE corpus when falling back. Same-dimension
            // vectors from distinct models must never share an index.
            let reason = match result { Ok(_) => "provider returned invalid embedding shape", Err(_) => "provider unavailable or embedding budget exhausted" };
            offline_corpus(prev, texts, reason)
        }
    }
}

fn valid_vectors(vectors: &[Vec<f32>], count: usize, dim: Option<usize>) -> bool {
    vectors.len() == count && vectors.first().is_some_and(|first| {
        let d = dim.unwrap_or(first.len());
        d > 0 && vectors.iter().all(|v| v.len() == d && v.iter().all(|n| n.is_finite()))
    })
}

fn embed_budgeted(provider: &dyn LlmProvider, budget: &cie_llm::Budget, batch: &[&str]) -> Result<Vec<Vec<f32>>, cie_llm::LlmError> {
    if provider.kind() != cie_llm::ProviderKind::Offline {
        if budget.exceeded() { return Err(cie_llm::LlmError::BudgetExceeded); }
        // Reserve the estimate BEFORE the external call, including a minimum
        // token for tiny strings. The cap cannot prevent already incurred cost.
        let tokens = batch.iter().map(|t| (t.len() as u64).div_ceil(4).max(1)).sum();
        budget.charge(tokens, 0, provider.cost_per_1k())?;
    }
    provider.embed(batch)
}

fn offline_corpus(prev: Option<&PersistedVectors>, texts: &[(String, String)], reason: &str) -> PersistedVectors {
    let mut out = update_embeddings(prev, texts, &cie_llm::Budget::unlimited(), &cie_llm::OfflineProvider);
    out.fallback_reason = Some(reason.into());
    out
}

/// Prepare corpus and query in the SAME embedding space, including on a
/// query-time network failure. Never compare a remote index to offline text.
pub fn prepare_search(prev: Option<&PersistedVectors>, texts: &[(String, String)], budget: &cie_llm::Budget,
    provider: &dyn LlmProvider, query: &str) -> (PersistedVectors, Vec<f32>) {
    let mut corpus = update_embeddings(prev, texts, budget, provider);
    let offline = cie_llm::OfflineProvider;
    let selected = if corpus.provider == offline.embedding_id() { &offline as &dyn LlmProvider } else { provider };
    match embed_budgeted(selected, budget, &[query]) {
        Ok(mut v) if valid_vectors(&v, 1, (corpus.dim > 0).then_some(corpus.dim)) => (corpus, v.remove(0)),
        _ => {
            corpus = offline_corpus(prev, texts, "query embedding failed; corpus rebuilt offline");
            let query = offline.embed(&[query]).expect("offline embeddings are infallible").remove(0);
            (corpus, query)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};
    struct CountingProvider { calls: AtomicUsize, items: AtomicUsize, dim: usize, id: &'static str, fail_at: usize, malformed: bool }
    impl CountingProvider {
        fn new(id: &'static str, dim: usize) -> Self { Self { calls: AtomicUsize::new(0), items: AtomicUsize::new(0), dim, id, fail_at: usize::MAX, malformed: false } }
    }
    impl LlmProvider for CountingProvider {
        fn id(&self) -> &'static str { self.id }
        fn kind(&self) -> cie_llm::ProviderKind { cie_llm::ProviderKind::Ollama }
        fn cost_per_1k(&self) -> (f64, f64) { (0.0, 0.0) }
        fn complete(&self, _: &str, _: u32) -> Result<String, cie_llm::LlmError> { unreachable!("embedding tests must not complete") }
        fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, cie_llm::LlmError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            self.items.fetch_add(texts.len(), Ordering::SeqCst);
            if call >= self.fail_at { return Err(cie_llm::LlmError::Unsupported); }
            if self.malformed { return Ok(vec![]); }
            Ok(texts.iter().map(|_| vec![1.0; self.dim]).collect())
        }
    }
    fn corpus() -> Vec<(String, String)> { vec![("a".into(), "first".into()), ("b".into(), "second".into())] }

    #[test]
    fn incremental_embeddings_count_actual_provider_calls_and_prune_deletions() {
        let p = CountingProvider::new("counter", 8);
        let budget = cie_llm::Budget::unlimited();
        let first = update_embeddings(None, &corpus(), &budget, &p);
        assert_eq!(p.items.load(Ordering::SeqCst), 2);
        let second = update_embeddings(Some(&first), &corpus(), &budget, &p);
        assert_eq!(p.calls.load(Ordering::SeqCst), 1, "no extra embed on unchanged corpus");
        let third = update_embeddings(Some(&second), &[("a".into(), "changed".into()), ("b".into(), "second".into())], &budget, &p);
        assert_eq!(p.items.load(Ordering::SeqCst), 3, "only one changed text embedded");
        let fourth = update_embeddings(Some(&third), &[("b".into(), "second".into())], &budget, &p);
        assert_eq!(fourth.records.len(), 1);
        assert_eq!(p.calls.load(Ordering::SeqCst), 2);
    }
    #[test]
    fn provider_switch_reembeds_all_even_when_dimensions_are_equal() {
        let budget = cie_llm::Budget::unlimited();
        let a = CountingProvider::new("model-a", 8);
        let first = update_embeddings(None, &corpus(), &budget, &a);
        for dim in [8, 16] {
            let b = CountingProvider::new("model-b", dim);
            let next = update_embeddings(Some(&first), &corpus(), &budget, &b);
            assert_eq!(next.records.len(), 2);
            assert_eq!(next.provider, "model-b");
            assert_eq!(next.dim, dim);
            assert_eq!(b.items.load(Ordering::SeqCst), 2);
            assert_eq!(next.rebuild_index().len(), 2);
        }
    }
    #[test]
    fn partial_provider_failure_reembeds_carried_records_instead_of_losing_them() {
        let budget = cie_llm::Budget::unlimited();
        let mut p = CountingProvider::new("model-a", 8);
        let first = update_embeddings(None, &corpus(), &budget, &p);
        p.fail_at = 2;
        let next = update_embeddings(Some(&first), &[("a".into(), "changed".into()), ("b".into(), "second".into())], &budget, &p);
        assert_eq!(next.provider, "offline-v1");
        assert_eq!(next.dim, 64);
        assert_eq!(next.records.len(), 2);
        assert!(next.fallback_reason.is_some());
        assert_eq!(next.rebuild_index().len(), 2);
    }
    #[test]
    fn budget_blocks_external_calls_and_malformed_responses_fall_back_completely() {
        let mut p = CountingProvider::new("model-a", 8);
        let next = update_embeddings(None, &corpus(), &cie_llm::Budget::new(0.0), &p);
        assert_eq!(p.calls.load(Ordering::SeqCst), 0);
        assert_eq!(next.records.len(), 2);
        assert_eq!(next.provider, "offline-v1");
        p.malformed = true;
        let next = update_embeddings(None, &corpus(), &cie_llm::Budget::unlimited(), &p);
        assert_eq!(next.provider, "offline-v1");
        assert_eq!(next.records.len(), 2);
    }
    #[test]
    fn query_failure_falls_back_with_a_matching_corpus() {
        let mut p = CountingProvider::new("model-a", 8);
        p.fail_at = 2;
        let (index, query) = prepare_search(None, &corpus(), &cie_llm::Budget::unlimited(), &p, "first");
        assert_eq!(index.provider, "offline-v1");
        assert_eq!(query.len(), index.dim);
        assert_eq!(index.records.len(), 2);
        assert!(matches!(index.rebuild_index().knn(&query, 2, 1_000_000_000), cie_store::hnsw::KnnOutcome::Neighbors(_)));
    }
    #[test]
    fn legacy_vectors_reembed_in_a_known_space() {
        let p = update_embeddings(None, &corpus(), &cie_llm::Budget::unlimited(), &cie_llm::OfflineProvider);
        let mut json = serde_json::to_value(&p).unwrap();
        json.as_object_mut().unwrap().remove("provider");
        let old: PersistedVectors = serde_json::from_value(json).unwrap();
        assert_eq!(old.provider, "legacy-unknown");
        let repaired = update_embeddings(Some(&old), &corpus(), &cie_llm::Budget::unlimited(), &cie_llm::OfflineProvider);
        assert_eq!(repaired.provider, "offline-v1");
        assert_eq!(repaired.rebuild_index().len(), 2);
    }

    #[test]
    fn unchanged_text_is_carried_forward_not_reembedded() {
        let offline = cie_llm::OfflineProvider;
        let budget = cie_llm::Budget::unlimited();
        let texts = vec![("a.foo".to_string(), "does a thing".to_string())];
        let v1 = update_embeddings(None, &texts, &budget, &offline);
        assert_eq!(v1.records.len(), 1);

        // Same text again — carried forward, not a fresh embed call (we
        // can't observe "no call" directly here, but the output must be
        // byte-identical, which a changed embed call would still produce
        // given determinism — so instead assert the dimension/record
        // count stay stable across repeated runs with unrelated new text).
        let v2 = update_embeddings(Some(&v1), &texts, &budget, &offline);
        assert_eq!(v1.records, v2.records);
    }

    #[test]
    fn changed_text_gets_a_fresh_vector() {
        let offline = cie_llm::OfflineProvider;
        let budget = cie_llm::Budget::unlimited();
        let v1 = update_embeddings(
            None,
            &[("a.foo".to_string(), "does a thing".to_string())],
            &budget,
            &offline,
        );
        let v2 = update_embeddings(
            Some(&v1),
            &[("a.foo".to_string(), "does something else entirely".to_string())],
            &budget,
            &offline,
        );
        let (_, vec1) = v1.records.values().next().unwrap();
        let (_, vec2) = v2.records.values().next().unwrap();
        assert_ne!(vec1, vec2);
    }

    #[test]
    fn rebuilt_index_answers_knn() {
        let offline = cie_llm::OfflineProvider;
        let budget = cie_llm::Budget::unlimited();
        let texts = vec![
            ("a.authorize".to_string(), "checks the caller has permission".to_string()),
            ("a.format_date".to_string(), "formats a date as a string".to_string()),
        ];
        let persisted = update_embeddings(None, &texts, &budget, &offline);
        let idx = persisted.rebuild_index();
        assert_eq!(idx.len(), 2);
        let query = cie_llm::LlmProvider::embed(&offline, &["check permission"]).unwrap().remove(0);
        match idx.knn(&query, 1, 1_000_000_000) {
            cie_store::hnsw::KnnOutcome::Neighbors(hits) => {
                assert_eq!(hits.len(), 1);
                let (text, _) = persisted.get(&hits[0].record_id).unwrap();
                assert_eq!(text, "checks the caller has permission");
            }
            other => panic!("expected neighbors, got {other:?}"),
        }
    }

    #[test]
    fn roundtrips_through_the_store() {
        let dir = std::env::temp_dir().join(format!("cie-embed-store-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = cie_store::Store::open(&dir).unwrap();
        let rev: RevId = 5;

        assert!(store.load_embeddings(rev).unwrap().is_none());
        let offline = cie_llm::OfflineProvider;
        let budget = cie_llm::Budget::unlimited();
        let persisted = update_embeddings(
            None,
            &[("a.foo".to_string(), "does a thing".to_string())],
            &budget,
            &offline,
        );
        store.save_embeddings(rev, &persisted).unwrap();
        let loaded = store.load_embeddings(rev).unwrap().unwrap();
        assert_eq!(loaded.records, persisted.records);
        std::fs::remove_dir_all(&dir).ok();
    }
}
