//! Application storage and keychain wiring for the shared AI configuration.
use tauri::AppHandle;
use vtb_account::Secrets;
use vtb_translate::settings::{
    self, AiSettings, EffectiveService, ResolvedService, ServiceConfig, ServiceKind,
};

static AI_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
const SETTINGS_KEY: &str = "ai.services";
fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}
fn key_name(config: &ServiceConfig, kind: ServiceKind) -> Result<String, String> {
    let (url, _) = settings::endpoint(config, kind, &env)?;
    Ok(Secrets::ai_name(kind.name(), &config.provider, &url))
}
fn key_error(_: impl std::fmt::Display) -> String {
    "无法访问系统凭据库，请检查系统凭据服务".into()
}

/// Migrate once, associating the old shared key only with its existing text
/// endpoint. An environment-provided address is pinned during migration.
fn load(app: &AppHandle) -> Result<AiSettings, String> {
    let root = super::config::read_settings(app);
    if let Some(value) = root.get(SETTINGS_KEY) {
        return serde_json::from_value(value.clone()).map_err(|_| "AI 配置文件格式无效".into());
    }
    let legacy_key = Secrets::get("llm-api-key").map_err(key_error)?;
    let config = migrate_config(&root, legacy_key.is_some(), &env)?;
    if let Some(key) = legacy_key {
        Secrets::set(&key_name(&config.text, ServiceKind::Text)?, &key).map_err(key_error)?;
    }
    super::config::save_app_key(
        app,
        SETTINGS_KEY,
        serde_json::to_value(&config).map_err(|e| e.to_string())?,
    )?;
    Ok(config)
}
fn migrate_config(
    root: &serde_json::Value,
    has_legacy_key: bool,
    environment: &dyn Fn(&str) -> Option<String>,
) -> Result<AiSettings, String> {
    let mut config = AiSettings::default();
    for (key, target) in [
        ("provider", &mut config.text.provider),
        ("base_url", &mut config.text.base_url),
        ("model", &mut config.text.model),
    ] {
        if let Some(value) = root["llm"][key].as_str().filter(|v| !v.trim().is_empty()) {
            *target = value.trim().into();
        }
    }
    if let Some(value) = root["cover.img_base"].as_str() {
        config.image.base_url = value.into();
    }
    if let Some(value) = root["cover.img_model"].as_str() {
        config.image.model = value.into();
    }
    if has_legacy_key {
        config.text.base_url = settings::endpoint(&config.text, ServiceKind::Text, environment)?.0;
    }
    Ok(config)
}
fn resolve_config(config: &AiSettings, kind: ServiceKind) -> Result<ResolvedService, String> {
    let (service, effective_kind) = config.service(kind);
    let saved = Secrets::get(&key_name(service, effective_kind)?).map_err(key_error)?;
    let mut resolved = settings::resolve(service, effective_kind, &env, saved)?;
    resolved.public.using_text = kind == ServiceKind::Vision && config.vision_uses_text;
    Ok(resolved)
}
pub fn resolve_service(app: &AppHandle, kind: ServiceKind) -> Result<ResolvedService, String> {
    let _guard = AI_LOCK.lock().map_err(|_| "AI 配置暂时不可用")?;
    let config = load(app)?;
    let resolved = resolve_config(&config, kind)?;
    if !resolved.public.ready {
        return Err("请先在设置 → AI 服务中配置 API Key".into());
    }
    Ok(resolved)
}
pub fn resolve_text(
    app: &AppHandle,
    provider: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
) -> Result<ResolvedService, String> {
    let _guard = AI_LOCK.lock().map_err(|_| "AI 配置暂时不可用")?;
    let mut config = load(app)?;
    for (value, target) in [
        (provider, &mut config.text.provider),
        (model, &mut config.text.model),
        (base_url, &mut config.text.base_url),
    ] {
        if let Some(value) = value.filter(|v| !v.trim().is_empty()) {
            *target = value.trim().into();
        }
    }
    let mut resolved = resolve_config(&config, ServiceKind::Text)?;
    if let Some(key) = api_key.filter(|v| !v.trim().is_empty()) {
        resolved.api_key = Some(key.trim().into());
        resolved.public.ready = true;
    }
    if !resolved.public.ready {
        return Err("请先在设置 → AI 服务中配置 API Key".into());
    }
    Ok(resolved)
}

