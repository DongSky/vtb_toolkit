//! Session review (高能进度条) commands: load analysis artifacts from an
//! offline-processing output directory, export arbitrary clips.

use serde::Serialize;
use std::path::{Path, PathBuf};
use vtb_common::{Highlight, HighlightSignals};

#[derive(Serialize)]
pub struct ReviewData {
    /// signals.json content, when present.
    pub signals: Option<serde_json::Value>,
    /// highlights.json content, when present.
    pub highlights: Vec<Highlight>,
    /// Existing clip files.
    pub clips: Vec<String>,
    /// Recording file next to / referenced by the analysis (best effort).
    pub video: Option<String>,
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
}

/// Load review artifacts from an offline output directory.
#[tauri::command]
pub async fn review_load(dir: String) -> Result<ReviewData, String> {
    let dir = PathBuf::from(dir);
    if !dir.is_dir() {
        return Err("目录不存在".into());
    }
    let signals = read_json(&dir.join("signals.json"));
    let highlights: Vec<Highlight> = read_json(&dir.join("highlights.json"))
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    let mut clips = vec![];
    if let Ok(rd) = std::fs::read_dir(dir.join("clips")) {
        for e in rd.flatten() {
            clips.push(e.path().to_string_lossy().into_owned());
        }
        clips.sort();
    }

    // Find a likely source video: parent dir media files (recorder layout
    // puts out/ next to rec.flv / parts).
    let video = dir
        .parent()
        .and_then(|p| std::fs::read_dir(p).ok())
        .and_then(|rd| {
            let mut media: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| matches!(e, "flv" | "mp4" | "ts" | "mkv"))
                        .unwrap_or(false)
                })
                .collect();
            media.sort();
            // Prefer flv/ts originals over remuxed mp4 duplicates.
            media
                .iter()
                .find(|p| p.extension().map(|e| e != "mp4").unwrap_or(false))
                .cloned()
                .or_else(|| media.into_iter().next())
        })
        .map(|p| p.to_string_lossy().into_owned());

    Ok(ReviewData {
        signals,
        highlights,
        clips,
        video,
    })
}

/// Cut an arbitrary [start_ms, end_ms] range out of a recording.
#[tauri::command]
pub async fn clip_export(
    input: String,
    output_dir: String,
    start_ms: u64,
    end_ms: u64,
) -> Result<String, String> {
    if end_ms <= start_ms {
        return Err("结束时间必须大于开始时间".into());
    }
    let opts = vtb_highlight::clip::ClipOptions {
        input: PathBuf::from(&input),
        output_dir: PathBuf::from(&output_dir),
        reencode: false,
            burn_subtitles: None,
    };
    let h = Highlight {
        start_ms,
        end_ms,
        score: 1.0,
        reason: "手动导出".into(),
        signals: HighlightSignals::default(),
        title: None,
    };
    let out = vtb_highlight::clip::cut_clip(Path::new("ffmpeg"), &opts, &h)
        .await
        .map_err(|e| e.to_string())?;
    Ok(out.to_string_lossy().into_owned())
}
