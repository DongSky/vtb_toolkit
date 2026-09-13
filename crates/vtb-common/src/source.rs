//! Source identity carried alongside legacy numeric Bilibili events.
//! Platform IDs remain strings; they are never hashed into Bilibili IDs.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LivePlatform {
    Bilibili,
    Youtube,
    Twitch,
}

impl LivePlatform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bilibili => "bilibili",
            Self::Youtube => "youtube",
            Self::Twitch => "twitch",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Money {
    /// ISO 4217 when known. Ambiguous symbols are retained as display only.
    pub currency: Option<String>,
    pub amount: Option<f64>,
    pub display: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatRun {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventSource {
    pub platform: LivePlatform,
    pub room_id: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub money: Option<Money>,
    /// Platform wording for membership / milestone / gift redemption.
    #[serde(default)]
    pub membership: Option<String>,
    #[serde(default)]
    pub sticker_url: Option<String>,
    #[serde(default)]
    pub event_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub message_runs: Vec<ChatRun>,
}

impl EventSource {
    pub fn room_key(&self) -> String {
        format!("{}:{}", self.platform.as_str(), self.room_id)
    }

    pub fn user_key(&self) -> Option<String> {
        self.user_id
            .as_ref()
            .map(|id| format!("{}:{id}", self.platform.as_str()))
    }
}
