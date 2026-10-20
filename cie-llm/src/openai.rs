//! OpenAI-compatible provider (any endpoint serving /v1/chat/completions:
//! OpenAI, Azure OpenAI gateways, vLLM, llama.cpp server, ...).
//! Opt-in via OPENAI_API_KEY (+ optional OPENAI_BASE_URL, OPENAI_MODEL).

use crate::{LlmError, LlmProvider, ProviderKind};
use serde_json::json;

pub struct OpenAiProvider {
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAiProvider {
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("OPENAI_API_KEY").ok()?;
        Some(OpenAiProvider {
            base_url: std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com".into())
                .trim_end_matches('/')
                .to_string(),
            api_key,
            model: std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into()),
        })
    }
}

impl LlmProvider for OpenAiProvider {
    fn id(&self) -> &'static str {
        "openai-compatible"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenAi
    }

    fn embedding_id(&self) -> String {
        cie_core::types::hash_hex(&cie_core::types::hash((format!("openai:{}:{}", self.base_url, std::env::var("OPENAI_EMBED_MODEL").unwrap_or_else(|_| "text-embedding-3-small".into()))).as_bytes()))
    }

    fn complete(&self, prompt: &str, max_tokens: u32) -> Result<String, LlmError> {
        let resp = ureq::post(&format!("{}/v1/chat/completions", self.base_url))
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .timeout(std::time::Duration::from_secs(120))
            .send_json(json!({
                "model": self.model,
                "messages": [
                    { "role": "system", "content": "You label and explain code facts. Never invent facts; answer tersely." },
                    { "role": "user", "content": prompt }
                ],
                "max_tokens": max_tokens,
                "temperature": 0.0
            }))
            .map_err(|e| LlmError::Network(e.to_string()))?;
        let body: serde_json::Value = resp
            .into_json()
            .map_err(|e| LlmError::Network(e.to_string()))?;
        Ok(body
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string())
    }

    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, LlmError> {
        let resp = ureq::post(&format!("{}/v1/embeddings", self.base_url))
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .timeout(std::time::Duration::from_secs(120))
            .send_json(json!({
                "model": std::env::var("OPENAI_EMBED_MODEL").unwrap_or_else(|_| "text-embedding-3-small".into()),
                "input": texts
            }))
            .map_err(|e| LlmError::Network(e.to_string()))?;
        let body: serde_json::Value = resp
            .into_json()
            .map_err(|e| LlmError::Network(e.to_string()))?;
        let out = body
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|item| {
                        item.get("embedding")
                            .and_then(|e| e.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|x| x.as_f64().map(|f| f as f32))
                                    .collect()
                            })
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(out)
    }

    fn cost_per_1k(&self) -> (f64, f64) {
        // Conservative public pricing hint for budget accounting.
        match self.model.as_str() {
            "gpt-4o" => (0.0025, 0.01),
            "gpt-4o-mini" => (0.00015, 0.0006),
            _ => (0.0005, 0.0015),
        }
    }
}
