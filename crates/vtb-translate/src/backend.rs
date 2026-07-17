//! LLM backends. A small trait so the pipeline can use Anthropic, any
//! OpenAI-compatible endpoint (OpenAI/DeepSeek/Ollama/vLLM...), or a mock.

use crate::error::{Result, TranslateError};
use async_trait::async_trait;
use serde_json::json;

#[async_trait]
pub trait LlmBackend: Send + Sync {
    /// Complete a single translation request; returns the raw text output.
    async fn complete(&self, system: &str, user: &str) -> Result<String>;
}

/// Anthropic Messages API backend.
pub struct AnthropicBackend {
    http: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl AnthropicBackend {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key: api_key.into(),
            model: model.into(),
            base_url: "https://api.anthropic.com".into(),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

#[async_trait]
impl LlmBackend for AnthropicBackend {
    async fn complete(&self, system: &str, user: &str) -> Result<String> {
        let resp = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&json!({
                "model": self.model,
                "max_tokens": 512,
                "system": system,
                "messages": [{"role": "user", "content": user}],
            }))
            .send()
            .await?;

        let status = resp.status().as_u16();
        let body = resp.text().await?;
        if status >= 400 {
            return Err(TranslateError::Api { status, body });
        }
        let v: serde_json::Value = serde_json::from_str(&body)?;
        let text = v["content"][0]["text"]
            .as_str()
            .ok_or(TranslateError::EmptyResponse)?
            .trim()
            .to_string();
        if text.is_empty() {
            return Err(TranslateError::EmptyResponse);
        }
        Ok(text)
    }
}

/// OpenAI-compatible chat completions backend.
pub struct OpenAiCompatBackend {
    http: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenAiCompatBackend {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key: api_key.into(),
            model: model.into(),
            base_url: base_url.into(),
        }
    }
}

#[async_trait]
impl LlmBackend for OpenAiCompatBackend {
    async fn complete(&self, system: &str, user: &str) -> Result<String> {
        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&json!({
                "model": self.model,
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": user},
                ],
                "max_tokens": 512,
            }))
            .send()
            .await?;

        let status = resp.status().as_u16();
        let body = resp.text().await?;
        if status >= 400 {
            return Err(TranslateError::Api { status, body });
        }
        let v: serde_json::Value = serde_json::from_str(&body)?;
        let text = v["choices"][0]["message"]["content"]
            .as_str()
            .ok_or(TranslateError::EmptyResponse)?
            .trim()
            .to_string();
        if text.is_empty() {
            return Err(TranslateError::EmptyResponse);
        }
        Ok(text)
    }
}

/// Test/mock backend (also used by downstream crates' tests).
pub mod mock {
    use super::*;
    use std::sync::Mutex;

    /// Test backend: records calls, returns scripted or echo responses.
    pub struct MockBackend {
        pub calls: Mutex<Vec<(String, String)>>,
        pub responses: Mutex<Vec<Result<String>>>,
    }

    impl MockBackend {
        pub fn echo() -> Self {
            Self {
                calls: Mutex::new(vec![]),
                responses: Mutex::new(vec![]),
            }
        }

        pub fn scripted(responses: Vec<Result<String>>) -> Self {
            Self {
                calls: Mutex::new(vec![]),
                responses: Mutex::new(responses),
            }
        }
    }

    #[async_trait]
    impl LlmBackend for MockBackend {
        async fn complete(&self, system: &str, user: &str) -> Result<String> {
            self.calls
                .lock()
                .unwrap()
                .push((system.to_string(), user.to_string()));
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                // Echo mode: return "T(<last line>)".
                let last = user.lines().last().unwrap_or("").to_string();
                Ok(format!("T({last})"))
            } else {
                responses.remove(0)
            }
        }
    }
}
