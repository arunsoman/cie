//! cie-llm — LLM integration (PRD Part IX).
//!
//! The LLM labels and explains; it NEVER decides membership, verification,
//! or truth. `Authority` cannot be set by an LLM (enforced by construction:
//! labelers return strings, not authorities). Default provider is offline
//! and deterministic; Ollama and OpenAI-compatible endpoints are opt-in;
//! everything is cached content-addressed by blake3(prompt+model+params)
//! and budget-capped.

pub mod budget;
pub mod readings;

pub use readings::{alternative_readings, apply_choice, ChosenReading, InterpretationLine, Reading, CONFIDENCE_FLOOR};
pub mod persist;

pub mod cache;
pub use persist::{CachedLabel, LabelCache, LabelStoreExt};

pub mod offline;
pub mod ollama;
pub mod openai;

pub use budget::Budget;
pub use cache::Labeler;
pub use offline::OfflineProvider;

use cie_core::types::Hash;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Offline,
    Ollama,
    OpenAi,
}

pub trait LlmProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn kind(&self) -> ProviderKind;
    /// Completion; `prompt` is a structured instruction.
    fn complete(&self, prompt: &str, max_tokens: u32) -> Result<String, LlmError>;
    /// Deterministic local embeddings when offline.
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError>;
    /// Rough cost per 1k tokens: (input_usd, output_usd).
    fn cost_per_1k(&self) -> (f64, f64);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmError {
    BudgetExceeded,
    Network(String),
    Unsupported,
}

/// Build the provider requested on the CLI. Never panics; falls back to
/// offline when the requested backend is not configured.
pub fn make_provider(kind: ProviderKind) -> Box<dyn LlmProvider> {
    match kind {
        ProviderKind::Offline => Box::new(OfflineProvider),
        ProviderKind::Ollama => Box::new(ollama::OllamaProvider::default()),
        ProviderKind::OpenAi => match openai::OpenAiProvider::from_env() {
            Some(p) => Box::new(p),
            None => Box::new(OfflineProvider),
        },
    }
}

/// Cache key: blake3(model + prompt + params) per PRD §63.
pub fn cache_key(model_id: &str, prompt: &str) -> Hash {
    cie_core::types::content_hash(format!("{model_id}\u{0}{prompt}").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_labels_are_deterministic() {
        let p = OfflineProvider::default();
        let a = p.complete("LABEL:cart.add_item:method", 32).unwrap();
        let b = p.complete("LABEL:cart.add_item:method", 32).unwrap();
        assert_eq!(a, b);
        assert!(!a.is_empty());
    }

    #[test]
    fn cache_key_stable() {
        let a = cache_key("offline-v1", "LABEL:x");
        let b = cache_key("offline-v1", "LABEL:x");
        assert_eq!(a, b);
        assert_ne!(a, cache_key("offline-v1", "LABEL:y"));
    }
}
