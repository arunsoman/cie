//! Deterministic offline provider (PRD §64 local-first, acceptance #12):
//! produces labels with zero network access.

use crate::{LlmError, LlmProvider, ProviderKind};

#[derive(Default)]
pub struct OfflineProvider;

fn prettify(symbol_like: &str) -> String {
    let mut out = String::new();
    for (i, part) in symbol_like.split(['_', '.', '-', '/']).enumerate() {
        if part.is_empty() {
            continue;
        }
        if i > 0 {
            out.push(' ');
        }
        let mut chars = part.chars();
        if let Some(c) = chars.next() {
            out.extend(c.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() {
        symbol_like.to_string()
    } else {
        out
    }
}

impl LlmProvider for OfflineProvider {
    fn id(&self) -> &'static str {
        "offline-v1"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Offline
    }

    fn complete(&self, prompt: &str, _max_tokens: u32) -> Result<String, LlmError> {
        // Structured prompts understood offline:
        //   LABEL:<qual_name>:<kind>  -> label
        //   EXPLAIN:<qual_name>:<effects comma-list> -> one-line explanation
        // Anything else echoes a deterministic summary.
        let parts: Vec<&str> = prompt.splitn(2, ':').collect();
        let body = parts.get(1).copied().unwrap_or(prompt);
        if body.starts_with("LABEL:") {
            let rest = body.trim_start_matches("LABEL:");
            let qual = rest.rsplit_once(':').map(|(q, _)| q).unwrap_or(rest);
            return Ok(prettify(qual));
        }
        if body.starts_with("EXPLAIN:") {
            let rest = body.trim_start_matches("EXPLAIN:");
            let mut it = rest.rsplitn(2, ':');
            let effects = it.next().unwrap_or("");
            let qual = it.next().unwrap_or("");
            let effect_list: Vec<&str> = effects.split(',').filter(|s| !s.is_empty()).collect();
            if effect_list.is_empty() {
                return Ok(format!(
                    "{} performs no externally visible effects.",
                    prettify(qual)
                ));
            }
            return Ok(format!(
                "{} interacts with: {}.",
                prettify(qual),
                effect_list.join(", ")
            ));
        }
        Ok(prettify(body))
    }

    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
        // Hashing-trick embeddings: deterministic, offline, 64-dim.
        const DIM: usize = 64;
        let mut out = vec![];
        for t in texts {
            let mut v = vec![0f32; DIM];
            for tok in t.split(|c: char| !c.is_ascii_alphanumeric()) {
                if tok.is_empty() {
                    continue;
                }
                let h = cie_core::types::content_hash(tok.to_lowercase().as_bytes());
                let idx = (h[0] as usize) % DIM;
                let idx2 = (h[1] as usize) % DIM;
                v[idx] += 1.0;
                v[idx2] += 0.5;
            }
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for x in &mut v {
                    *x /= norm;
                }
            }
            out.push(v);
        }
        Ok(out)
    }

    fn cost_per_1k(&self) -> (f64, f64) {
        (0.0, 0.0)
    }
}
