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
        // Structured prompts understood offline (matched on the FULL
        // prompt — an earlier splitn(2, ':') consumed the prefix before
        // the branch checked it, so every structured prompt silently fell
        // through to the catch-all and labels came out mangled, e.g.
        // "Concept:money:money movement"):
        //   LABEL:<qual_name>:<kind>                    -> prettified qual
        //   LABEL:concept:<intent>:<fallback>[FUNCTIONS:...] -> the
        //                                                  mechanical fallback
        //   EXPLAIN:<qual_name>:<effects comma-list>     -> one-line explanation
        // Anything else echoes a deterministic summary.
        if let Some(rest) = prompt.strip_prefix("LABEL:concept:") {
            let head = rest.split("\nFUNCTIONS:").next().unwrap_or(rest);
            let fallback = head.rsplit_once(':').map(|(_, f)| f).unwrap_or(head);
            return Ok(fallback.trim().to_string());
        }
        if let Some(rest) = prompt.strip_prefix("LABEL:") {
            let qual = rest.rsplit_once(':').map(|(q, _)| q).unwrap_or(rest);
            return Ok(prettify(qual));
        }
        if let Some(rest) = prompt.strip_prefix("EXPLAIN:") {
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
        Ok(prettify(prompt))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concept_labels_read_the_mechanical_fallback_not_the_catch_all() {
        let p = OfflineProvider;
        let prompt = "LABEL:concept:money+db_write:writes data & money movement\nFUNCTIONS:\n- transfer_money [money] (db_write)";
        assert_eq!(
            p.complete(prompt, 48).unwrap(),
            "writes data & money movement",
            "offline cannot read code — the honest label is the mechanical fallback"
        );
        assert_eq!(
            p.complete("LABEL:concept:charge:Cart", 48).unwrap(),
            "Cart"
        );
    }

    #[test]
    fn symbol_labels_lose_the_kind_suffix_and_explain_parses() {
        let p = OfflineProvider;
        // the prefix-eating bug used to yield "Cart Add Item:method"
        assert_eq!(
            p.complete("LABEL:cart.add_item:method", 32).unwrap(),
            "Cart Add Item"
        );
        assert_eq!(
            p.complete("EXPLAIN:cart.total:db_read,db_write", 128).unwrap(),
            "Cart Total interacts with: db_read, db_write."
        );
        assert_eq!(
            p.complete("EXPLAIN:cart.total:", 128).unwrap(),
            "Cart Total performs no externally visible effects."
        );
        // the catch-all still exists for unstructured prompts
        assert!(p.complete("free form thought", 16).is_ok());
    }
}
