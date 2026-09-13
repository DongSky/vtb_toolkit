//! Provider settings shared by translation, vision and image generation.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceKind {
    Text,
    Vision,
    Image,
}
impl ServiceKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Vision => "vision",
            Self::Image => "image",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ServiceConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub use_env: bool,
    pub no_auth: bool,
}
impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            provider: "anthropic".into(),
            base_url: String::new(),
            model: String::new(),
            use_env: true,
            no_auth: false,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AiSettings {
    pub text: ServiceConfig,
    pub vision: ServiceConfig,
    pub image: ServiceConfig,
    pub vision_uses_text: bool,
}
impl Default for AiSettings {
    fn default() -> Self {
        Self {
            text: ServiceConfig::default(),
            vision: ServiceConfig::default(),
            image: ServiceConfig {
                provider: "openai".into(),
                model: "gpt-image-2".into(),
                use_env: false,
                ..ServiceConfig::default()
            },
            vision_uses_text: true,
        }
    }
}
impl AiSettings {
    pub fn service(&self, kind: ServiceKind) -> (&ServiceConfig, ServiceKind) {
        match kind {
            ServiceKind::Text => (&self.text, kind),
            ServiceKind::Vision if self.vision_uses_text => (&self.text, ServiceKind::Text),
            ServiceKind::Vision => (&self.vision, kind),
            ServiceKind::Image => (&self.image, kind),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct EffectiveService {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub base_source: String,
    pub model_source: String,
    pub key_source: String,
    pub ready: bool,
    pub using_text: bool,
    pub has_saved_key: bool,
}
// Deliberately no Debug/Serialize: credentials must not enter logs or responses.
pub struct ResolvedService {
    pub public: EffectiveService,
    pub api_key: Option<String>,
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}
pub fn env_prefix(kind: ServiceKind, provider: &str) -> &'static str {
    if kind == ServiceKind::Image {
        "IMAGE"
    } else if provider == "openai" {
        "OPENAI"
    } else {
        "ANTHROPIC"
    }
}
pub fn normalize_url(value: &str) -> Result<String, String> {
    let mut url =
        reqwest::Url::parse(value.trim()).map_err(|_| "请输入完整的 HTTP 或 HTTPS 服务地址")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("服务地址不能包含账号、密码、查询参数或片段".into());
    }
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&path);
    Ok(url.to_string().trim_end_matches('/').to_owned())
}
pub fn endpoint(
    config: &ServiceConfig,
    kind: ServiceKind,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<(String, String), String> {
    if !matches!(config.provider.as_str(), "openai" | "anthropic")
        || (kind == ServiceKind::Image && config.provider != "openai")
    {
        return Err("不支持的 AI 服务类型".into());
    }
    let prefix = env_prefix(kind, &config.provider);
    let explicit = non_empty(Some(config.base_url.clone()));
    let from_env = config
        .use_env
        .then(|| non_empty(env(&format!("{prefix}_BASE_URL"))))
        .flatten();
    let (base, source) = if let Some(base) = explicit {
        (base, "settings")
    } else if let Some(base) = from_env {
        (base, "environment")
    } else {
        (
            if config.provider == "openai" {
                "https://api.openai.com/v1"
            } else {
                "https://api.anthropic.com"
            }
            .into(),
            "default",
        )
    };
    Ok((normalize_url(&base)?, source.into()))
}
pub fn resolve(
    config: &ServiceConfig,
    kind: ServiceKind,
    env: &dyn Fn(&str) -> Option<String>,
    saved_key: Option<String>,
) -> Result<ResolvedService, String> {
    let (base_url, base_source) = endpoint(config, kind, env)?;
    let prefix = env_prefix(kind, &config.provider);
    let (model, model_source) = if let Some(model) = non_empty(Some(config.model.clone())) {
        (model, "settings")
    } else if let Some(model) = config
        .use_env
        .then(|| non_empty(env(&format!("{prefix}_MODEL"))))
        .flatten()
    {
        (model, "environment")
    } else {
        (
            if kind == ServiceKind::Image {
                "gpt-image-2"
            } else if config.provider == "openai" {
                "gpt-4o-mini"
            } else {
                "claude-haiku-4-5"
            }
            .into(),
            "default",
        )
    };
    let saved_key = non_empty(saved_key);
    let has_saved_key = saved_key.is_some();
    // Environment credentials may only follow the endpoint declared by that
    // environment (or its official default), never an unrelated UI endpoint.
    let env_config = ServiceConfig {
        base_url: String::new(),
        ..config.clone()
    };
    let env_matches =
        config.use_env && endpoint(&env_config, kind, env).is_ok_and(|(url, _)| url == base_url);
    let env_key = env_matches
        .then(|| non_empty(env(&format!("{prefix}_API_KEY"))))
        .flatten();
    let (api_key, key_source) = if config.no_auth {
        (None, "none")
    } else if saved_key.is_some() {
        (saved_key, "keychain")
    } else if env_key.is_some() {
        (env_key, "environment")
    } else {
        (None, "missing")
    };
    Ok(ResolvedService {
        public: EffectiveService {
            provider: config.provider.clone(),
            base_url,
            model,
            base_source,
            model_source: model_source.into(),
            key_source: key_source.into(),
            ready: config.no_auth || api_key.is_some(),
            using_text: false,
            has_saved_key,
        },
        api_key,
    })
}

/// Probe with fixed non-user data. Image probing only checks model discovery;
/// it does not claim that generation/edits or billing permissions were tested.
pub async fn probe(service: &ResolvedService, kind: ServiceKind) -> Result<&'static str, String> {
    if !service.public.ready {
        return Err("请先配置 API Key，或选择无需认证的本地服务".into());
    }
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法创建连接测试客户端")?;
    let cfg = &service.public;
    let mut request = if kind == ServiceKind::Image {
        http.get(format!("{}/models", cfg.base_url))
    } else if cfg.provider == "anthropic" {
        let mut content = vec![serde_json::json!({"type":"text","text":"Reply OK."})];
        if kind == ServiceKind::Vision {
            content.push(serde_json::json!({"type":"image","source":{"type":"base64","media_type":"image/png","data":TEST_IMAGE}}));
        }
        http.post(format!("{}/v1/messages", cfg.base_url)).header("anthropic-version", "2023-06-01").json(&serde_json::json!({"model":cfg.model,"max_tokens":8,"messages":[{"role":"user","content":content}]}))
    } else {
        let content = if kind == ServiceKind::Vision {
            serde_json::json!([{"type":"text","text":"Reply OK."},{"type":"image_url","image_url":{"url":format!("data:image/png;base64,{TEST_IMAGE}")}}])
        } else {
            serde_json::json!("Reply OK.")
        };
        http.post(format!("{}/chat/completions", cfg.base_url)).json(&serde_json::json!({"model":cfg.model,"max_tokens":8,"messages":[{"role":"user","content":content}]}))
    };
    if let Some(key) = &service.api_key {
        request = if cfg.provider == "anthropic" {
            request.header("x-api-key", key)
        } else {
            request.bearer_auth(key)
        };
    }
    let response = request
        .send()
        .await
        .map_err(|_| "连接失败，请检查服务地址、网络或证书")?;
    if !response.status().is_success() {
        return Err(format!(
            "连接测试失败（HTTP {0}）",
            response.status().as_u16()
        ));
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "服务返回了无法识别的响应")?;
    if kind == ServiceKind::Image {
        if !body["data"]
            .as_array()
            .is_some_and(|models| models.iter().any(|m| m["id"].as_str() == Some(&cfg.model)))
        {
            return Err("模型列表中未找到指定模型，图片生成尚未验证".into());
        }
        Ok("模型查询通过；尚未执行图片生成或编辑")
    } else {
        let valid = if cfg.provider == "anthropic" {
            body["content"].as_array().is_some_and(|v| {
                v.iter()
                    .any(|v| v["text"].as_str().is_some_and(|s| !s.is_empty()))
            })
        } else {
            body["choices"][0]["message"]["content"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
        };
        if !valid {
            return Err("服务返回了空内容或不兼容的响应格式".into());
        }
        Ok(if kind == ServiceKind::Vision {
            "视觉连接测试通过"
        } else {
            "文本连接测试通过"
        })
    }
}
const TEST_IMAGE: &str = "iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAIAAAD8GO2jAAAAN0lEQVR4nO3RwQ0AMAjDwJT9d05HMB9+vgGCZF7bXJrT9XhgwR8gEyETIRMhEyETIRMhEyEThXzH8QM9OMM6fAAAAABJRU5ErkJggg==";

#[cfg(test)]
mod tests {
    use super::*;
    fn server(body: &'static str, status: u16) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            loop {
                let mut buf = [0; 4096];
                let size = stream.read(&mut buf).unwrap();
                if size == 0 {
                    break;
                }
                bytes.extend_from_slice(&buf[..size]);
                let text = String::from_utf8_lossy(&bytes);
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text[..end]
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|v| v.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            String::from_utf8(bytes).unwrap()
        });
        (format!("http://{address}/v1"), handle)
    }
    #[tokio::test]
    async fn probes_authenticate_and_vision_includes_sample_image() {
        let (url, request) = server(r#"{"choices":[{"message":{"content":"OK"}}]}"#, 200);
        let config = ServiceConfig {
            provider: "openai".into(),
            base_url: url,
            ..ServiceConfig::default()
        };
        let resolved = resolve(
            &config,
            ServiceKind::Text,
            &|_| None,
            Some("test-value".into()),
        )
        .unwrap();
        assert_eq!(
            probe(&resolved, ServiceKind::Vision).await.unwrap(),
            "视觉连接测试通过"
        );
        let request = request.join().unwrap();
        assert!(request.starts_with("POST /v1/chat/completions"));
        assert!(request
            .to_lowercase()
            .contains("authorization: bearer test-value"));
        assert!(request.contains("data:image/png;base64,"));
    }
    #[tokio::test]
    async fn image_probe_only_lists_models_and_no_auth_omits_credentials() {
        let (url, request) = server(r#"{"data":[{"id":"gpt-image-2"}]}"#, 200);
        let config = ServiceConfig {
            provider: "openai".into(),
            base_url: url,
            no_auth: true,
            ..ServiceConfig::default()
        };
        let resolved = resolve(
            &config,
            ServiceKind::Image,
            &|_| None,
            Some("not-sent".into()),
        )
        .unwrap();
        assert!(probe(&resolved, ServiceKind::Image)
            .await
            .unwrap()
            .contains("尚未执行"));
        let request = request.join().unwrap().to_lowercase();
        assert!(request.starts_with("get /v1/models"));
        assert!(!request.contains("authorization:"));
        assert!(!request.contains("not-sent"));
    }
    #[tokio::test]
    async fn failed_probe_never_exposes_response_secrets() {
        let (url, request) = server(r#"{"error":"echoed-test-secret"}"#, 401);
        let config = ServiceConfig {
            provider: "openai".into(),
            base_url: url,
            ..ServiceConfig::default()
        };
        let resolved = resolve(&config, ServiceKind::Text, &|_| None, Some("test".into())).unwrap();
        assert_eq!(
            probe(&resolved, ServiceKind::Text).await.unwrap_err(),
            "连接测试失败（HTTP 401）"
        );
        request.join().unwrap();
    }
    #[test]
    fn saved_key_wins_and_environment_stays_with_its_provider_and_endpoint() {
        let env = |name: &str| match name {
            "OPENAI_API_KEY" => Some("env-test".into()),
            _ => None,
        };
        let mut cfg = ServiceConfig {
            provider: "openai".into(),
            ..ServiceConfig::default()
        };
        let result = resolve(&cfg, ServiceKind::Text, &env, Some("saved-test".into())).unwrap();
        assert_eq!(result.api_key.as_deref(), Some("saved-test"));
        cfg.base_url = "https://different.example/v1".into();
        assert!(
            !resolve(&cfg, ServiceKind::Text, &env, None)
                .unwrap()
                .public
                .ready
        );
        cfg.provider = "anthropic".into();
        cfg.base_url.clear();
        assert!(
            !resolve(&cfg, ServiceKind::Text, &env, None)
                .unwrap()
                .public
                .ready
        );
    }
    #[test]
    fn no_auth_and_vision_inheritance() {
        let mut settings = AiSettings::default();
        settings.text.no_auth = true;
        let (config, role) = settings.service(ServiceKind::Vision);
        assert_eq!(role, ServiceKind::Text);
        let resolved = resolve(config, role, &|_| None, Some("unused".into())).unwrap();
        assert!(resolved.public.ready);
        assert!(resolved.api_key.is_none());
        assert_eq!(resolved.public.key_source, "none");
    }
    #[test]
    fn urls_are_canonical_and_secrets_cannot_be_settings() {
        assert_eq!(
            normalize_url(" https://example.com/v1/ ").unwrap(),
            "https://example.com/v1"
        );
        for url in [
            "file:///tmp",
            "https://user:pass@example.com",
            "https://example.com?key=secret",
        ] {
            assert!(normalize_url(url).is_err());
        }
        assert!(
            serde_json::from_value::<ServiceConfig>(serde_json::json!({"api_key":"secret"}))
                .is_err()
        );
    }
}
