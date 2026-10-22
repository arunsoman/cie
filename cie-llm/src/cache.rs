//! Content-addressed labeler with persistent cache (PRD §63, acceptance
//! #13: cache hits do no model call). All labels record provenance; the
//! LLM can never set contract authority.

use crate::persist::{CachedLabel, LabelCache};
use crate::{cache_key, LlmError, LlmProvider};
use cie_core::types::Ns;

/// One member function of the concept being labeled — the evidence an LLM
/// may read so it can name the STORY ("Transfer funds between accounts")
/// instead of joining attribute words ("writes data & money movement").
/// Mirrors cie-analysis's `LabelSymbolEvidence` (no dependency edge
/// cie-analysis → cie-llm; cie-ingest converts at the boundary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptEvidence {
    pub name: String,
    pub tags: Vec<String>,
    pub effects: Vec<String>,
}

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

    /// A3: the concept labeler sees the cluster's members — names, tags,
    /// effects — so a real provider can name the story instead of joining
    /// tag words. `evidence` is caller-bounded (hierarchy caps at 12
    /// members, extent order, deterministic); the offline provider reads
    /// only the mechanical fallback and ignores FUNCTIONS.
    pub fn label_concept(
        &mut self,
        intent: &[String],
        fallback: &str,
        evidence: &[ConceptEvidence],
    ) -> LabelRecord {
        let mut prompt = format!("LABEL:concept:{}:{fallback}", intent.join("+"));
        if !evidence.is_empty() {
            prompt.push_str("\nFUNCTIONS:");
            for e in evidence {
                prompt.push_str(&format!("\n- {}", e.name));
                if !e.tags.is_empty() {
                    prompt.push_str(&format!(" [{}]", e.tags.join(", ")));
                }
                if !e.effects.is_empty() {
                    prompt.push_str(&format!(" ({})", e.effects.join(", ")));
                }
            }
        }
        self.run(&prompt, 48)
    }

    pub fn explain(&mut self, qual_name: &str, effects: &[String]) -> LabelRecord {
        let prompt = format!("EXPLAIN:{qual_name}:{}", effects.join(","));
        self.run(&prompt, 128)
    }

    fn run(&mut self, prompt: &str, max_tokens: u32) -> LabelRecord {
        // Label-parse namespace: offline labels are parsed by the offline
        // provider itself, and the parser was rewritten (the old splitn
        // ate the LABEL:/EXPLAIN: prefix before the branch checked it, so
        // every structured prompt fell through to the catch-all). The
        // namespace bump invalidates cached labels produced by the broken
        // parse WITHOUT touching the embedding provider id (vector_store
        // pins "offline-v1" separately).
        let model_ns: &str = if self.provider.kind() == crate::ProviderKind::Offline {
            "offline-v2"
        } else {
            self.provider.id()
        };
        let key = cache_key(model_ns, prompt);
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
                        // provenance records the namespace the label was
                        // produced under (offline parser v2 vs the raw
                        // provider id) so cache entries stay self-describing
                        model_id: model_ns.to_string(),
                        source: source.into(),
                        created_ns: now,
                    },
                );
                LabelRecord {
                    label,
                    model_id: model_ns,
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
        let key = cache_key("offline-v2", prompt);
        let key_hex = cie_core::types::hash_hex(&key);
        self.cache.entries.insert(
            key_hex,
            CachedLabel {
                label: label.clone(),
                model_id: "offline-v2".into(),
                source: "offline".into(),
                created_ns: now,
            },
        );
        LabelRecord {
            label,
            model_id: "offline-v2",
            source: "offline",
            created_ns: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A capturing provider: asserts the enriched prompt reaches the model.
    struct Capture(std::sync::Mutex<Vec<String>>);
    impl LlmProvider for Capture {
        fn id(&self) -> &'static str {
            "capture-v1"
        }
        fn kind(&self) -> crate::ProviderKind {
            crate::ProviderKind::OpenAi
        }
        fn complete(&self, prompt: &str, _max: u32) -> Result<String, LlmError> {
            self.0.lock().unwrap().push(prompt.to_string());
            Ok("Transfer funds between accounts".into())
        }
        fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
            Ok(texts.iter().map(|t| vec![t.len() as f32]).collect())
        }
        fn cost_per_1k(&self) -> (f64, f64) {
            (0.0, 0.0)
        }
    }

    #[test]
    fn concept_prompt_carries_the_function_evidence() {
        let provider = Capture(std::sync::Mutex::new(Vec::new()));
        let mut cache = LabelCache::default();
        let budget = crate::Budget::unlimited();
        let mut labeler = Labeler::new(&provider, &mut cache, &budget);
        let rec = labeler.label_concept(
            &["money".into(), "db_write".into()],
            "writes data & money movement",
            &[
                ConceptEvidence {
                    name: "transferMoney".into(),
                    tags: vec!["auth".into(), "money".into()],
                    effects: vec!["db_write".into()],
                },
                ConceptEvidence {
                    name: "updateLedger".into(),
                    tags: vec!["money".into()],
                    effects: vec![],
                },
            ],
        );
        assert_eq!(rec.label, "Transfer funds between accounts");
        assert_eq!(rec.source, "llm");
        let prompts = provider.0.lock().unwrap();
        let p = &prompts[0];
        assert!(
            p.starts_with("LABEL:concept:money+db_write:writes data & money movement"),
            "intent + fallback head: {p}"
        );
        assert!(p.contains("FUNCTIONS:"), "the evidence section: {p}");
        assert!(p.contains("- transferMoney [auth, money] (db_write)"), "{p}");
        assert!(p.contains("- updateLedger [money]"), "{p}");
    }

    #[test]
    fn evidence_free_concept_prompts_keep_the_old_shape() {
        let provider = Capture(std::sync::Mutex::new(Vec::new()));
        let mut cache = LabelCache::default();
        let budget = crate::Budget::unlimited();
        let mut labeler = Labeler::new(&provider, &mut cache, &budget);
        let _ = labeler.label_concept(&["charge".into()], "Cart", &[]);
        let p = provider.0.lock().unwrap()[0].clone();
        assert_eq!(p, "LABEL:concept:charge:Cart");
        assert!(!p.contains("FUNCTIONS:"), "no empty evidence section: {p}");
    }
}
