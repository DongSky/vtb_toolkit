//! 场控 commands: sending danmaku from the logged-in account.
//!
//! - `danmaku_send` — manual send (errors surface to the UI directly).
//! - Auto-thank: the danmaku pump formats thank-you lines for gifts /
//!   guard purchases / SuperChats and feeds them through a serial,
//!   rate-limited worker (min 3 s gap, small queue, drop when saturated).
//! - Timed announcements: a per-room loop that enqueues a fixed line
//!   every N seconds (clamped ≥ 30 s).
//!
//! Compliance: everything here posts through the official web endpoint on
//! the user's own account, serially and conservatively throttled — no
//! bulk/multi-account behavior.

use crate::state::AppState;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use vtb_common::LiveEvent;

/// Minimum gap between automated sends.
const AUTO_GAP: Duration = Duration::from_secs(3);
/// Timed announcements can't fire more often than this.
const MIN_TIMER_SECS: u64 = 30;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AutoThankConfig {
    pub thank_gift: bool,
    pub thank_guard: bool,
    pub thank_sc: bool,
    /// Templates support `{user}` `{gift}` `{count}` `{level}` `{price}`.
    pub gift_template: String,
    pub guard_template: String,
    pub sc_template: String,
    /// Only thank gifts worth at least this many CNY (0 = all).
    pub min_gift_price: f64,
}

impl Default for AutoThankConfig {
    fn default() -> Self {
        Self {
            thank_gift: false,
            thank_guard: false,
            thank_sc: false,
            gift_template: "感谢 {user} 投喂的 {gift} ×{count}！".into(),
            guard_template: "感谢 {user} 开通{level}！".into(),
            sc_template: "感谢 {user} 的SC！".into(),
            min_gift_price: 0.0,
        }
    }
}

fn guard_name(level: u8) -> &'static str {
    match level {
        1 => "总督",
        2 => "提督",
        3 => "舰长",
        _ => "大航海",
    }
}

fn fill(template: &str, pairs: &[(&str, String)]) -> String {
    let mut out = template.to_string();
    for (k, v) in pairs {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// Format the thank-you line for an event, if the config wants one.
pub fn format_thanks(cfg: &AutoThankConfig, ev: &LiveEvent) -> Option<String> {
    let line = match ev {
        LiveEvent::Gift(g) if cfg.thank_gift => {
            if g.total_price < cfg.min_gift_price {
                return None;
            }
            fill(
                &cfg.gift_template,
                &[
                    ("user", g.username.clone()),
                    ("gift", g.gift_name.clone()),
                    ("count", g.count.to_string()),
                    ("price", format!("{:.1}", g.total_price)),
                ],
            )
        }
        LiveEvent::GuardBuy(g) if cfg.thank_guard => fill(
            &cfg.guard_template,
            &[
                ("user", g.username.clone()),
                ("level", guard_name(g.guard_level).to_string()),
                ("count", g.count.to_string()),
            ],
        ),
        LiveEvent::SuperChat(s) if cfg.thank_sc => fill(
            &cfg.sc_template,
            &[
                ("user", s.username.clone()),
                ("price", format!("{:.0}", s.price)),
            ],
        ),
        _ => return None,
    };
    let line = line.trim().to_string();
    (!line.is_empty()).then_some(line)
}

async fn send_now(state: &AppState, room_id: u64, text: &str) -> Result<(), String> {
    let creds = state.creds().ok_or("未登录，无法发送弹幕")?;
    if creds.bili_jct.is_empty() {
        return Err("登录凭据缺少 bili_jct，请重新扫码登录".into());
    }
    let api = vtb_danmaku::api::BiliApi::new(state.api_client()?);
    api.send_danmaku(room_id, text, &creds.bili_jct)
        .await
        .map_err(|e| e.to_string())
}

/// Spawn the serial rate-limited sender used by auto-thank and timers.
pub fn spawn_send_worker(app: AppHandle) -> tokio::sync::mpsc::Sender<(u64, String)> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(u64, String)>(8);
    tauri::async_runtime::spawn(async move {
        let mut last: Option<tokio::time::Instant> = None;
        while let Some((room_id, text)) = rx.recv().await {
            if let Some(t) = last {
                let since = t.elapsed();
                if since < AUTO_GAP {
                    tokio::time::sleep(AUTO_GAP - since).await;
                }
            }
            let state = app.state::<AppState>();
            if let Err(e) = send_now(&state, room_id, &text).await {
                tracing::warn!("auto danmaku send failed ({room_id}): {e}");
            }
            last = Some(tokio::time::Instant::now());
        }
    });
    tx
}

/// Manual send from the UI. Errors (rate limit, filtered, not logged in)
/// come back verbatim.
#[tauri::command]
pub async fn danmaku_send(
    state: State<'_, AppState>,
    room_id: u64,
    text: String,
) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("弹幕内容为空".into());
    }
    send_now(&state, room_id, text).await
}

