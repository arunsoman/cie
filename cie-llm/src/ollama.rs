//! Ollama provider (default local backend per PRD §64): talks to
//! `http://localhost:11434` using ureq. Offline-friendly: when Ollama is
//! unreachable, callers fall back to OfflineProvider.

use crate::{LlmError, LlmProvider, ProviderKind};
use serde_json::json;

pub struct OllamaProvider {
    base_url: String,
    model: String,
}

impl Default for OllamaProvider {
    fn default() -> Self {
        OllamaProvider {
            base_url: std::env::var("OLLAMA_HOST")
                .unwrap_or_else(|_| "http://localhost:11434".into()),
            model: std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2".into()),
        }
    }
}

impl LlmProvider for OllamaProvider {
    fn id(&self) -> &'static str {
        "ollama"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Ollama
    }

    fn complete(&self, prompt: &str, max_tokens: u32) -> Result<String, LlmError> {
        let resp = ureq::post(&format!("{}/api/generate", self.base_url))
            .timeout(std::time::Duration::from_secs(120))
            .send_json(json!({
                "model": self.model,
                "prompt": prompt,
                "stream": false,
                "options": { "num_predict": max_tokens }
            }))
            .map_err(|e| LlmError::Network(e.to_string()))?;
        let body: serde_json::Value = resp
            .into_json()
            .map_err(|e| LlmError::Network(e.to_string()))?;
        Ok(body
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string())
    }

    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
        let mut out = vec![];
        for t in texts {
            let resp = ureq::post(&format!("{}/api/embeddings", self.base_url))
                .timeout(std::time::Duration::from_secs(60))
                .send_json(json!({ "model": self.model, "prompt": t }))
                .map_err(|e| LlmError::Network(e.to_string()))?;
            let body: serde_json::Value = resp
                .into_json()
                .map_err(|e| LlmError::Network(e.to_string()))?;
            let v: Vec<f32> = body
                .get("embedding")
                .and_then(|e| e.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_f64().map(|f| f as f32))
                        .collect()
                })
                .unwrap_or_default();
            out.push(v);
        }
        Ok(out)
    }

    fn cost_per_1k(&self) -> (f64, f64) {
        (0.0, 0.0) // local inference is free in USD
    }
}
