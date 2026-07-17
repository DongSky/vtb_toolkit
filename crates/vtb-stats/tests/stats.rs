use chrono::{DateTime, Duration, TimeZone, Utc};
use vtb_common::{
    DanmakuMsg, EnterMsg, GiftMsg, GuardBuyMsg, LikeMsg, LiveEvent, SuperChatMsg,
};
use vtb_stats::{export_markdown, export_superchats_csv, StatsDb};

const ROOM: u64 = 42;
const SESSION: &str = "2026-07-17-evening";

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 7, 17, 12, 0, 0).unwrap()
}

fn at(secs: i64) -> DateTime<Utc> {
    t0() + Duration::seconds(secs)
}

fn danmaku(uid: u64, username: &str, text: &str, ts: DateTime<Utc>) -> LiveEvent {
    LiveEvent::Danmaku(DanmakuMsg {
        room_id: ROOM,
        uid,
        username: username.to_string(),
        text: text.to_string(),
        timestamp: ts,
        medal: None,
        guard_level: 0,
        is_admin: false,
        emoticon: None,
    })
}

fn superchat(uid: u64, username: &str, text: &str, price: f64, ts: DateTime<Utc>) -> LiveEvent {
    LiveEvent::SuperChat(SuperChatMsg {
        room_id: ROOM,
        uid,
        username: username.to_string(),
        text: text.to_string(),
        price,
        timestamp: ts,
        duration_secs: 60,
    })
}

fn gift(uid: u64, username: &str, name: &str, count: u64, total: f64, ts: DateTime<Utc>) -> LiveEvent {
    LiveEvent::Gift(GiftMsg {
        room_id: ROOM,
        uid,
        username: username.to_string(),
        gift_name: name.to_string(),
        gift_id: 1,
        count,
        total_price: total,
        timestamp: ts,
    })
}

fn guard(uid: u64, username: &str, level: u8, count: u64, price: f64, ts: DateTime<Utc>) -> LiveEvent {
    LiveEvent::GuardBuy(GuardBuyMsg {
        room_id: ROOM,
        uid,
        username: username.to_string(),
        guard_level: level,
        count,
        price,
        timestamp: ts,
    })
}

fn enter(uid: u64, username: &str, ts: DateTime<Utc>) -> LiveEvent {
    LiveEvent::Enter(EnterMsg {
        room_id: ROOM,
        uid,
        username: username.to_string(),
        timestamp: ts,
    })
}

/// Populate a db with a mixed set of events for SESSION.
fn populate(db: &StatsDb) {
    let events: Vec<(LiveEvent, DateTime<Utc>)> = vec![
        // 5 danmaku from 3 unique chatters.
        (danmaku(1, "Alice", "哈哈哈草", at(1)), at(1)),
        (danmaku(1, "Alice", "hello world hello", at(2)), at(2)),
        (danmaku(1, "Alice", "草", at(3)), at(3)),
        (danmaku(2, "Bob", "123 456 ok!", at(4)), at(4)),
        (danmaku(3, "查理", "他说,\"你好,世界\"", at(5)), at(5)),
        // Enters and likes.
        (enter(2, "Bob", at(6)), at(6)),
        (enter(4, "Dave", at(7)), at(7)),
        (
            LiveEvent::Like(LikeMsg {
                room_id: ROOM,
                uid: 2,
                username: "Bob".into(),
                timestamp: at(8),
            }),
            at(8),
        ),
        // SuperChats (out of insertion order vs received_at on purpose).
        (
            superchat(5, "SC-User", "说\"你好\",世界|管道", 30.0, at(10)),
            at(10),
        ),
        (superchat(2, "Bob", "加油", 50.0, at(9)), at(9)),
        // Gifts (one free).
        (gift(2, "Bob", "小花花", 5, 5.0, at(11)), at(11)),
        (gift(3, "查理", "牛哇", 1, 10.0, at(12)), at(12)),
        (gift(4, "Dave", "免费赞", 10, 0.0, at(13)), at(13)),
        // Guard purchases.
        (guard(6, "Rich", 3, 1, 138.0, at(20)), at(20)),
        (guard(7, "Whale", 2, 2, 3996.0, at(30)), at(30)),
        // Ignored event kinds.
        (
            LiveEvent::LiveStart { room_id: ROOM, timestamp: at(0) },
            at(0),
        ),
        (
            LiveEvent::LiveEnd { room_id: ROOM, timestamp: at(40) },
            at(40),
        ),
        (LiveEvent::WatchedChange { room_id: ROOM, count: 999 }, at(15)),
    ];
    for (ev, ts) in &events {
        db.record(SESSION, ev, *ts).unwrap();
    }
}

