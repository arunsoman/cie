//! Content-addressed labeler with persistent cache (PRD §63, acceptance
//! #13: cache hits do no model call). All labels record provenance; the
//! LLM can never set contract authority.

use crate::persist::{CachedLabel, LabelCache};
use crate::{cache_key, LlmError, LlmProvider};
use cie_core::types::Ns;

pub struct Labeler<'a> {
    provider: &'a dyn LlmProvider,
    cache: &'a mut LabelCache,
    budget: &'a crate::Budget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelRecord {
    pub label: String,
    pub model_id: &'static str,
    pub source: &'static str, // "cache" | "offline" | "llm"
    pub created_ns: Ns,
}

impl<'a> Labeler<'a> {
    pub fn new(
        provider: &'a dyn LlmProvider,
        cache: &'a mut LabelCache,
        budget: &'a crate::Budget,
    ) -> Labeler<'a> {
        Labeler {
            provider,
            cache,
            budget,
        }
    }

    pub fn label_symbol(&mut self, qual_name: &str, kind: &str) -> LabelRecord {
        let prompt = format!("LABEL:{qual_name}:{kind}");
        self.run(&prompt, 32)
    }

    pub fn label_concept(&mut self, intent: &[String], fallback: &str) -> LabelRecord {
        let prompt = format!("LABEL:concept:{}:{fallback}", intent.join("+"));
        self.run(&prompt, 48)
    }

    pub fn explain(&mut self, qual_name: &str, effects: &[String]) -> LabelRecord {
        let prompt = format!("EXPLAIN:{qual_name}:{}", effects.join(","));
        self.run(&prompt, 128)
    }

    fn run(&mut self, prompt: &str, max_tokens: u32) -> LabelRecord {
        let key = cache_key(self.provider.id(), prompt);
        let key_hex = cie_core::types::hash_hex(&key);
        let now = cie_core::types::now_ns();

        if let Some(cached) = self.cache.entries.get(&key_hex) {
            return LabelRecord {
                label: cached.label.clone(),
                model_id: "cached",
                source: "cache",
                created_ns: cached.created_ns,
            };
        }

        if self.budget.exceeded() {
            return self.offline_record(prompt, now);
        }

        // FR-4 provenance: an offline provider's output is NOT an LLM label —
        // it is the deterministic rule-based result (acc #12).
        let source: &str = if self.provider.kind() == crate::ProviderKind::Offline {
            "offline"
        } else {
            "llm"
        };
        match self.provider.complete(prompt, max_tokens) {
            Ok(label) if !label.is_empty() => {
                // charge ~ tokens/4 estimate
                let tokens = (prompt.len() as u64 / 4) + (label.len() as u64 / 4);
                let _ = self.budget.charge(tokens, 0, self.provider.cost_per_1k());
                self.cache.entries.insert(
                    key_hex,
                    CachedLabel {
                        label: label.clone(),
                        model_id: self.provider.id().to_string(),
                        source: source.into(),
                        created_ns: now,
                    },
                );
                LabelRecord {
                    label,
                    model_id: self.provider.id(),
                    source,
                    created_ns: now,
                }
            }
            Ok(_) | Err(LlmError::BudgetExceeded) | Err(LlmError::Network(_)) => {
                self.offline_record(prompt, now)
            }
            Err(LlmError::Unsupported) => self.offline_record(prompt, now),
        }
    }

    fn offline_record(&mut self, prompt: &str, now: Ns) -> LabelRecord {
        let offline = crate::OfflineProvider;
        let label = offline.complete(prompt, 64).unwrap_or_default();
        let key = cache_key("offline-v1", prompt);
        let key_hex = cie_core::types::hash_hex(&key);
        self.cache.entries.insert(
            key_hex,
            CachedLabel {
                label: label.clone(),
                model_id: "offline-v1".into(),
                source: "offline".into(),
                created_ns: now,
            },
        );
        LabelRecord {
            label,
            model_id: "offline-v1",
            source: "offline",
            created_ns: now,
        }
    }
}
