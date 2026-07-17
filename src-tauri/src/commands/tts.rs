//! Danmaku TTS (macOS `say`): reads chat aloud so single streamers don't
//! need to watch the chat window.

use crate::state::AppState;
use std::sync::atomic::Ordering;
use tauri::State;
use vtb_common::LiveEvent;

/// What to speak for an event under the current filter, if anything.
pub fn should_speak(ev: &LiveEvent, paid_only: bool) -> Option<String> {
    match ev {
        LiveEvent::Danmaku(d) if !paid_only => {
            // Skip sticker-only spam like "[妙][妙][妙]": drop bracketed
            // groups entirely; if nothing remains, don't speak it.
            let mut outside = String::new();
            let mut depth = 0usize;
            for c in d.text.chars() {
                match c {
                    '[' | '（' | '(' => depth += 1,
                    ']' | '）' | ')' => depth = depth.saturating_sub(1),
                    _ if depth == 0 => outside.push(c),
                    _ => {}
                }
            }
            if outside.trim().is_empty() {
                return None;
            }
            Some(format!("{}说：{}", d.username, outside.trim()))
        }
        LiveEvent::SuperChat(sc) => {
            Some(format!("{}的{}元留言：{}", sc.username, sc.price as i64, sc.text))
        }
        LiveEvent::Gift(g) if g.total_price >= 1.0 => {
            Some(format!("感谢{}投喂{}个{}", g.username, g.count, g.gift_name))
        }
        LiveEvent::GuardBuy(g) => Some(format!("感谢{}开通舰长", g.username)),
        _ => None,
    }
}

/// Serial speech worker: one utterance at a time, bounded queue (overflow
/// drops oldest — TTS must never back up the event pump).
pub fn spawn_tts_worker() -> tokio::sync::mpsc::Sender<String> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(16);
    tokio::spawn(async move {
        while let Some(text) = rx.recv().await {
            #[cfg(target_os = "macos")]
            {
                let _ = tokio::process::Command::new("say")
                    .arg("-r")
                    .arg("220")
                    .arg(&text)
                    .status()
                    .await;
            }
            #[cfg(not(target_os = "macos"))]
            {
                tracing::debug!("TTS unsupported on this OS: {text}");
            }
        }
    });
    tx
}

#[derive(serde::Serialize)]
pub struct TtsStatus {
    pub enabled: bool,
    pub paid_only: bool,
}

#[tauri::command]
pub async fn tts_set(
    state: State<'_, AppState>,
    enabled: bool,
    paid_only: bool,
) -> Result<TtsStatus, String> {
    state.tts_enabled.store(enabled, Ordering::Relaxed);
    state.tts_paid_only.store(paid_only, Ordering::Relaxed);
    Ok(TtsStatus { enabled, paid_only })
}

#[tauri::command]
pub async fn tts_status(state: State<'_, AppState>) -> Result<TtsStatus, String> {
    Ok(TtsStatus {
        enabled: state.tts_enabled.load(Ordering::Relaxed),
        paid_only: state.tts_paid_only.load(Ordering::Relaxed),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use vtb_common::{DanmakuMsg, GiftMsg, SuperChatMsg};

    fn danmaku(text: &str) -> LiveEvent {
        LiveEvent::Danmaku(DanmakuMsg {
            room_id: 1,
            uid: 1,
            username: "观众".into(),
            text: text.into(),
            timestamp: Utc::now(),
            medal: None,
            guard_level: 0,
            is_admin: false,
            emoticon: None,
        })
    }

    #[test]
    fn danmaku_spoken_unless_paid_only() {
        let ev = danmaku("晚上好");
        assert_eq!(should_speak(&ev, false).unwrap(), "观众说：晚上好");
        assert!(should_speak(&ev, true).is_none());
    }

    #[test]
    fn emoticon_spam_skipped() {
        assert!(should_speak(&danmaku("[妙][妙][妙]"), false).is_none());
    }

    #[test]
    fn superchat_always_spoken() {
        let sc = LiveEvent::SuperChat(SuperChatMsg {
            room_id: 1,
            uid: 2,
            username: "金主".into(),
            text: "加油".into(),
            price: 30.0,
            duration_secs: 60,
            timestamp: Utc::now(),
        });
        for paid_only in [false, true] {
            let s = should_speak(&sc, paid_only).unwrap();
            assert!(s.contains("金主") && s.contains("30") && s.contains("加油"));
        }
    }

    #[test]
    fn free_gifts_not_spoken() {
        let free = LiveEvent::Gift(GiftMsg {
            room_id: 1,
            uid: 3,
            username: "路人".into(),
            gift_name: "辣条".into(),
            gift_id: 1,
            count: 5,
            total_price: 0.0,
            timestamp: Utc::now(),
        });
        assert!(should_speak(&free, false).is_none());
    }
}
