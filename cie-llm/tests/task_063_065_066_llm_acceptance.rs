//! TASK-063/065/066 validation — LLM acceptance trio (PRD §13 items 12-14).
//! Adaptations per docs/TRACEABILITY.md: the sketch's
//! `Label { source: Llm, model_id, prompt_hash }` maps to `LabelRecord`
//! (`source`/`model_id` strings + the blake3 `cache_key` as prompt hash).

use cie_llm::LabelCache;
use cie_llm::{cache_key, Budget, Labeler, LlmError, LlmProvider, OfflineProvider, ProviderKind};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Provider that FAILS the test if the engine calls it — used to prove the
/// budget-exceeded path never reaches the model.
struct MustNotCall;
impl LlmProvider for MustNotCall {
    fn id(&self) -> &'static str {
        "must-not-call"
    }
    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenAi
    }
    fn complete(&self, _prompt: &str, _max_tokens: u32) -> Result<String, LlmError> {
        panic!("acc #14: budget exceeded must downgrade BEFORE any model call");
    }
    fn embed(&self, _texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
        panic!("no network on the downgrade path");
    }
    fn cost_per_1k(&self) -> (f64, f64) {
        (10.0, 30.0)
    }
}

/// Provider counting completions (cache-hit proof for acc #13).
struct CountingProvider {
    calls: AtomicUsize,
}
impl LlmProvider for CountingProvider {
    fn id(&self) -> &'static str {
        "counting-v1"
    }
    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenAi // a cloud stand-in: successful completions are "llm" labels
    }
    fn complete(&self, _p: &str, _m: u32) -> Result<String, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok("Authorize user".to_string())
    }
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
        Ok(texts.iter().map(|t| vec![t.len() as f32]).collect())
    }
    fn cost_per_1k(&self) -> (f64, f64) {
        (0.0, 0.0)
    }
}

/// acc #12 (TASK-063): offline labeling — deterministic labels with zero
/// network, provenance recorded (`source`, `model_id`, prompt hash key).
#[test]
fn task_063_acc12_offline_labeling_no_network() {
    let provider = OfflineProvider::default();
    assert_eq!(provider.kind(), ProviderKind::Offline);
    // No network by construction: complete() is pure string work.
    let label = provider.complete("LABEL:auth.authorize:fn", 32).unwrap();
    assert!(!label.is_empty());

    let mut cache = LabelCache::default();
    let budget = Budget::unlimited();
    let mut labeler = Labeler::new(&provider, &mut cache, &budget);
    let rec = labeler.label_symbol("auth.authorize", "fn");
    assert_eq!(rec.source, "offline", "FR-4 provenance: source recorded");
    assert_eq!(rec.model_id, "offline-v1", "model_id recorded");
    assert!(!rec.label.is_empty());

    // The prompt hash (cache_key) is the content address of the record.
    let key = cie_core::types::hash_hex(&cache_key("offline-v1", "LABEL:auth.authorize:fn"));
    assert!(
        cache.entries.contains_key(&key),
        "label cached under blake3(model+prompt)"
    );
}

/// acc #13 (TASK-065): cache hits do no model call; the hot path is the
/// blake3 cache_key (perf gate below).
#[test]
fn task_065_acc13_cache_hit_skips_model() {
    let provider = CountingProvider {
        calls: AtomicUsize::new(0),
    };
    let mut cache = LabelCache::default();
    let budget = Budget::unlimited();
    let mut labeler = Labeler::new(&provider, &mut cache, &budget);

    let first = labeler.label_symbol("cart.total", "fn");
    assert_eq!(first.source, "llm");
    let second = labeler.label_symbol("cart.total", "fn");
    assert_eq!(second.source, "cache", "second call is a cache hit");
    assert_eq!(
        provider.calls.load(Ordering::SeqCst),
        1,
        "cache hit does no model call"
    );
    assert_eq!(first.label, second.label);
}

/// acc #13 perf gate: cache_key(model, prompt) ≤ 100 ns × CI_TOLERANCE.
#[test]
#[ignore = "perf: run with `cargo test -- --ignored`"]
fn task_065_acc13_cache_key_perf() {
    if cfg!(debug_assertions) {
        // §15.4 targets bind on optimized builds; perf gates run with
        // `cargo test --release -- --ignored` (perf convention, TASK-004).
        eprintln!("perf gate skipped on debug build");
        return;
    }
    let mut sink = 0u64;
    let samples = 10_000;
    let t0 = std::time::Instant::now();
    for i in 0..samples {
        let key = cache_key("offline-v1", &format!("LABEL:auth.authorize:{i}"));
        sink ^= key[0] as u64;
    }
    let per_call = t0.elapsed().as_nanos() as f64 / samples as f64;
    std::hint::black_box(sink);
    assert!(
        per_call <= 100.0 * cie_core::CI_TOLERANCE,
        "§13 #13: cache hit path ≤ 100 ns × CI_TOLERANCE, measured {per_call:.1} ns"
    );
}

/// acc #14 (TASK-066): budget exceeded → downgrade-and-continue — the run
/// completes with the rule-based fallback and never hard-fails.
#[test]
fn task_066_acc14_budget_exceeded_downgrades_and_continues() {
    let budget = Budget::new(0.0); // zero budget → immediately exceeded
    assert!(budget.exceeded());

    // The provider would panic if called; the downgrade must prevent that.
    let provider = MustNotCall;
    let mut cache = LabelCache::default();
    let mut labeler = Labeler::new(&provider, &mut cache, &budget);

    // Runs to completion (no panic, no hard failure): rule-based fallback.
    let rec = labeler.label_concept(&["charge".into(), "money".into()], "Cart");
    assert_eq!(rec.source, "offline", "downgraded to offline provider");
    assert_eq!(rec.model_id, "offline-v1");
    assert!(!rec.label.is_empty(), "fallback label present");

    // Budget charge refuses past the cap (no silent overrun).
    assert!(matches!(
        budget.charge(10_000, 10_000, (10.0, 30.0)),
        Err(LlmError::BudgetExceeded)
    ));
}
