//! 封面辅助 commands: candidate frame extraction, LLM cover-design ideas,
//! and gpt-image-2 concept-image generation. All operate on an offline
//! analysis directory (highlights.json / signals.json / transcript.json /
//! stats DB), writing into `<dir>/cover/`.

use super::review::discover_source_video;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use vtb_common::Highlight;
use vtb_highlight::cover::{
    self, CoverIdea, IdeasContext, ImageGenClient, ImageGenConfig,
};

fn read_highlights(dir: &Path) -> Vec<Highlight> {
    std::fs::read_to_string(dir.join("highlights.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Extract candidate cover frames from the top highlights into
/// `<dir>/cover/`. Returns the saved frame paths (score order).
#[tauri::command]
pub async fn cover_extract_frames(
    dir: String,
    top_n: Option<usize>,
    per_highlight: Option<usize>,
) -> Result<Vec<String>, String> {
    let dir = PathBuf::from(dir);
    let highlights = read_highlights(&dir);
    if highlights.is_empty() {
        return Err("没有高能片段，请先运行离线处理".into());
    }
    let source = discover_source_video(&dir).ok_or("找不到源录播文件")?;
    let out_dir = dir.join("cover");
    let frames = cover::extract_candidates(
        Path::new("ffmpeg"),
        &source,
        &highlights,
        top_n.unwrap_or(4),
        per_highlight.unwrap_or(3),
        &out_dir,
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(frames
        .into_iter()
        .map(|f| f.path.to_string_lossy().into_owned())
        .collect())
}

/// Transcript excerpt: first few segments' text, capped for prompt size.
fn transcript_excerpt(dir: &Path) -> String {
    let Some(text) = std::fs::read_to_string(dir.join("transcript.json")).ok() else {
        return String::new();
    };
    let Ok(segs) = serde_json::from_str::<Vec<vtb_common::TranscriptSegment>>(&text) else {
        return String::new();
    };
    let mut out = String::new();
    for s in &segs {
        out.push_str(&s.text);
        out.push('\n');
        if out.len() > 1500 {
            break;
        }
    }
    out
}

/// Build a stats summary line from the session report, when available.
/// The stats DB is keyed by the recording session directory (the analysis
/// dir's parent in the recorder layout; the dir itself is tried as a
/// fallback for flat layouts).
async fn stats_summary(app: &AppHandle, dir: &Path) -> String {
    let mut candidates = vec![];
    if let Some(parent) = dir.parent() {
        candidates.push(parent.to_path_buf());
    }
    candidates.push(dir.to_path_buf());

    for candidate in candidates {
        let Ok(report) =
            super::stats::stats_report(app.clone(), candidate.to_string_lossy().into_owned())
                .await
        else {
            continue;
        };
        if report.danmaku_count == 0 {
            continue;
        }
        let mut s = format!("弹幕 {} 条", report.danmaku_count);
        if report.revenue_total > 0.0 {
            s.push_str(&format!(", 营收 ¥{:.0}", report.revenue_total));
        }
        let top: Vec<String> = report
            .word_freq
            .iter()
            .take(6)
            .map(|(w, _)| w.clone())
            .collect();
        if !top.is_empty() {
            s.push_str(&format!(", 热词: {}", top.join("/")));
        }
        return s;
    }
    String::new()
}

/// Generate cover design ideas with the configured text LLM. Writes
/// `<dir>/cover/ideas.md` and returns the parsed ideas. `extra_text` is
/// injected into the prompt (梗/人设/活动信息).
#[tauri::command]
pub async fn cover_ideas(
    app: AppHandle,
    dir: String,
    extra_text: Option<String>,
    llm_provider: Option<String>,
    llm_api_key: Option<String>,
    llm_model: Option<String>,
    llm_base_url: Option<String>,
) -> Result<Vec<CoverIdea>, String> {
    let dir = PathBuf::from(dir);
    let highlights = read_highlights(&dir);

    let mut ctx = IdeasContext {
        session_title: dir
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        highlights: highlights
            .iter()
            .map(|h| {
                format!(
                    "{} ({:.2})",
                    h.title.clone().unwrap_or_else(|| h.reason.clone()),
                    h.score
                )
            })
            .collect(),
        stats_summary: stats_summary(&app, &dir).await,
        transcript_excerpt: transcript_excerpt(&dir),
        extra_text: extra_text.unwrap_or_default(),
    };
    // Nothing to reason about → refuse rather than hallucinate.
    if ctx.highlights.is_empty()
        && ctx.transcript_excerpt.is_empty()
        && ctx.extra_text.is_empty()
    {
        return Err("缺少直播内容信息（高能/字幕/补充文本均为空）".into());
    }
    ctx.highlights.truncate(8);

    let backend = super::pipeline::build_backend_pub(
        &app,
        llm_provider,
        llm_api_key,
        llm_model,
        llm_base_url,
    )?;
    let (system, user) = cover::build_ideas_prompt(&ctx);
    let raw = backend
        .complete(&system, &user)
        .await
        .map_err(|e| e.to_string())?;
    let ideas = cover::parse_ideas(&raw).map_err(|e| e.to_string())?;

    let cover_dir = dir.join("cover");
    std::fs::create_dir_all(&cover_dir).map_err(|e| e.to_string())?;
    std::fs::write(cover_dir.join("ideas.md"), cover::ideas_markdown(&ideas))
        .map_err(|e| e.to_string())?;
    Ok(ideas)
}

#[derive(serde::Deserialize)]
pub struct CoverGenOptions {
    pub dir: String,
    pub prompt: String,
    /// gpt-image-2 endpoint (base URL / key / model / size). All required
    /// so the user controls exactly which endpoint is billed.
    pub base_url: String,
    pub api_key: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub size: Option<String>,
    /// Auxiliary text appended to the prompt.
    #[serde(default)]
    pub extra_text: Option<String>,
    /// Local reference image paths (candidate frames, 立绘, logos…).
    #[serde(default)]
    pub reference_images: Vec<String>,
    #[serde(default)]
    pub n: Option<u32>,
}

/// Generate cover concept images via a gpt-image-2 endpoint. Saves PNGs
/// to `<dir>/cover/gen_*.png` and returns their paths.
#[tauri::command]
pub async fn cover_generate(options: CoverGenOptions) -> Result<Vec<String>, String> {
    if options.api_key.trim().is_empty() || options.base_url.trim().is_empty() {
        return Err("请填写图像生成 API Base URL 与 Key".into());
    }
    let mut prompt = options.prompt.trim().to_string();
    if let Some(extra) = &options.extra_text {
        if !extra.trim().is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(extra.trim());
        }
    }
    if prompt.is_empty() {
        return Err("请填写图像生成 prompt".into());
    }

    let mut cfg = ImageGenConfig::new(
        options.base_url.trim(),
        options.api_key.trim(),
        options
            .model
            .clone()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| "gpt-image-2".into()),
    );
    if let Some(size) = options.size.filter(|s| !s.trim().is_empty()) {
        cfg.size = size;
    }

    let refs: Vec<PathBuf> = options
        .reference_images
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();

    let client = ImageGenClient::new(cfg);
    let images = client
        .generate(&prompt, &refs, options.n.unwrap_or(1))
        .await
        .map_err(|e| e.to_string())?;

    let cover_dir = PathBuf::from(&options.dir).join("cover");
    std::fs::create_dir_all(&cover_dir).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    // Timestamp-free unique names: index within this batch + existing count.
    let existing = std::fs::read_dir(&cover_dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with("gen_")
                })
                .count()
        })
        .unwrap_or(0);
    for (i, bytes) in images.iter().enumerate() {
        let path = cover_dir.join(format!("gen_{:03}.png", existing + i));
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        out.push(path.to_string_lossy().into_owned());
    }
    Ok(out)
}

