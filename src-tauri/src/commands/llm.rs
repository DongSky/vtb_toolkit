//! LLM backend configuration resolution.
//!
//! Every field can come from (in priority order): explicit request options
//! → persisted settings (`settings.json["llm"]`) → environment variables →
//! OS keychain (key only) → defaults. This lets users point the translator
//! at any OpenAI-compatible endpoint (自建/中转/官方) with their own key.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LlmSettings {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLlm {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub base_url: Option<String>,
}

fn non_empty(v: Option<String>) -> Option<String> {
    v.filter(|s| !s.trim().is_empty())
}

/// Resolve the effective LLM config. `settings` is the parsed `"llm"`
/// object from settings.json (or Null). `keychain_key` is the stored
/// secret, passed in for testability.
pub fn resolve_llm(
    provider: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
    settings: &serde_json::Value,
    env: &dyn Fn(&str) -> Option<String>,
    keychain_key: Option<String>,
) -> Result<ResolvedLlm, String> {
    let saved: LlmSettings =
        serde_json::from_value(settings.clone()).unwrap_or_default();

    let provider = non_empty(provider)
        .or_else(|| non_empty(saved.provider.clone()))
        .unwrap_or_else(|| "anthropic".into());

    let base_url = non_empty(base_url)
        .or_else(|| non_empty(saved.base_url.clone()))
        .or_else(|| match provider.as_str() {
            "openai" => env("OPENAI_BASE_URL"),
            _ => env("ANTHROPIC_BASE_URL"),
        })
        .and_then(|u| non_empty(Some(u)));

    let model = non_empty(model)
        .or_else(|| non_empty(saved.model.clone()))
        .or_else(|| env("OPENAI_MODEL").filter(|_| provider == "openai"))
        .unwrap_or_else(|| match provider.as_str() {
            "openai" => "gpt-4o-mini".into(),
            _ => "claude-haiku-4-5".into(),
        });

    let api_key = non_empty(api_key)
        .or_else(|| match provider.as_str() {
            "openai" => env("OPENAI_API_KEY"),
            _ => env("ANTHROPIC_API_KEY").or_else(|| env("OPENAI_API_KEY")),
        })
        .or_else(|| non_empty(keychain_key))
        .ok_or("未配置 LLM API key（面板/钥匙串/环境变量均未找到）")?;

    Ok(ResolvedLlm {
        provider,
        api_key,
        model,
        base_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn explicit_options_win() {
        let settings = serde_json::json!({
            "provider": "anthropic", "base_url": "https://saved", "model": "saved-model"
        });
        let r = resolve_llm(
            Some("openai".into()),
            Some("k-explicit".into()),
            Some("m-explicit".into()),
            Some("https://explicit".into()),
            &settings,
            &no_env,
            Some("k-keychain".into()),
        )
        .unwrap();
        assert_eq!(r.provider, "openai");
        assert_eq!(r.api_key, "k-explicit");
        assert_eq!(r.model, "m-explicit");
        assert_eq!(r.base_url.as_deref(), Some("https://explicit"));
    }

    #[test]
    fn settings_fill_missing_fields() {
        let settings = serde_json::json!({
            "provider": "openai", "base_url": "https://relay.example/v1", "model": "gpt-x"
        });
        let r = resolve_llm(None, None, None, None, &settings, &no_env, Some("kk".into()))
            .unwrap();
        assert_eq!(r.provider, "openai");
        assert_eq!(r.base_url.as_deref(), Some("https://relay.example/v1"));
        assert_eq!(r.model, "gpt-x");
        assert_eq!(r.api_key, "kk");
    }

    #[test]
    fn env_base_url_fallback_per_provider() {
        let env = |k: &str| match k {
            "OPENAI_BASE_URL" => Some("https://env-openai/v1".to_string()),
            "OPENAI_API_KEY" => Some("env-key".to_string()),
            _ => None,
        };
        let r = resolve_llm(
            Some("openai".into()),
            None,
            None,
            None,
            &serde_json::Value::Null,
            &env,
            None,
        )
        .unwrap();
        assert_eq!(r.base_url.as_deref(), Some("https://env-openai/v1"));
        assert_eq!(r.api_key, "env-key");

        // Anthropic provider does not pick up OPENAI_BASE_URL.
        let r2 = resolve_llm(
            Some("anthropic".into()),
            Some("k".into()),
            None,
            None,
            &serde_json::Value::Null,
            &env,
            None,
        )
        .unwrap();
        assert_eq!(r2.base_url, None);
        assert_eq!(r2.model, "claude-haiku-4-5");
    }

    #[test]
    fn empty_strings_treated_as_unset() {
        let r = resolve_llm(
            Some("".into()),
            Some("k".into()),
            Some("  ".into()),
            Some("".into()),
            &serde_json::Value::Null,
            &no_env,
            None,
        )
        .unwrap();
        assert_eq!(r.provider, "anthropic");
        assert_eq!(r.model, "claude-haiku-4-5");
        assert_eq!(r.base_url, None);
    }

    #[test]
    fn missing_key_errors() {
        let err =
            resolve_llm(None, None, None, None, &serde_json::Value::Null, &no_env, None)
                .unwrap_err();
        assert!(err.contains("API key"));
    }
}
