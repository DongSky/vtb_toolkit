//! Recording session metadata: tracks a single live session's files and
//! produces an export manifest when the stream ends.

use crate::error::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Metadata describing a completed recording session, written next to the
/// video files as JSON when the stream ends.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionMetadata {
    pub room_id: u64,
    pub title: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub segments: Vec<SegmentInfo>,
    /// Post-recording health verdict (codec / picture / audio checks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<crate::check::RecordingHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SegmentInfo {
    pub path: PathBuf,
    pub size_bytes: u64,
}

impl SessionMetadata {
    pub fn total_bytes(&self) -> u64 {
        self.segments.iter().map(|s| s.size_bytes).sum()
    }

    pub fn duration_secs(&self) -> Option<i64> {
        self.ended_at.map(|e| (e - self.started_at).num_seconds())
    }
}

/// Tracks an in-progress recording and finalizes metadata on stop.
pub struct RecordingSession {
    room_id: u64,
    title: Option<String>,
    started_at: DateTime<Utc>,
    output_dir: PathBuf,
}

impl RecordingSession {
    pub fn start(room_id: u64, title: Option<String>, output_dir: impl Into<PathBuf>) -> Self {
        Self {
            room_id,
            title,
            started_at: Utc::now(),
            output_dir: output_dir.into(),
        }
    }

    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
    }

    /// Collect segment files matching `stem`/`ext`, compute sizes, and build
    /// finalized metadata with `ended_at = now`.
    pub fn finalize(&self, stem: &str, ext: &str) -> Result<SessionMetadata> {
        let segments = collect_segments(&self.output_dir, stem, &[ext])?;
        Ok(SessionMetadata {
            room_id: self.room_id,
            title: self.title.clone(),
            started_at: self.started_at,
            ended_at: Some(Utc::now()),
            segments,
            health: None,
        })
    }

    /// Like [`finalize`], but collects any common media container matching
    /// the stem — used by supervised sessions whose parts may vary.
    pub fn finalize_media(&self, stem: &str) -> Result<SessionMetadata> {
        let segments =
            collect_segments(&self.output_dir, stem, &["flv", "ts", "mkv", "m4s"])?;
        Ok(SessionMetadata {
            room_id: self.room_id,
            title: self.title.clone(),
            started_at: self.started_at,
            ended_at: Some(Utc::now()),
            segments,
            health: None,
        })
    }

    /// Write the metadata JSON to `<output_dir>/<stem>.meta.json`.
    pub fn export_manifest(&self, meta: &SessionMetadata, stem: &str) -> Result<PathBuf> {
        let path = self.output_dir.join(format!("{stem}.meta.json"));
        let json = serde_json::to_string_pretty(meta)?;
        std::fs::write(&path, json)?;
        Ok(path)
    }
}

fn collect_segments(dir: &Path, stem: &str, exts: &[&str]) -> Result<Vec<SegmentInfo>> {
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    let suffixes: Vec<String> = exts.iter().map(|e| format!(".{e}")).collect();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with(stem) && suffixes.iter().any(|s| name.ends_with(s)) {
                let size = entry.metadata()?.len();
                out.push(SegmentInfo {
                    path: path.clone(),
                    size_bytes: size,
                });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalize_collects_segments_and_exports() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("roomA_000.flv"), b"aaaa").unwrap();
        std::fs::write(dir.path().join("roomA_001.flv"), b"bb").unwrap();
        std::fs::write(dir.path().join("unrelated.flv"), b"zzz").unwrap();

        let session =
            RecordingSession::start(7, Some("测试直播".into()), dir.path());
        let meta = session.finalize("roomA", "flv").unwrap();

        assert_eq!(meta.room_id, 7);
        assert_eq!(meta.segments.len(), 2);
        assert_eq!(meta.total_bytes(), 6);
        assert!(meta.ended_at.is_some());
        assert!(meta.duration_secs().is_some());

        let path = session.export_manifest(&meta, "roomA").unwrap();
        assert!(path.exists());
        let loaded: SessionMetadata =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded, meta);
    }

    #[test]
    fn finalize_on_empty_dir_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let session = RecordingSession::start(1, None, dir.path());
        let meta = session.finalize("nope", "flv").unwrap();
        assert!(meta.segments.is_empty());
        assert_eq!(meta.total_bytes(), 0);
    }
}
