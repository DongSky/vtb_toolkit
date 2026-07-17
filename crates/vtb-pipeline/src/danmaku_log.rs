//! Danmaku session logs: JSONL of timestamped [`LiveEvent`]s recorded
//! alongside a video. Used offline to rebuild highlight signals.

use crate::error::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use vtb_common::LiveEvent;

/// One log line: wall-clock receive time + the event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogEntry {
    pub received_at: DateTime<Utc>,
    pub event: LiveEvent,
}

/// Append entries to a JSONL log file.
pub struct DanmakuLogWriter<W: Write> {
    inner: W,
}

impl DanmakuLogWriter<std::io::BufWriter<std::fs::File>> {
    pub fn create(path: &Path) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            inner: std::io::BufWriter::new(file),
        })
    }
}

impl<W: Write> DanmakuLogWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner }
    }

    pub fn write(&mut self, entry: &LogEntry) -> Result<()> {
        serde_json::to_writer(&mut self.inner, entry)?;
        self.inner.write_all(b"\n")?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.inner.flush()?;
        Ok(())
    }
}

/// Read a JSONL log; malformed lines are skipped with a warning.
pub fn read_log(path: &Path) -> Result<Vec<LogEntry>> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<LogEntry>(&line) {
            Ok(e) => out.push(e),
            Err(e) => tracing::warn!("skipping malformed log line {}: {e}", i + 1),
        }
    }
    Ok(out)
}

/// Convert log entries to `(offset_ms, &LiveEvent)` pairs relative to a
/// session start time, dropping events before the start.
pub fn to_offsets<'a>(
    entries: &'a [LogEntry],
    session_start: DateTime<Utc>,
) -> Vec<(u64, &'a LiveEvent)> {
    entries
        .iter()
        .filter_map(|e| {
            let delta = e.received_at - session_start;
            let ms = delta.num_milliseconds();
            (ms >= 0).then_some((ms as u64, &e.event))
        })
        .collect()
}

/// What a recording session directory contains alongside the video.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredSession {
    pub danmaku_log: PathBuf,
    /// From the session manifest's `started_at`, when present.
    pub session_start: Option<DateTime<Utc>>,
}

/// Auto-discover the danmaku log + session start for a recording file:
/// looks for `danmaku.jsonl` and a `*.meta.json` manifest in the same
/// directory (the layout AutoRecorder produces).
pub fn discover_session(input: &Path) -> Option<DiscoveredSession> {
    let dir = input.parent()?;
    let log = dir.join("danmaku.jsonl");
    if !log.exists() {
        return None;
    }
    let mut session_start = None;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".meta.json") {
                if let Ok(text) = std::fs::read_to_string(entry.path()) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        session_start = v
                            .get("started_at")
                            .and_then(|s| s.as_str())
                            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                            .map(|d| d.with_timezone(&Utc));
                    }
                }
                break;
            }
        }
    }
    Some(DiscoveredSession {
        danmaku_log: log,
        session_start,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use vtb_common::DanmakuMsg;

    fn danmaku_at(secs: i64) -> LogEntry {
        let ts = Utc.timestamp_opt(1_700_000_000 + secs, 0).single().unwrap();
        LogEntry {
            received_at: ts,
            event: LiveEvent::Danmaku(DanmakuMsg {
                room_id: 1,
                uid: 1,
                username: "u".into(),
                text: format!("msg at {secs}"),
                timestamp: ts,
                medal: None,
                guard_level: 0,
                is_admin: false,
                emoticon: None,
            }),
        }
    }

    #[test]
    fn roundtrip_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("danmaku.jsonl");
        {
            let mut w = DanmakuLogWriter::create(&path).unwrap();
            w.write(&danmaku_at(0)).unwrap();
            w.write(&danmaku_at(5)).unwrap();
            w.flush().unwrap();
        }
        let entries = read_log(&path).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], danmaku_at(0));
    }

    #[test]
    fn malformed_lines_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.jsonl");
        let good = serde_json::to_string(&danmaku_at(1)).unwrap();
        std::fs::write(&path, format!("not json\n{good}\n\n{{\"broken\":\n")).unwrap();
        let entries = read_log(&path).unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn offsets_relative_to_session_start() {
        let start = Utc.timestamp_opt(1_700_000_000, 0).single().unwrap();
        let entries = vec![danmaku_at(-3), danmaku_at(0), danmaku_at(10)];
        let offsets = to_offsets(&entries, start);
        // Pre-start event dropped.
        assert_eq!(offsets.len(), 2);
        assert_eq!(offsets[0].0, 0);
        assert_eq!(offsets[1].0, 10_000);
    }

    #[test]
    fn discovers_log_and_session_start() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("rec-p00.flv");
        std::fs::write(&video, b"x").unwrap();

        // No danmaku.jsonl yet → None.
        assert!(discover_session(&video).is_none());

        std::fs::write(dir.path().join("danmaku.jsonl"), b"").unwrap();
        std::fs::write(
            dir.path().join("rec.meta.json"),
            r#"{"room_id":1,"started_at":"2026-07-17T12:00:00Z","segments":[]}"#,
        )
        .unwrap();

        let found = discover_session(&video).unwrap();
        assert_eq!(found.danmaku_log, dir.path().join("danmaku.jsonl"));
        let start = found.session_start.unwrap();
        assert_eq!(start.timestamp(), 1784289600); // 2026-07-17T12:00:00Z
    }

    #[test]
    fn discovery_without_manifest_still_returns_log() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("v.flv");
        std::fs::write(&video, b"x").unwrap();
        std::fs::write(dir.path().join("danmaku.jsonl"), b"").unwrap();
        let found = discover_session(&video).unwrap();
        assert!(found.session_start.is_none());
    }
}
