//! 打点 (markers): timestamped points of interest recorded during a live
//! session — automatic (realtime highlight alerts) or manual (hotkey /
//! button / danmaku command).
//!
//! Entries are stored as wall-clock JSONL (`markers.jsonl`) in the session
//! directory, exactly like `danmaku.jsonl`; offsets into the recording are
//! computed against the manifest's `started_at` at read time. Storing wall
//! clock instead of ad-hoc offsets is what keeps markers aligned with the
//! video regardless of which subsystem (danmaku pump, hotkey handler)
//! produced them.

use crate::error::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKind {
    /// Realtime highlight detector fired.
    Auto,
    /// A human decided this moment matters.
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Marker {
    pub created_at: DateTime<Utc>,
    pub kind: MarkerKind,
    /// Where it came from: "realtime" | "hotkey" | "button" | "danmaku".
    pub source: String,
    /// Free-form note ("名场面", auto-alert reason, danmaku command text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl Marker {
    pub fn manual(source: impl Into<String>, note: Option<String>) -> Self {
        Self {
            created_at: Utc::now(),
            kind: MarkerKind::Manual,
            source: source.into(),
            note,
        }
    }

    pub fn auto(note: impl Into<String>) -> Self {
        Self {
            created_at: Utc::now(),
            kind: MarkerKind::Auto,
            source: "realtime".into(),
            note: Some(note.into()),
        }
    }
}

/// Marker with its computed offset into the recording.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OffsetMarker {
    pub at_ms: u64,
    #[serde(flatten)]
    pub marker: Marker,
}

/// Append one marker to `<dir>/markers.jsonl` (created on first write).
/// Opens per call: marker writes are rare (a handful per stream), and
/// open-append-close means concurrent writers (danmaku pump + hotkey
/// handler) never hold a file handle across await points.
pub fn append_marker(dir: &Path, marker: &Marker) -> Result<()> {
    let path = dir.join("markers.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    serde_json::to_writer(&mut file, marker)?;
    file.write_all(b"\n")?;
    Ok(())
}

/// Read `<dir>/markers.jsonl`; missing file → empty, malformed lines skipped.
pub fn read_markers(dir: &Path) -> Result<Vec<Marker>> {
    let path = dir.join("markers.jsonl");
    if !path.exists() {
        return Ok(vec![]);
    }
    let reader = BufReader::new(std::fs::File::open(path)?);
    let mut out = Vec::new();
    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Marker>(&line) {
            Ok(m) => out.push(m),
            Err(e) => tracing::warn!("skipping malformed marker line {}: {e}", i + 1),
        }
    }
    Ok(out)
}

/// Compute offsets against a session start, dropping pre-start markers
/// (same convention as [`crate::danmaku_log::to_offsets`]).
pub fn to_offsets(markers: &[Marker], session_start: DateTime<Utc>) -> Vec<OffsetMarker> {
    markers
        .iter()
        .filter_map(|m| {
            let ms = (m.created_at - session_start).num_milliseconds();
            (ms >= 0).then(|| OffsetMarker {
                at_ms: ms as u64,
                marker: m.clone(),
            })
        })
        .collect()
}

