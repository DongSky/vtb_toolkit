//! Sending danmaku (chat) messages to a live room.
//!
//! Uses the official web endpoint `POST /msg/send` with the account's
//! `bili_jct` cookie as the csrf token — the same call the live.bilibili.com
//! web page makes. Requires a logged-in cookie client.

use crate::error::{DanmakuError, Result};

const MSG_SEND: &str = "https://api.live.bilibili.com/msg/send";

/// Build the `msg/send` form fields. `rnd` is the unix timestamp (a
/// parameter so tests are deterministic).
pub fn build_send_form(
    room_id: u64,
    msg: &str,
    csrf: &str,
    rnd: u64,
) -> Vec<(&'static str, String)> {
    vec![
        ("bubble", "0".into()),
        ("msg", msg.to_string()),
        ("color", "16777215".into()),
        ("mode", "1".into()),
        ("fontsize", "25".into()),
        ("rnd", rnd.to_string()),
        ("roomid", room_id.to_string()),
        ("csrf", csrf.to_string()),
        ("csrf_token", csrf.to_string()),
    ]
}

/// Interpret a `msg/send` response body.
///
/// Known outcomes: `code=0, message=""` sent; `code=0, message="f"` the
/// message was silently swallowed by the room's filter; `code=10030` rate
/// limited; `code=10031` duplicate; `code=-101` not logged in.
pub fn parse_send_response(body: &str) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct Envelope {
        code: i64,
        #[serde(default)]
        message: String,
        #[serde(default)]
        msg: String,
    }
    let env: Envelope = serde_json::from_str(body)?;
    if env.code != 0 {
        let message = match env.code {
            10030 => "发送过于频繁，请稍后".into(),
            10031 => "不能重复发送相同弹幕".into(),
            -101 => "账号未登录".into(),
            _ if !env.message.is_empty() => env.message,
            _ => env.msg,
        };
        return Err(DanmakuError::Api {
            code: env.code,
            message,
        });
    }
    if env.message == "f" || env.msg == "f" {
        return Err(DanmakuError::Api {
            code: 0,
            message: "弹幕被房间屏蔽词过滤".into(),
        });
    }
    Ok(())
}

impl crate::api::BiliApi {
    /// Send a chat message to `room_id` on behalf of the logged-in user.
    /// The underlying client must carry the account cookies; `csrf` is the
    /// account's `bili_jct`.
    pub async fn send_danmaku(&self, room_id: u64, msg: &str, csrf: &str) -> Result<()> {
        let rnd = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let body = self
            .http()
            .post(MSG_SEND)
            .header("Referer", "https://live.bilibili.com/")
            .form(&build_send_form(room_id, msg, csrf, rnd))
            .send()
            .await?
            .text()
            .await?;
        parse_send_response(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_has_csrf_pair_and_room() {
        let form = build_send_form(320, "你好", "TOKEN", 1_700_000_000);
        let get = |k: &str| {
            form.iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.as_str())
                .unwrap()
        };
        assert_eq!(get("roomid"), "320");
        assert_eq!(get("msg"), "你好");
        assert_eq!(get("csrf"), "TOKEN");
        assert_eq!(get("csrf_token"), "TOKEN");
        assert_eq!(get("mode"), "1");
        assert_eq!(get("rnd"), "1700000000");
    }

    #[test]
    fn parse_ok() {
        assert!(parse_send_response(r#"{"code":0,"message":"","data":{}}"#).is_ok());
    }

    #[test]
    fn parse_filtered_rate_limited_and_dup() {
        let f = parse_send_response(r#"{"code":0,"message":"f","data":{}}"#);
        assert!(f.unwrap_err().to_string().contains("过滤"));

        let rl = parse_send_response(r#"{"code":10030,"message":"msg in 1s"}"#);
        assert!(rl.unwrap_err().to_string().contains("频繁"));

        let dup = parse_send_response(r#"{"code":10031,"message":"msg repeat"}"#);
        assert!(dup.unwrap_err().to_string().contains("重复"));

        let anon = parse_send_response(r#"{"code":-101,"message":"账号未登录"}"#);
        assert!(anon.unwrap_err().to_string().contains("未登录"));
    }
}
