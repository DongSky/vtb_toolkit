//! 剪辑打通 commands: export the session's highlights + 打点 to editor
//! projects (FCPXML with markers, 剪映 draft) and bundle a ready-to-edit
//! 素材包 (clips + subtitles + danmaku ASS + timestamp list) in one dir.

use super::markers::{self as marker_cmd};
use super::review::discover_source_video;
use std::path::{Path, PathBuf};
use vtb_common::Highlight;
use vtb_pipeline::markers as mk;

fn read_highlights(dir: &Path) -> Vec<Highlight> {
    std::fs::read_to_string(dir.join("highlights.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// total_ms from the analysis `signals.json` (written by the pipeline).
fn total_ms(dir: &Path) -> Option<u64> {
    let v: serde_json::Value = std::fs::read_to_string(dir.join("signals.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())?;
    v["total_ms"].as_u64()
}

/// Marker points as `(offset_ms, label)` for an analysis dir, via the
/// session directory + manifest start.
fn marker_points(dir: &Path) -> Vec<(u64, String)> {
    let session = match marker_cmd::session_dir_of_pub(dir) {
        Some(s) => s,
        None => return vec![],
    };
    let start = match marker_cmd::session_start_of_pub(&session) {
        Some(s) => s,
        None => return vec![],
    };
    let ms = mk::read_markers(&session).unwrap_or_default();
    mk::to_offsets(&ms, start)
        .into_iter()
        .map(|m| {
            let label = m.marker.note.clone().unwrap_or_else(|| match m.marker.kind {
                mk::MarkerKind::Auto => "自动打点".into(),
                mk::MarkerKind::Manual => "打点".into(),
            });
            (m.at_ms, format!("打点: {label}"))
        })
        .collect()
}

/// Export FCPXML (markers on the full recording) → `highlights.fcpxml`.
/// Imports into DaVinci Resolve / Final Cut / (via conversion) Premiere.
#[tauri::command]
pub async fn fcpxml_export(dir: String) -> Result<String, String> {
    let dir = PathBuf::from(dir);
    let highlights = read_highlights(&dir);
    let markers = marker_points(&dir);
    if highlights.is_empty() && markers.is_empty() {
        return Err("没有可导出的高能片段或打点".into());
    }
    let source =
        discover_source_video(&dir).ok_or("找不到源录播文件")?;
    let total = total_ms(&dir)
        .or_else(|| highlights.iter().map(|h| h.end_ms).max())
        .unwrap_or(0)
        .max(1000);
    let title = dir
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "session".into());

    let xml = vtb_highlight::export::fcpxml_with_markers(
        &title,
        &source,
        total,
        vtb_highlight::export::DEFAULT_FPS,
        &highlights,
        &markers,
    );
    let out = dir.join("highlights.fcpxml");
    std::fs::write(&out, xml).map_err(|e| e.to_string())?;
    Ok(out.to_string_lossy().into_owned())
}

/// Export a 剪映专业版 draft folder under `<dir>/jianying/<name>/`.
/// EXPERIMENTAL: the draft format is unofficial; copy the folder into
/// 剪映's draft root to open it. Returns the draft folder path.
#[tauri::command]
pub async fn jianying_export(dir: String) -> Result<String, String> {
    let dir = PathBuf::from(dir);
    let highlights = read_highlights(&dir);
    if highlights.is_empty() {
        return Err("没有可导出的高能片段".into());
    }
    let source = discover_source_video(&dir).ok_or("找不到源录播文件")?;
    let total = total_ms(&dir)
        .or_else(|| highlights.iter().map(|h| h.end_ms).max())
        .unwrap_or(0)
        .max(1000);
    let name = dir
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "vtb-draft".into());

    let out_root = dir.join("jianying");
    let draft = tokio::task::spawn_blocking(move || {
        vtb_highlight::jianying::write_draft(&out_root, &name, &source, total, &highlights)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    Ok(draft.to_string_lossy().into_owned())
}

#[derive(serde::Serialize)]
pub struct BundleResult {
    pub dir: String,
    pub files: Vec<String>,
}

/// Bundle everything a clipper needs into `<dir>/素材包/`: exported clips,
/// bilingual subtitles, danmaku ASS/XML, timestamp list, cover frames —
/// copied flat so "打开文件夹" gives a single ready-to-edit package.
#[tauri::command]
pub async fn asset_bundle_export(dir: String) -> Result<BundleResult, String> {
    let dir = PathBuf::from(dir);
    if !dir.is_dir() {
        return Err("目录不存在".into());
    }
    let bundle = dir.join("素材包");
    std::fs::create_dir_all(&bundle).map_err(|e| e.to_string())?;
    let mut files = Vec::new();

    let copy_into = |src: &Path, files: &mut Vec<String>| {
        if !src.exists() {
            return;
        }
        if let Some(name) = src.file_name() {
            let dst = bundle.join(name);
            match std::fs::copy(src, &dst) {
                Ok(_) => files.push(dst.to_string_lossy().into_owned()),
                Err(e) => tracing::warn!("bundle copy {src:?} failed: {e}"),
            }
        }
    };

    // Subtitles + danmaku + timestamps (regenerate the list fresh).
    for name in [
        "subtitles.srt",
        "subtitles.bilingual.srt",
        "subtitles.bilingual.ass",
        "danmaku.xml",
    ] {
        copy_into(&dir.join(name), &mut files);
    }
    // Timestamp list: prefer a freshly exported one.
    let ts = dir.join("timestamps.txt");
    if !ts.exists() {
        let _ = super::markers::timestamps_export(dir.to_string_lossy().into_owned()).await;
    }
    copy_into(&ts, &mut files);

    // Clips → 素材包/clips/.
    let clips_src = dir.join("clips");
    if clips_src.is_dir() {
        let clips_dst = bundle.join("clips");
        std::fs::create_dir_all(&clips_dst).map_err(|e| e.to_string())?;
        if let Ok(rd) = std::fs::read_dir(&clips_src) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_file() {
                    if let Some(name) = p.file_name() {
                        let dst = clips_dst.join(name);
                        if std::fs::copy(&p, &dst).is_ok() {
                            files.push(dst.to_string_lossy().into_owned());
                        }
                    }
                }
            }
        }
    }
    // Cover candidate frames → 素材包/cover/.
    let cover_src = dir.join("cover");
    if cover_src.is_dir() {
        let cover_dst = bundle.join("cover");
        std::fs::create_dir_all(&cover_dst).map_err(|e| e.to_string())?;
        if let Ok(rd) = std::fs::read_dir(&cover_src) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_file() {
                    if let Some(name) = p.file_name() {
                        let dst = cover_dst.join(name);
                        if std::fs::copy(&p, &dst).is_ok() {
                            files.push(dst.to_string_lossy().into_owned());
                        }
                    }
                }
            }
        }
    }

    if files.is_empty() {
        return Err("没有可打包的素材（先运行离线处理生成切片/字幕）".into());
    }
    Ok(BundleResult {
        dir: bundle.to_string_lossy().into_owned(),
        files,
    })
}

/// Generate a 720p low-bitrate proxy next to the recording so multi-hour
/// 原画 files scrub smoothly in editors. Returns the proxy path.
#[tauri::command]
pub async fn proxy_generate(input: String) -> Result<String, String> {
    let input = PathBuf::from(input);
    if !input.is_file() {
        return Err("源文件不存在".into());
    }
    let out = input.with_extension("proxy.mp4");
    let status = tokio::process::Command::new("ffmpeg")
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(&input)
        .args([
            "-vf", "scale=-2:720",
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "26",
            "-c:a", "aac", "-b:a", "128k",
            "-movflags", "+faststart",
        ])
        .arg(&out)
        .status()
        .await
        .map_err(|e| format!("ffmpeg 启动失败: {e}"))?;
    if !status.success() {
        return Err("代理生成失败（查看日志）".into());
    }
    Ok(out.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vtb_common::HighlightSignals;

    fn write_analysis(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        let hs = vec![Highlight {
            start_ms: 10_000,
            end_ms: 20_000,
            score: 0.9,
            reason: "弹幕密度峰值".into(),
            signals: HighlightSignals::default(),
            title: Some("名场面".into()),
        }];
        std::fs::write(
            dir.join("highlights.json"),
            serde_json::to_string(&hs).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("signals.json"),
            serde_json::json!({"window_ms":10000,"total_ms":600000}).to_string(),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn fcpxml_export_writes_file() {
        let root = tempfile::tempdir().unwrap();
        // Recorder layout: session/ contains rec.flv + out/.
        std::fs::write(root.path().join("rec.flv"), b"x").unwrap();
        let out = root.path().join("out");
        write_analysis(&out);

        let path = fcpxml_export(out.to_string_lossy().into_owned())
            .await
            .unwrap();
        assert!(path.ends_with("highlights.fcpxml"));
        let xml = std::fs::read_to_string(&path).unwrap();
        assert!(xml.contains("<fcpxml"));
        assert!(xml.contains("名场面"));
        assert!(xml.contains("rec.flv"));
    }

    #[tokio::test]
    async fn jianying_export_writes_draft() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("rec.flv"), b"x").unwrap();
        let out = root.path().join("out");
        write_analysis(&out);

        let draft = jianying_export(out.to_string_lossy().into_owned())
            .await
            .unwrap();
        let p = PathBuf::from(&draft);
        assert!(p.join("draft_content.json").exists());
        assert!(p.join("draft_meta_info.json").exists());
    }

    #[tokio::test]
    async fn bundle_collects_present_artifacts() {
        let root = tempfile::tempdir().unwrap();
        let out = root.path().join("out");
        write_analysis(&out);
        std::fs::write(out.join("subtitles.srt"), b"1\n").unwrap();
        std::fs::create_dir_all(out.join("clips")).unwrap();
        std::fs::write(out.join("clips/a.mp4"), b"v").unwrap();

        let r = asset_bundle_export(out.to_string_lossy().into_owned())
            .await
            .unwrap();
        assert!(r.dir.ends_with("素材包"));
        assert!(r.files.iter().any(|f| f.ends_with("subtitles.srt")));
        assert!(r.files.iter().any(|f| f.ends_with("a.mp4")));
    }

    #[tokio::test]
    async fn export_requires_highlights() {
        let root = tempfile::tempdir().unwrap();
        let out = root.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        assert!(jianying_export(out.to_string_lossy().into_owned())
            .await
            .is_err());
    }
}
