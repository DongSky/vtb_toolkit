//! Parse Bilibili `cmd` JSON payloads into [`LiveEvent`]s.
//!
//! The danmaku server sends many command types; we map the ones relevant
//! to a VTuber toolkit and ignore the rest (returning `None`).

use chrono::{TimeZone, Utc};
use serde_json::Value;
use vtb_common::{
    DanmakuMsg, EnterMsg, GiftMsg, GuardBuyMsg, LikeMsg, LiveEvent, Medal, SuperChatMsg,
};

/// Parse one command object into a [`LiveEvent`], if recognized.
pub fn parse_cmd(room_id: u64, v: &Value) -> Option<LiveEvent> {
    let cmd = v.get("cmd")?.as_str()?;
    // DANMU_MSG sometimes carries a suffix like "DANMU_MSG:4:0:2:2:2:0".
    let cmd = cmd.split(':').next().unwrap_or(cmd);
    match cmd {
        "DANMU_MSG" => parse_danmu(room_id, v),
        "SUPER_CHAT_MESSAGE" => parse_super_chat(room_id, v),
        "SEND_GIFT" => parse_gift(room_id, v),
        "GUARD_BUY" => parse_guard_buy(room_id, v),
        "INTERACT_WORD" => parse_interact(room_id, v),
        "LIKE_INFO_V3_CLICK" => parse_like(room_id, v),
        "LIVE" => Some(LiveEvent::LiveStart {
            room_id,
            timestamp: Utc::now(),
        }),
        "PREPARING" => Some(LiveEvent::LiveEnd {
            room_id,
            timestamp: Utc::now(),
        }),
        "WATCHED_CHANGE" => {
            let count = v.get("data")?.get("num")?.as_u64()?;
            Some(LiveEvent::WatchedChange { room_id, count })
        }
        _ => None,
    }
}

fn ts_from_secs_or_now(secs: Option<i64>) -> chrono::DateTime<Utc> {
    secs.and_then(|s| Utc.timestamp_opt(s, 0).single())
        .unwrap_or_else(Utc::now)
}

fn ts_from_millis_or_now(ms: Option<i64>) -> chrono::DateTime<Utc> {
    ms.and_then(|m| Utc.timestamp_millis_opt(m).single())
        .unwrap_or_else(Utc::now)
}

fn parse_danmu(room_id: u64, v: &Value) -> Option<LiveEvent> {
    let info = v.get("info")?.as_array()?;
    let text = info.get(1)?.as_str()?.to_string();

    // info[0] is an array of meta; index 4 holds the send-time in ms.
    let meta = info.first()?.as_array()?;
    let ts = ts_from_millis_or_now(meta.get(4).and_then(Value::as_i64));

    // info[2] = [uid, uname, is_admin, ...]
    let user = info.get(2)?.as_array()?;
    let uid = user.first()?.as_u64().unwrap_or(0);
    let username = user.get(1)?.as_str().unwrap_or("").to_string();
    let is_admin = user.get(2).and_then(Value::as_i64).unwrap_or(0) != 0;

    // info[3] = [medal_level, medal_name, anchor_uname, anchor_roomid, ...]
    let medal = info
        .get(3)
        .and_then(Value::as_array)
        .filter(|m| !m.is_empty())
        .map(|m| Medal {
            level: m.first().and_then(Value::as_u64).unwrap_or(0) as u32,
            name: m.get(1).and_then(Value::as_str).unwrap_or("").to_string(),
            anchor_room_id: m.get(3).and_then(Value::as_u64).unwrap_or(0),
        });

    // guard level lives at info[7].
    let guard_level = info.get(7).and_then(Value::as_u64).unwrap_or(0) as u8;

    // Emoticon danmaku: info[0][13] is an object with a "url".
    let emoticon = meta
        .get(13)
        .and_then(|e| e.get("url"))
        .and_then(Value::as_str)
        .map(str::to_string);

    Some(LiveEvent::Danmaku(DanmakuMsg {
        room_id,
        uid,
        username,
        text,
        timestamp: ts,
        medal,
        guard_level,
        is_admin,
        emoticon,
    }))
}

