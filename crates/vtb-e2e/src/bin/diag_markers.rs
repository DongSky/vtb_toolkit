//! Diagnostic: 打点 (markers) end-to-end — synthesizes a session directory
//! with a manifest, a danmaku burst, and manual/auto markers, then runs
//! the offline marker merge exactly like the pipeline does and verifies
//! offsets align with the recording timeline.
//!
//! Usage: cargo run -p vtb-e2e --bin diag_markers
//! (No live room needed — this validates the timeline math and file
//! formats; live-room marker writing is covered by the app path.)

use chrono::{Duration, Utc};
use vtb_common::{DanmakuMsg, LiveEvent};
use vtb_pipeline::danmaku_log::{DanmakuLogWriter, LogEntry};
use vtb_pipeline::markers::{self, Marker, MarkerKind};

fn main() {
    let dir = tempfile::tempdir().unwrap();
    let session = dir.path();
    let start = Utc::now() - Duration::seconds(600);
    println!("会话目录: {}", session.display());
    println!("session_start: {start}");

    // 1. Manifest, like the recorder writes.
    std::fs::write(
        session.join("rec.meta.json"),
        serde_json::json!({
            "room_id": 320,
            "title": "diag",
            "started_at": start.to_rfc3339(),
            "ended_at": (start + Duration::seconds(600)).to_rfc3339(),
            "segments": [],
        })
        .to_string(),
    )
    .unwrap();

    // 2. Danmaku burst at t=120s (20 messages in 1s).
    {
        let mut w = DanmakuLogWriter::create(&session.join("danmaku.jsonl")).unwrap();
        for i in 0..20 {
            let ts = start + Duration::milliseconds(120_000 + i * 50);
            w.write(&LogEntry {
                source: None,
                received_at: ts,
                event: LiveEvent::Danmaku(DanmakuMsg {
                    room_id: 320,
                    uid: i as u64,
                    username: "u".into(),
                    text: "草".into(),
                    timestamp: ts,
                    medal: None,
                    guard_level: 0,
                    is_admin: false,
                    emoticon: None,
                }),
            })
            .unwrap();
        }
        w.flush().unwrap();
    }

    // 3. Markers: manual at t=300s (uncovered) + auto at t=120s (covered
    //    by the burst window a real detector would flag).
    let manual = Marker {
        created_at: start + Duration::seconds(300),
        kind: MarkerKind::Manual,
        source: "hotkey".into(),
        note: Some("名场面".into()),
    };
    let auto = Marker {
        created_at: start + Duration::seconds(120),
        kind: MarkerKind::Auto,
        source: "realtime".into(),
        note: Some("弹幕爆发 z=3.2".into()),
    };
    markers::append_marker(session, &manual).unwrap();
    markers::append_marker(session, &auto).unwrap();

    // 4. Read back → offsets (must survive the JSONL roundtrip).
    let read = markers::read_markers(session).unwrap();
    assert_eq!(read.len(), 2, "两个标记都应读回");
    let offsets = markers::to_offsets(&read, start);
    println!("\n标记偏移:");
    for m in &offsets {
        println!(
            "  {} [{:?}/{}] {:?}",
            markers::hms(m.at_ms),
            m.marker.kind,
            m.marker.source,
            m.marker.note
        );
    }
    assert_eq!(offsets[0].at_ms, 300_000, "手动标记应在 05:00");
    assert_eq!(offsets[1].at_ms, 120_000, "自动标记应在 02:00");

    // 5. Merge into a detected-highlight set: the burst at 120s is a
    //    highlight; 300s is uncovered → must be promoted.
    let mut highlights = vec![vtb_common::Highlight {
        start_ms: 115_000,
        end_ms: 135_000,
        score: 0.7,
        reason: "弹幕密度峰值".into(),
        signals: Default::default(),
        title: None,
    }];
    markers::merge_marker_highlights(&mut highlights, &offsets, 15_000, 15_000, 600_000);
    println!("\n合并后高能片段:");
    for h in &highlights {
        println!(
            "  {}-{} score={:.2} {} {:?}",
            markers::hms(h.start_ms),
            markers::hms(h.end_ms),
            h.score,
            h.reason,
            h.title
        );
    }
    assert_eq!(highlights.len(), 2, "手动标记应新增一个候选");
    assert_eq!(highlights[1].start_ms, 285_000);
    assert_eq!(highlights[1].end_ms, 315_000);
    assert_eq!(highlights[1].title.as_deref(), Some("名场面"));

    // 6. Timestamp list (B站评论格式).
    let list = markers::timestamp_list(&offsets, &highlights);
    println!("\ntimestamps.txt:\n{list}");
    assert!(list.contains("00:02:00"));
    assert!(list.contains("00:05:00 名场面"));

    println!("\n✅ diag_markers 全部通过");
}
