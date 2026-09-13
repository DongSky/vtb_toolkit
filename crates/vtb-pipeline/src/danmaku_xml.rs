//! Danmaku XML export in the Bilibili-compatible format used by the
//! 录播姬 (BililiveRecorder) ecosystem: standard `<d>` chat elements plus
//! the `<sc>` / `<gift>` / `<guard>` extensions.

use crate::danmaku_log::LogEntry;
use crate::error::Result;
use chrono::{DateTime, Utc};
use std::path::Path;
use vtb_common::LiveEvent;

/// Escape the five XML special characters for text nodes and attributes.
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Seconds relative to session start; `None` drops pre-start events.
fn rel_secs(received_at: DateTime<Utc>, session_start: DateTime<Utc>) -> Option<f64> {
    let ms = (received_at - session_start).num_milliseconds();
    (ms >= 0).then(|| ms as f64 / 1000.0)
}

/// Build a Bilibili-compatible danmaku XML document from log entries.
///
/// Event times are relative to `session_start` (3 decimal places, seconds);
/// events received before the session start are dropped. Events other than
/// Danmaku / SuperChat / Gift / GuardBuy are skipped.
pub fn to_xml(entries: &[LogEntry], session_start: DateTime<Utc>) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <i>\n\
         <chatserver>chat.bilibili.com</chatserver>\n\
         <chatid>0</chatid>\n\
         <mission>0</mission>\n\
         <maxlimit>1000</maxlimit>\n",
    );
    for entry in entries {
        let Some(time) = rel_secs(entry.received_at, session_start) else {
            continue;
        };
        if let Some(source) = &entry.source {
            if source.platform == vtb_common::LivePlatform::Youtube {
                let text = match &entry.event {
                    LiveEvent::Danmaku(d) => format!("{}: {}", d.username, d.text),
                    LiveEvent::SuperChat(sc) => format!(
                        "{} {}: {}",
                        sc.username,
                        source
                            .money
                            .as_ref()
                            .map(|m| m.display.as_str())
                            .unwrap_or("Super Chat"),
                        sc.text
                    ),
                    LiveEvent::Gift(g) => format!("{}: {}", g.username, g.gift_name),
                    LiveEvent::GuardBuy(g) => format!(
                        "{}: {}",
                        g.username,
                        source.membership.as_deref().unwrap_or("频道会员")
                    ),
                    _ => continue,
                };
                let metadata = serde_json::to_string(source).unwrap_or_default();
                out.push_str(&format!(
                    "<d p=\"{time:.3},1,25,16777215,{},0,0,0\" vtb_source=\"{}\">{}</d>\n",
                    entry.received_at.timestamp(),
                    xml_escape(&metadata),
                    xml_escape(&text)
                ));
                continue;
            }
        }
        match &entry.event {
            // p = time,mode,size,color,unix_ts,pool,uid,row — mode 1 (滚动),
            // size 25, color 16777215 (white).
            LiveEvent::Danmaku(d) => {
                out.push_str(&format!(
                    "<d p=\"{time:.3},1,25,16777215,{},0,{},0\">{}</d>\n",
                    d.timestamp.timestamp(),
                    d.uid,
                    xml_escape(&d.text)
                ));
            }
            LiveEvent::SuperChat(sc) => {
                out.push_str(&format!(
                    "<sc p=\"{time:.3},{},{},{}\">{}</sc>\n",
                    sc.price.round() as i64,
                    sc.uid,
                    sc.timestamp.timestamp(),
                    xml_escape(&sc.text)
                ));
            }
            LiveEvent::Gift(g) => {
                out.push_str(&format!(
                    "<gift p=\"{time:.3},{},{}\" giftname=\"{}\" giftcount=\"{}\"/>\n",
                    g.uid,
                    g.timestamp.timestamp(),
                    xml_escape(&g.gift_name),
                    g.count
                ));
            }
            LiveEvent::GuardBuy(gb) => {
                out.push_str(&format!(
                    "<guard p=\"{time:.3},{},{}\" level=\"{}\" count=\"{}\"/>\n",
                    gb.uid,
                    gb.timestamp.timestamp(),
                    gb.guard_level,
                    gb.count
                ));
            }
            _ => {}
        }
    }
    out.push_str("</i>\n");
    out
}