/// Update the auto-thank config (global; applies to all connected rooms).
#[tauri::command]
pub async fn danmaku_autothank_set(
    state: State<'_, AppState>,
    config: AutoThankConfig,
) -> Result<(), String> {
    *state.autothank.lock().unwrap() = config;
    Ok(())
}

#[tauri::command]
pub async fn danmaku_autothank_get(
    state: State<'_, AppState>,
) -> Result<AutoThankConfig, String> {
    Ok(state.autothank.lock().unwrap().clone())
}

/// Start (or replace) a timed announcement for a room.
#[tauri::command]
pub async fn danmaku_timer_start(
    app: AppHandle,
    state: State<'_, AppState>,
    room_id: u64,
    text: String,
    interval_secs: u64,
) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("定时弹幕内容为空".into());
    }
    if state.creds().is_none() {
        return Err("未登录，无法发送弹幕".into());
    }
    let interval = Duration::from_secs(interval_secs.max(MIN_TIMER_SECS));
    let tx = state
        .danmaku_send_tx
        .get_or_init(|| spawn_send_worker(app.clone()))
        .clone();
    let task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            if tx.try_send((room_id, text.clone())).is_err() {
                tracing::warn!("timed danmaku dropped (queue full)");
            }
        }
    });
    let mut timers = state.danmaku_timers.lock().unwrap();
    if let Some(old) = timers.insert(room_id, task) {
        old.abort();
    }
    Ok(())
}

#[tauri::command]
pub async fn danmaku_timer_stop(
    state: State<'_, AppState>,
    room_id: u64,
) -> Result<(), String> {
    if let Some(t) = state.danmaku_timers.lock().unwrap().remove(&room_id) {
        t.abort();
        Ok(())
    } else {
        Err(format!("room {room_id} 没有运行中的定时弹幕"))
    }
}

/// Rooms with an active timed announcement.
#[tauri::command]
pub async fn danmaku_timer_status(state: State<'_, AppState>) -> Result<Vec<u64>, String> {
    let timers = state.danmaku_timers.lock().unwrap();
    Ok(timers
        .iter()
        .filter(|(_, t)| !t.is_finished())
        .map(|(id, _)| *id)
        .collect())
}

/// Shared handle type for the auto-thank config.
pub type SharedAutoThank = Arc<Mutex<AutoThankConfig>>;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use vtb_common::{GiftMsg, GuardBuyMsg, SuperChatMsg};

    fn gift(price: f64) -> LiveEvent {
        LiveEvent::Gift(GiftMsg {
            room_id: 1,
            uid: 2,
            username: "老板".into(),
            gift_name: "小花花".into(),
            gift_id: 1,
            count: 10,
            total_price: price,
            timestamp: Utc::now(),
        })
    }

    #[test]
    fn thanks_disabled_by_default() {
        let cfg = AutoThankConfig::default();
        assert!(format_thanks(&cfg, &gift(5.0)).is_none());
    }

    #[test]
    fn gift_template_fills_and_respects_min_price() {
        let cfg = AutoThankConfig {
            thank_gift: true,
            min_gift_price: 1.0,
            ..Default::default()
        };
        assert_eq!(
            format_thanks(&cfg, &gift(5.0)).unwrap(),
            "感谢 老板 投喂的 小花花 ×10！"
        );
        assert!(format_thanks(&cfg, &gift(0.0)).is_none(), "free gift below min");
    }

    #[test]
    fn guard_and_sc_templates() {
        let cfg = AutoThankConfig {
            thank_guard: true,
            thank_sc: true,
            sc_template: "感谢 {user} 的 ¥{price} SC！".into(),
            ..Default::default()
        };
        let guard = LiveEvent::GuardBuy(GuardBuyMsg {
            room_id: 1,
            uid: 3,
            username: "新舰长".into(),
            guard_level: 3,
            count: 1,
            price: 198.0,
            timestamp: Utc::now(),
        });
        assert_eq!(format_thanks(&cfg, &guard).unwrap(), "感谢 新舰长 开通舰长！");

        let sc = LiveEvent::SuperChat(SuperChatMsg {
            room_id: 1,
            uid: 4,
            username: "金主".into(),
            text: "加油".into(),
            price: 30.0,
            timestamp: Utc::now(),
            duration_secs: 60,
        });
        assert_eq!(format_thanks(&cfg, &sc).unwrap(), "感谢 金主 的 ¥30 SC！");
    }

    #[test]
    fn plain_danmaku_never_thanked() {
        let cfg = AutoThankConfig {
            thank_gift: true,
            thank_guard: true,
            thank_sc: true,
            ..Default::default()
        };
        let ev = LiveEvent::Danmaku(vtb_common::DanmakuMsg {
            room_id: 1,
            uid: 5,
            username: "观众".into(),
            text: "谢谢".into(),
            timestamp: Utc::now(),
            medal: None,
            guard_level: 0,
            is_admin: false,
            emoticon: None,
        });
        assert!(format_thanks(&cfg, &ev).is_none());
    }
}