/// `HH:MM:SS` for a millisecond offset (B站评论/简介 timestamp format).
pub fn hms(ms: u64) -> String {
    let s = ms / 1000;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// Render a plain-text timestamp list (markers + highlights merged,
/// sorted) ready to paste into a B站 comment or video description.
pub fn timestamp_list(markers: &[OffsetMarker], highlights: &[vtb_common::Highlight]) -> String {
    let mut lines: Vec<(u64, String)> = Vec::new();
    for m in markers {
        let label = m
            .marker
            .note
            .clone()
            .unwrap_or_else(|| match m.marker.kind {
                MarkerKind::Auto => "高能时刻".into(),
                MarkerKind::Manual => "打点".into(),
            });
        lines.push((m.at_ms, label));
    }
    for h in highlights {
        let label = h.title.clone().unwrap_or_else(|| h.reason.clone());
        lines.push((h.start_ms, label));
    }
    lines.sort_by_key(|(ms, _)| *ms);
    lines.dedup();
    lines
        .into_iter()
        .map(|(ms, label)| format!("{} {label}", hms(ms)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Fold manual markers into detected highlights as a strong prior: a
/// marker inside an existing highlight boosts its score; a marker not
/// covered by any highlight becomes its own `[at-pre, at+post]` candidate
/// (someone pressed the button — that moment ships even if the signal
/// fusion missed it).
pub fn merge_marker_highlights(
    highlights: &mut Vec<vtb_common::Highlight>,
    markers: &[OffsetMarker],
    pre_ms: u64,
    post_ms: u64,
    total_ms: u64,
) {
    for m in markers {
        if m.marker.kind != MarkerKind::Manual {
            continue; // auto markers came from the same signals fusion saw
        }
        if m.at_ms >= total_ms {
            continue;
        }
        if let Some(h) = highlights
            .iter_mut()
            .find(|h| h.start_ms <= m.at_ms && m.at_ms <= h.end_ms)
        {
            h.score = (h.score + 0.2).min(1.0);
            if !h.reason.contains("手动打点") {
                h.reason = format!("{} + 手动打点", h.reason);
            }
            continue;
        }
        let start_ms = m.at_ms.saturating_sub(pre_ms);
        let end_ms = m.at_ms.saturating_add(post_ms).min(total_ms);
        highlights.push(vtb_common::Highlight {
            start_ms,
            end_ms,
            score: 0.8,
            reason: "手动打点".into(),
            signals: Default::default(),
            title: m.marker.note.clone().filter(|n| !n.is_empty()),
        });
    }
    highlights.sort_by_key(|h| h.start_ms);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use vtb_common::{Highlight, HighlightSignals};

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_700_000_000 + secs, 0).single().unwrap()
    }

    fn manual_at(secs: i64, note: Option<&str>) -> Marker {
        Marker {
            created_at: at(secs),
            kind: MarkerKind::Manual,
            source: "hotkey".into(),
            note: note.map(String::from),
        }
    }

    fn hl(start_ms: u64, end_ms: u64) -> Highlight {
        Highlight {
            start_ms,
            end_ms,
            score: 0.5,
            reason: "弹幕密度峰值".into(),
            signals: HighlightSignals::default(),
            title: None,
        }
    }

    #[test]
    fn roundtrip_jsonl_and_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_markers(dir.path()).unwrap().is_empty());

        append_marker(dir.path(), &manual_at(5, Some("名场面"))).unwrap();
        append_marker(dir.path(), &Marker::auto("弹幕爆发 z=3.1")).unwrap();

        let markers = read_markers(dir.path()).unwrap();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].note.as_deref(), Some("名场面"));
        assert_eq!(markers[1].kind, MarkerKind::Auto);
    }

    #[test]
    fn malformed_lines_skipped() {
        let dir = tempfile::tempdir().unwrap();
        append_marker(dir.path(), &manual_at(1, None)).unwrap();
        let path = dir.path().join("markers.jsonl");
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("not json\n");
        std::fs::write(&path, text).unwrap();
        assert_eq!(read_markers(dir.path()).unwrap().len(), 1);
    }

    #[test]
    fn offsets_drop_pre_session_markers() {
        let markers = vec![
            manual_at(-10, None),
            manual_at(0, None),
            manual_at(90, None),
        ];
        let offsets = to_offsets(&markers, at(0));
        assert_eq!(offsets.len(), 2);
        assert_eq!(offsets[0].at_ms, 0);
        assert_eq!(offsets[1].at_ms, 90_000);
    }

    #[test]
    fn hms_format() {
        assert_eq!(hms(0), "00:00:00");
        assert_eq!(hms(75_000), "00:01:15");
        assert_eq!(hms(3_725_000), "01:02:05");
    }

    #[test]
    fn timestamp_list_merges_and_sorts() {
        let markers = to_offsets(&[manual_at(120, Some("爆笑"))], at(0));
        let highlights = vec![hl(30_000, 50_000)];
        let text = timestamp_list(&markers, &highlights);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "00:00:30 弹幕密度峰值");
        assert_eq!(lines[1], "00:02:00 爆笑");
    }

    #[test]
    fn uncovered_manual_marker_becomes_highlight() {
        let mut hs = vec![hl(0, 20_000)];
        let markers = to_offsets(&[manual_at(100, Some("这段要切"))], at(0));
        merge_marker_highlights(&mut hs, &markers, 15_000, 15_000, 3_600_000);
        assert_eq!(hs.len(), 2);
        let added = &hs[1];
        assert_eq!(added.start_ms, 85_000);
        assert_eq!(added.end_ms, 115_000);
        assert_eq!(added.reason, "手动打点");
        assert_eq!(added.title.as_deref(), Some("这段要切"));
    }

    #[test]
    fn covered_manual_marker_boosts_score() {
        let mut hs = vec![hl(0, 20_000)];
        let markers = to_offsets(&[manual_at(10, None)], at(0));
        merge_marker_highlights(&mut hs, &markers, 15_000, 15_000, 3_600_000);
        assert_eq!(hs.len(), 1);
        assert!((hs[0].score - 0.7).abs() < 1e-9);
        assert!(hs[0].reason.contains("手动打点"));
    }

    #[test]
    fn auto_markers_do_not_duplicate_highlights() {
        let mut hs = vec![];
        let auto = Marker {
            created_at: at(10),
            kind: MarkerKind::Auto,
            source: "realtime".into(),
            note: None,
        };
        merge_marker_highlights(
            &mut hs,
            &to_offsets(&[auto], at(0)),
            15_000,
            15_000,
            100_000,
        );
        assert!(hs.is_empty());
    }

    #[test]
    fn marker_start_clamps_to_zero() {
        let mut hs = vec![];
        let markers = to_offsets(&[manual_at(5, None)], at(0));
        merge_marker_highlights(&mut hs, &markers, 15_000, 15_000, 100_000);
        assert_eq!(hs[0].start_ms, 0);
    }

    #[test]
    fn marker_end_clamps_to_recording_and_late_marker_is_dropped() {
        let mut hs = vec![];
        let markers = to_offsets(&[manual_at(95, None), manual_at(120, None)], at(0));
        merge_marker_highlights(&mut hs, &markers, 15_000, 15_000, 100_000);
        assert_eq!(hs.len(), 1);
        assert_eq!(hs[0].end_ms, 100_000);
    }
}