/// Write the danmaku XML for `entries` to `path`.
pub fn write_xml(entries: &[LogEntry], session_start: DateTime<Utc>, path: &Path) -> Result<()> {
    std::fs::write(path, to_xml(entries, session_start))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use vtb_common::{DanmakuMsg, GiftMsg, GuardBuyMsg, SuperChatMsg};

    const BASE: i64 = 1_700_000_000;

    fn start() -> DateTime<Utc> {
        Utc.timestamp_opt(BASE, 0).single().unwrap()
    }

    fn at_ms(offset_ms: i64) -> DateTime<Utc> {
        start() + chrono::Duration::milliseconds(offset_ms)
    }

    fn danmaku(offset_ms: i64, uid: u64, text: &str) -> LogEntry {
        let ts = at_ms(offset_ms);
        LogEntry {
            source: None,
            received_at: ts,
            event: LiveEvent::Danmaku(DanmakuMsg {
                room_id: 1,
                uid,
                username: "u".into(),
                text: text.into(),
                timestamp: ts,
                medal: None,
                guard_level: 0,
                is_admin: false,
                emoticon: None,
            }),
        }
    }

    fn superchat(offset_ms: i64, uid: u64, price: f64, text: &str) -> LogEntry {
        let ts = at_ms(offset_ms);
        LogEntry {
            source: None,
            received_at: ts,
            event: LiveEvent::SuperChat(SuperChatMsg {
                room_id: 1,
                uid,
                username: "u".into(),
                text: text.into(),
                price,
                timestamp: ts,
                duration_secs: 60,
            }),
        }
    }

    fn gift(offset_ms: i64, uid: u64, name: &str, count: u64) -> LogEntry {
        let ts = at_ms(offset_ms);
        LogEntry {
            source: None,
            received_at: ts,
            event: LiveEvent::Gift(GiftMsg {
                room_id: 1,
                uid,
                username: "u".into(),
                gift_name: name.into(),
                gift_id: 1,
                count,
                total_price: 0.0,
                timestamp: ts,
            }),
        }
    }

    fn guard(offset_ms: i64, uid: u64, level: u8, count: u64) -> LogEntry {
        let ts = at_ms(offset_ms);
        LogEntry {
            source: None,
            received_at: ts,
            event: LiveEvent::GuardBuy(GuardBuyMsg {
                room_id: 1,
                uid,
                username: "u".into(),
                guard_level: level,
                count,
                price: 198.0,
                timestamp: ts,
            }),
        }
    }

    #[test]
    fn escapes_special_characters() {
        assert_eq!(xml_escape(r#"<&>"'"#), "&lt;&amp;&gt;&quot;&apos;");
        assert_eq!(xml_escape("弹幕"), "弹幕");
    }

    #[test]
    fn danmaku_element_format() {
        let xml = to_xml(&[danmaku(1500, 42, "草")], start());
        let expected = format!(
            "<d p=\"1.500,1,25,16777215,{},0,42,0\">草</d>",
            BASE + 1 // event timestamp truncated to whole seconds
        );
        assert!(xml.contains(&expected), "xml was:\n{xml}");
    }

    #[test]
    fn header_and_footer_present() {
        let xml = to_xml(&[danmaku(0, 1, "hi")], start());
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<i>\n"));
        assert!(xml.contains("<chatserver>chat.bilibili.com</chatserver>"));
        assert!(xml.contains("<chatid>0</chatid>"));
        assert!(xml.contains("<mission>0</mission>"));
        assert!(xml.contains("<maxlimit>1000</maxlimit>"));
        assert!(xml.ends_with("</i>\n"));
    }

    #[test]
    fn text_is_escaped_in_output() {
        let xml = to_xml(&[danmaku(0, 1, r#"A<B> & "中文" 'q'"#)], start());
        assert!(xml.contains(">A&lt;B&gt; &amp; &quot;中文&quot; &apos;q&apos;</d>"));
        // No raw specials leak into the text node.
        assert!(!xml.contains("A<B>"));
    }

    #[test]
    fn superchat_extension() {
        let xml = to_xml(&[superchat(2000, 7, 30.4, "加油&<3")], start());
        let expected = format!("<sc p=\"2.000,30,7,{}\">加油&amp;&lt;3</sc>", BASE + 2);
        assert!(xml.contains(&expected), "xml was:\n{xml}");
    }

    #[test]
    fn youtube_paid_xml_preserves_currency_and_source_without_cny_extension() {
        let mut entry = superchat(2000, 0, 20.0, "<加油>");
        entry.source = Some(serde_json::from_value(serde_json::json!({"platform":"youtube","room_id":"video","user_id":"author","money":{"display":"HK$20.00","currency":"HKD","amount":20}})).unwrap());
        let xml = to_xml(&[entry], start());
        assert!(xml.contains("HK$20.00"));
        assert!(xml.contains("&lt;加油&gt;"));
        assert!(xml.contains("vtb_source="));
        assert!(!xml.contains("<sc "));
    }

    #[test]
    fn gift_extension_escapes_attribute() {
        let xml = to_xml(&[gift(500, 9, "小心心<&>", 10)], start());
        let expected = format!(
            "<gift p=\"0.500,9,{}\" giftname=\"小心心&lt;&amp;&gt;\" giftcount=\"10\"/>",
            BASE
        );
        assert!(xml.contains(&expected), "xml was:\n{xml}");
    }

    #[test]
    fn guard_extension() {
        let xml = to_xml(&[guard(3250, 11, 3, 1)], start());
        let expected = format!(
            "<guard p=\"3.250,11,{}\" level=\"3\" count=\"1\"/>",
            BASE + 3
        );
        assert!(xml.contains(&expected), "xml was:\n{xml}");
    }

    #[test]
    fn pre_session_events_dropped_and_others_skipped() {
        let ts = at_ms(1000);
        let entries = vec![
            danmaku(-2000, 1, "太早了"),
            danmaku(1000, 2, "正好"),
            LogEntry {
                source: None,
                received_at: ts,
                event: LiveEvent::LiveStart {
                    room_id: 1,
                    timestamp: ts,
                },
            },
            LogEntry {
                source: None,
                received_at: ts,
                event: LiveEvent::WatchedChange {
                    room_id: 1,
                    count: 5,
                },
            },
        ];
        let xml = to_xml(&entries, start());
        assert!(!xml.contains("太早了"));
        assert!(xml.contains("正好"));
        // Exactly one <d> element, nothing emitted for LiveStart/WatchedChange.
        assert_eq!(xml.matches("<d p=").count(), 1);
        assert!(!xml.contains("live_start"));
        assert!(!xml.contains("watched"));
    }

    #[test]
    fn mixed_events_appear_in_entry_order() {
        let entries = vec![
            danmaku(0, 1, "一"),
            superchat(1000, 2, 50.0, "二"),
            gift(2000, 3, "辣条", 99),
            guard(3000, 4, 2, 1),
        ];
        let xml = to_xml(&entries, start());
        let d = xml.find("<d p=").unwrap();
        let sc = xml.find("<sc p=").unwrap();
        let g = xml.find("<gift p=").unwrap();
        let gd = xml.find("<guard p=").unwrap();
        assert!(d < sc && sc < g && g < gd);
    }

    #[test]
    fn empty_input_yields_valid_empty_document() {
        let xml = to_xml(&[], start());
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<i>\n"));
        assert!(xml.ends_with("<maxlimit>1000</maxlimit>\n</i>\n"));
        assert!(!xml.contains("<d "));
    }

    #[test]
    fn write_xml_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("danmaku.xml");
        let entries = vec![danmaku(0, 1, "你好<世界>"), superchat(500, 2, 30.0, "SC&")];
        write_xml(&entries, start(), &path).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written, to_xml(&entries, start()));
        assert!(written.contains("你好&lt;世界&gt;"));
        assert!(written.contains("SC&amp;"));
    }
}