#[test]
fn mixed_events_report() {
    let db = StatsDb::open_memory().unwrap();
    populate(&db);
    let r = db.session_report(SESSION).unwrap();

    assert_eq!(r.session_id, SESSION);
    assert_eq!(r.danmaku_count, 5);
    assert_eq!(r.unique_chatters, 3);
    assert_eq!(r.enter_count, 2);
    assert_eq!(r.sc_total, 80.0);
    assert_eq!(r.gift_total, 15.0);
    assert_eq!(r.guard_count, 3); // 1 + 2 units
    assert_eq!(r.revenue_total, 80.0 + 15.0 + 138.0 + 3996.0);

    // Top chatters: Alice(3) first, then Bob(1)/查理(1) tie broken by name.
    assert_eq!(r.top_chatters.len(), 3);
    assert_eq!(r.top_chatters[0], ("Alice".to_string(), 3));
    assert_eq!(r.top_chatters[1].1, 1);

    // SuperChats sorted by received_at: Bob's (t+9) before SC-User's (t+10).
    assert_eq!(r.superchats.len(), 2);
    assert_eq!(r.superchats[0].username, "Bob");
    assert_eq!(r.superchats[0].price, 50.0);
    assert_eq!(r.superchats[1].text, "说\"你好\",世界|管道");
    assert_eq!(r.superchats[1].received_at, at(10));

    // Guards preserved with level.
    assert_eq!(r.guards.len(), 2);
    assert_eq!(r.guards[0].username, "Rich");
    assert_eq!(r.guards[0].level, 3);
    assert_eq!(r.guards[1].level, 2);
    assert_eq!(r.guards[1].count, 2);

    // Word freq: expected bigrams/words present, junk filtered.
    let get = |w: &str| r.word_freq.iter().find(|(t, _)| t == w).map(|(_, n)| *n);
    assert_eq!(get("哈哈"), Some(2)); // from 哈哈哈
    assert_eq!(get("hello"), Some(2));
    assert_eq!(get("你好"), Some(1));
    assert_eq!(get("世界"), Some(1));
    assert_eq!(get("123"), None); // pure numeric filtered
    assert!(r.word_freq.iter().all(|(t, _)| t.chars().count() >= 2));
    // Ties at count 2 sorted by token: "hello" < "哈哈".
    assert_eq!(r.word_freq[0], ("hello".to_string(), 2));
    assert_eq!(r.word_freq[1], ("哈哈".to_string(), 2));
}

#[test]
fn markdown_contains_key_lines() {
    let db = StatsDb::open_memory().unwrap();
    populate(&db);
    let r = db.session_report(SESSION).unwrap();
    let md = export_markdown(&r);

    assert!(md.contains("# 场次统计报告:2026-07-17-evening"));
    assert!(md.contains("| 弹幕数 | 5 |"));
    assert!(md.contains("| SC 总额 | ¥80.00 |"));
    assert!(md.contains("| 礼物总额 | ¥15.00 |"));
    assert!(md.contains("| 舰长数 | 3 |"));
    assert!(md.contains("| 总营收 | ¥4229.00 |"));
    // SC row with pipe escaped so the table is not broken.
    assert!(md.contains("说\"你好\",世界\\|管道"));
    assert!(md.contains("| 2026-07-17 12:00:09 | Bob | ¥50.00 | 加油 |"));
    // Guard row with level name.
    assert!(md.contains("| 2026-07-17 12:00:20 | Rich | 舰长 | 1 | ¥138.00 |"));
    assert!(md.contains("提督"));
    // Top chatters section.
    assert!(md.contains("## TOP 弹幕用户"));
    assert!(md.contains("| Alice | 3 |"));
}

#[test]
fn csv_escaping() {
    let db = StatsDb::open_memory().unwrap();
    populate(&db);
    let r = db.session_report(SESSION).unwrap();
    let csv = export_superchats_csv(&r);

    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines[0], "username,price,text,received_at");
    assert_eq!(lines[1], "Bob,50.00,加油,2026-07-17T12:00:09+00:00");
    // Text containing quotes and commas must be quoted with doubled quotes.
    assert_eq!(
        lines[2],
        "SC-User,30.00,\"说\"\"你好\"\",世界|管道\",2026-07-17T12:00:10+00:00"
    );
    assert_eq!(lines.len(), 3);
}

#[test]
fn empty_session_report_is_all_zero() {
    let db = StatsDb::open_memory().unwrap();
    populate(&db); // other session's data must not leak in
    let r = db.session_report("no-such-session").unwrap();

    assert_eq!(r.danmaku_count, 0);
    assert_eq!(r.unique_chatters, 0);
    assert_eq!(r.enter_count, 0);
    assert_eq!(r.sc_total, 0.0);
    assert_eq!(r.gift_total, 0.0);
    assert_eq!(r.guard_count, 0);
    assert_eq!(r.revenue_total, 0.0);
    assert!(r.top_chatters.is_empty());
    assert!(r.superchats.is_empty());
    assert!(r.guards.is_empty());
    assert!(r.word_freq.is_empty());

    // Exports still work on an empty report.
    let md = export_markdown(&r);
    assert!(md.contains("| 弹幕数 | 0 |"));
    assert!(md.contains("(无)"));
    assert_eq!(export_superchats_csv(&r), "username,price,text,received_at\n");
}

#[test]
fn report_is_serializable() {
    let db = StatsDb::open_memory().unwrap();
    populate(&db);
    let r = db.session_report(SESSION).unwrap();
    let json = serde_json::to_string(&r).unwrap();
    assert!(json.contains("\"danmaku_count\":5"));
    assert!(json.contains("\"session_id\":\"2026-07-17-evening\""));
}

#[test]
fn file_db_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stats").join("events.sqlite");

    {
        let db = StatsDb::open(&path).unwrap();
        populate(&db);
    } // drop closes the connection

    let db = StatsDb::open(&path).unwrap();
    let r = db.session_report(SESSION).unwrap();
    assert_eq!(r.danmaku_count, 5);
    assert_eq!(r.sc_total, 80.0);
    assert_eq!(r.guard_count, 3);
    assert_eq!(r.superchats[1].text, "说\"你好\",世界|管道");
    assert_eq!(r.guards[0].received_at, at(20));
}