/// List existing cover artifacts in `<dir>/cover/` (frames + generated).
#[tauri::command]
pub async fn cover_list(dir: String) -> Result<Vec<String>, String> {
    let cover_dir = PathBuf::from(dir).join("cover");
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&cover_dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension()
                .and_then(|x| x.to_str())
                .map(|x| matches!(x.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png" | "webp"))
                .unwrap_or(false)
            {
                out.push(p.to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn generate_rejects_missing_credentials() {
        let opts = CoverGenOptions {
            dir: "/tmp".into(),
            prompt: "p".into(),
            base_url: "".into(),
            api_key: "".into(),
            model: None,
            size: None,
            extra_text: None,
            reference_images: vec![],
            n: None,
        };
        assert!(cover_generate(opts).await.is_err());
    }

    #[tokio::test]
    async fn cover_list_reads_dir() {
        let dir = tempfile::tempdir().unwrap();
        let cover = dir.path().join("cover");
        std::fs::create_dir_all(&cover).unwrap();
        std::fs::write(cover.join("frame_00_1000.jpg"), b"x").unwrap();
        std::fs::write(cover.join("gen_000.png"), b"y").unwrap();
        std::fs::write(cover.join("ideas.md"), b"z").unwrap(); // not an image
        let list = cover_list(dir.path().to_string_lossy().into_owned())
            .await
            .unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|p| p.ends_with("gen_000.png")));
    }

    #[tokio::test]
    async fn extract_frames_requires_highlights() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("highlights.json"), "[]").unwrap();
        assert!(cover_extract_frames(
            dir.path().to_string_lossy().into_owned(),
            None,
            None
        )
        .await
        .is_err());
    }
}