fn parse_super_chat(room_id: u64, v: &Value) -> Option<LiveEvent> {
    let data = v.get("data")?;
    let user = data.get("user_info")?;
    Some(LiveEvent::SuperChat(SuperChatMsg {
        room_id,
        uid: data.get("uid")?.as_u64().unwrap_or(0),
        username: user
            .get("uname")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        text: data
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        price: data.get("price").and_then(Value::as_f64).unwrap_or(0.0),
        duration_secs: data.get("time").and_then(Value::as_u64).unwrap_or(0),
        timestamp: ts_from_secs_or_now(data.get("start_time").and_then(Value::as_i64)),
    }))
}

fn parse_gift(room_id: u64, v: &Value) -> Option<LiveEvent> {
    let data = v.get("data")?;
    let count = data.get("num").and_then(Value::as_u64).unwrap_or(0);
    // price is in 电池/100 = CNY; total_price already in 电池 units (1000 = 1 CNY).
    let total_coin = data.get("total_coin").and_then(Value::as_f64).unwrap_or(0.0);
    let coin_type = data
        .get("coin_type")
        .and_then(Value::as_str)
        .unwrap_or("silver");
    // Only gold gifts have real monetary value.
    let total_price = if coin_type == "gold" {
        total_coin / 1000.0
    } else {
        0.0
    };
    Some(LiveEvent::Gift(GiftMsg {
        room_id,
        uid: data.get("uid").and_then(Value::as_u64).unwrap_or(0),
        username: data
            .get("uname")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        gift_name: data
            .get("giftName")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        gift_id: data.get("giftId").and_then(Value::as_u64).unwrap_or(0),
        count,
        total_price,
        timestamp: ts_from_secs_or_now(data.get("timestamp").and_then(Value::as_i64)),
    }))
}

fn parse_guard_buy(room_id: u64, v: &Value) -> Option<LiveEvent> {
    let data = v.get("data")?;
    Some(LiveEvent::GuardBuy(GuardBuyMsg {
        room_id,
        uid: data.get("uid").and_then(Value::as_u64).unwrap_or(0),
        username: data
            .get("username")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        guard_level: data.get("guard_level").and_then(Value::as_u64).unwrap_or(0) as u8,
        count: data.get("num").and_then(Value::as_u64).unwrap_or(0),
        price: data.get("price").and_then(Value::as_f64).unwrap_or(0.0) / 1000.0,
        timestamp: ts_from_secs_or_now(data.get("start_time").and_then(Value::as_i64)),
    }))
}

fn parse_interact(room_id: u64, v: &Value) -> Option<LiveEvent> {
    let data = v.get("data")?;
    // msg_type 1 = enter, 2 = follow, 3 = share.
    let msg_type = data.get("msg_type").and_then(Value::as_u64).unwrap_or(1);
    if msg_type != 1 {
        return None;
    }
    Some(LiveEvent::Enter(EnterMsg {
        room_id,
        uid: data.get("uid").and_then(Value::as_u64).unwrap_or(0),
        username: data
            .get("uname")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        timestamp: ts_from_secs_or_now(data.get("timestamp").and_then(Value::as_i64)),
    }))
}

