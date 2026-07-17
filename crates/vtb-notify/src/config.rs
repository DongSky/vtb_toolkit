//! User-facing notification settings.

use crate::notifiers::{Bark, ServerChan, Telegram, Webhook};
use crate::{MultiNotifier, Notifier};
use serde::{Deserialize, Serialize};

/// Which channels are configured; every field is optional so this can live
/// inside the app's settings JSON. [`NotifyConfig::build`] turns it into
/// live notifiers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NotifyConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bark_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serverchan_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telegram: Option<TelegramConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub webhook_urls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String,
}

impl NotifyConfig {
    /// One notifier per configured channel; empty keys/URLs are skipped.
    pub fn build(&self) -> MultiNotifier {
        let mut out: Vec<Box<dyn Notifier>> = Vec::new();
        if let Some(key) = self.bark_key.as_deref().filter(|k| !k.is_empty()) {
            out.push(Box::new(Bark::new(key)));
        }
        if let Some(key) = self.serverchan_key.as_deref().filter(|k| !k.is_empty()) {
            out.push(Box::new(ServerChan::new(key)));
        }
        if let Some(tg) = self.telegram.as_ref().filter(|t| !t.bot_token.is_empty()) {
            out.push(Box::new(Telegram::new(
                tg.bot_token.clone(),
                tg.chat_id.clone(),
            )));
        }
        for url in self.webhook_urls.iter().filter(|u| !u.is_empty()) {
            out.push(Box::new(Webhook::new(url.clone())));
        }
        MultiNotifier(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_creates_one_notifier_per_channel() {
        let cfg = NotifyConfig {
            bark_key: Some("bk".into()),
            serverchan_key: Some("sk".into()),
            telegram: Some(TelegramConfig {
                bot_token: "123:abc".into(),
                chat_id: "42".into(),
            }),
            webhook_urls: vec!["http://a.example/h".into(), "http://b.example/h".into()],
        };
        assert_eq!(cfg.build().len(), 5);
    }

    #[test]
    fn empty_config_builds_nothing() {
        assert!(NotifyConfig::default().build().is_empty());
        // Empty strings count as "not configured".
        let cfg = NotifyConfig {
            bark_key: Some(String::new()),
            webhook_urls: vec![String::new()],
            ..Default::default()
        };
        assert_eq!(cfg.build().len(), 0);
    }

    #[test]
    fn deserializes_from_partial_json() {
        let cfg: NotifyConfig = serde_json::from_str(r#"{"bark_key":"abc"}"#).unwrap();
        assert_eq!(cfg.bark_key.as_deref(), Some("abc"));
        assert!(cfg.serverchan_key.is_none());
        assert!(cfg.webhook_urls.is_empty());
        assert_eq!(cfg.build().len(), 1);

        // Round-trips without noise for unset fields.
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(json, r#"{"bark_key":"abc"}"#);
    }
}