#[derive(serde::Serialize)]
pub struct AiSnapshot {
    config: AiSettings,
    effective: std::collections::BTreeMap<String, EffectiveService>,
}
fn snapshot(config: AiSettings) -> Result<AiSnapshot, String> {
    let mut effective = std::collections::BTreeMap::new();
    for kind in [ServiceKind::Text, ServiceKind::Vision, ServiceKind::Image] {
        effective.insert(kind.name().into(), resolve_config(&config, kind)?.public);
    }
    Ok(AiSnapshot { config, effective })
}
#[tauri::command]
pub async fn ai_settings_load(app: AppHandle) -> Result<AiSnapshot, String> {
    let _guard = AI_LOCK.lock().map_err(|_| "AI 配置暂时不可用")?;
    snapshot(load(&app)?)
}
#[tauri::command]
pub async fn ai_settings_save(
    app: AppHandle,
    kind: ServiceKind,
    service: ServiceConfig,
    vision_uses_text: Option<bool>,
    api_key: Option<String>,
) -> Result<AiSnapshot, String> {
    let _guard = AI_LOCK.lock().map_err(|_| "AI 配置暂时不可用")?;
    let mut config = load(&app)?;
    settings::endpoint(&service, kind, &env)?;
    if let Some(key) = api_key.filter(|v| !v.trim().is_empty()) {
        if service.no_auth {
            return Err("无需认证时不能保存 API Key".into());
        }
        Secrets::set(&key_name(&service, kind)?, key.trim()).map_err(key_error)?;
    }
    match kind {
        ServiceKind::Text => config.text = service,
        ServiceKind::Vision => {
            config.vision = service;
            if let Some(value) = vision_uses_text {
                config.vision_uses_text = value;
            }
        }
        ServiceKind::Image => config.image = service,
    }
    super::config::save_app_key(
        &app,
        SETTINGS_KEY,
        serde_json::to_value(&config).map_err(|e| e.to_string())?,
    )?;
    snapshot(config)
}
#[tauri::command]
pub async fn ai_key_clear(app: AppHandle, kind: ServiceKind) -> Result<AiSnapshot, String> {
    let _guard = AI_LOCK.lock().map_err(|_| "AI 配置暂时不可用")?;
    let config = load(&app)?;
    if kind == ServiceKind::Vision && config.vision_uses_text {
        return Err("请到文本模型中管理共用密钥".into());
    }
    let (service, role) = config.service(kind);
    Secrets::delete(&key_name(service, role)?).map_err(key_error)?;
    snapshot(config)
}
#[tauri::command]
pub async fn ai_connection_test(app: AppHandle, kind: ServiceKind) -> Result<String, String> {
    let service = resolve_service(&app, kind)?;
    settings::probe(&service, kind).await.map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_preserves_public_settings_and_pins_the_legacy_key_destination() {
        let root = serde_json::json!({"llm":{"provider":"openai","model":"old-text"}, "cover.img_base":"https://images.example/v1", "cover.img_model":"old-image"});
        let env =
            |name: &str| (name == "OPENAI_BASE_URL").then(|| "https://text.example/v1/".into());
        let config = migrate_config(&root, true, &env).unwrap();
        assert_eq!(config.text.base_url, "https://text.example/v1");
        assert_eq!(config.text.model, "old-text");
        assert_eq!(config.image.base_url, "https://images.example/v1");
        assert_eq!(config.image.model, "old-image");
        assert!(config.vision_uses_text);
        assert!(!config.image.use_env);
        assert!(migrate_config(&root, false, &env)
            .unwrap()
            .text
            .base_url
            .is_empty());
    }
}