fn parse_like(room_id: u64, v: &Value) -> Option<LiveEvent> {
    let data = v.get("data")?;
    Some(LiveEvent::Like(LikeMsg {
        room_id,
        uid: data.get("uid").and_then(Value::as_u64).unwrap_or(0),
        username: data
            .get("uname")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        timestamp: Utc::now(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_plain_danmaku() {
        let v = json!({
            "cmd": "DANMU_MSG",
            "info": [
                [0,1,25,16777215,1700000000000i64,0,0,"",0,0,0],
                "你好世界",
                [12345, "观众A", 0, 0, 0, 10000, 1, ""],
                [21, "牌子", "主播", 555, 6809855, "", 0],
                [], [], 0, 3
            ]
        });
        let ev = parse_cmd(100, &v).unwrap();
        match ev {
            LiveEvent::Danmaku(d) => {
                assert_eq!(d.text, "你好世界");
                assert_eq!(d.uid, 12345);
                assert_eq!(d.username, "观众A");
                assert_eq!(d.guard_level, 3);
                let medal = d.medal.unwrap();
                assert_eq!(medal.level, 21);
                assert_eq!(medal.name, "牌子");
                assert_eq!(medal.anchor_room_id, 555);
            }
            _ => panic!("expected danmaku"),
        }
    }

    #[test]
    fn parse_danmaku_with_cmd_suffix() {
        let v = json!({
            "cmd": "DANMU_MSG:4:0:2:2:2:0",
            "info": [[0,1,25,0,1700000000000i64],"hi",[1,"u",0],[],[],0,0]
        });
        assert!(matches!(parse_cmd(1, &v), Some(LiveEvent::Danmaku(_))));
    }

    #[test]
    fn parse_super_chat_msg() {
        let v = json!({
            "cmd": "SUPER_CHAT_MESSAGE",
            "data": {
                "uid": 999,
                "price": 30,
                "time": 60,
                "start_time": 1700000000,
                "message": "感谢主播",
                "user_info": { "uname": "壕友" }
            }
        });
        match parse_cmd(100, &v).unwrap() {
            LiveEvent::SuperChat(sc) => {
                assert_eq!(sc.price, 30.0);
                assert_eq!(sc.text, "感谢主播");
                assert_eq!(sc.username, "壕友");
                assert_eq!(sc.duration_secs, 60);
            }
            _ => panic!("expected super chat"),
        }
    }

    #[test]
    fn parse_gold_gift_has_value() {
        let v = json!({
            "cmd": "SEND_GIFT",
            "data": {
                "uid": 7, "uname": "土豪", "giftName": "小心心", "giftId": 30607,
                "num": 2, "total_coin": 2000, "coin_type": "gold", "timestamp": 1700000000
            }
        });
        match parse_cmd(1, &v).unwrap() {
            LiveEvent::Gift(g) => {
                assert_eq!(g.count, 2);
                assert_eq!(g.total_price, 2.0);
                assert_eq!(g.gift_name, "小心心");
            }
            _ => panic!("expected gift"),
        }
    }

    #[test]
    fn parse_silver_gift_is_free() {
        let v = json!({
            "cmd": "SEND_GIFT",
            "data": {
                "uid": 7, "uname": "路人", "giftName": "辣条", "giftId": 1,
                "num": 5, "total_coin": 0, "coin_type": "silver", "timestamp": 1700000000
            }
        });
        match parse_cmd(1, &v).unwrap() {
            LiveEvent::Gift(g) => assert_eq!(g.total_price, 0.0),
            _ => panic!("expected gift"),
        }
    }

    #[test]
    fn parse_guard() {
        let v = json!({
            "cmd": "GUARD_BUY",
            "data": {
                "uid": 5, "username": "舰长大人", "guard_level": 3, "num": 1,
                "price": 198000, "start_time": 1700000000
            }
        });
        match parse_cmd(1, &v).unwrap() {
            LiveEvent::GuardBuy(g) => {
                assert_eq!(g.guard_level, 3);
                assert_eq!(g.price, 198.0);
                assert_eq!(g.username, "舰长大人");
            }
            _ => panic!("expected guard"),
        }
    }

    #[test]
    fn parse_enter_only_for_enter_type() {
        let enter = json!({
            "cmd": "INTERACT_WORD",
            "data": { "uid": 3, "uname": "新人", "msg_type": 1, "timestamp": 1700000000 }
        });
        assert!(matches!(parse_cmd(1, &enter), Some(LiveEvent::Enter(_))));

        let follow = json!({
            "cmd": "INTERACT_WORD",
            "data": { "uid": 3, "uname": "新人", "msg_type": 2, "timestamp": 1700000000 }
        });
        assert!(parse_cmd(1, &follow).is_none());
    }

    #[test]
    fn watched_change_and_live_states() {
        let w = json!({"cmd":"WATCHED_CHANGE","data":{"num":1234}});
        assert!(matches!(
            parse_cmd(1, &w),
            Some(LiveEvent::WatchedChange { count: 1234, .. })
        ));
        assert!(matches!(
            parse_cmd(1, &json!({"cmd":"LIVE"})),
            Some(LiveEvent::LiveStart { .. })
        ));
        assert!(matches!(
            parse_cmd(1, &json!({"cmd":"PREPARING"})),
            Some(LiveEvent::LiveEnd { .. })
        ));
    }

    #[test]
    fn unknown_cmd_returns_none() {
        assert!(parse_cmd(1, &json!({"cmd":"ONLINE_RANK_V2"})).is_none());
        assert!(parse_cmd(1, &json!({"foo":"bar"})).is_none());
    }
}
