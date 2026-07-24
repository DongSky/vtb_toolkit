//! 封面辅助 (cover assist): candidate frame extraction from 高能片段,
//! LLM cover-design ideas, and concept-image generation via a
//! gpt-image-2 (OpenAI Images API compatible) endpoint.
//!
//! The image endpoint is fully user-configurable (base URL / API key /
//! model), and generation accepts arbitrary auxiliary inputs: reference
//! images (candidate frames, character art, logos — sent through the
//! `images/edits` multipart endpoint) and free-form text appended to the
//! prompt.

use crate::error::{HighlightError, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use vtb_common::Highlight;

/// A saved candidate frame.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CoverFrame {
    pub path: PathBuf,
    /// Which highlight it came from (index into the sorted-by-score list).
    pub highlight_index: usize,
    pub at_ms: u64,
}

/// Extract candidate frames from the top-scoring highlights into
/// `out_dir` as JPEGs (1280px wide — covers need more pixels than the
/// 640px judge frames). Returns the saved frames sorted by source score.
pub async fn extract_candidates(
    ffmpeg: &Path,
    input: &Path,
    highlights: &[Highlight],
    top_n: usize,
    per_highlight: usize,
    out_dir: &Path,
) -> Result<Vec<CoverFrame>> {
    if highlights.is_empty() {
        return Err(HighlightError::Config("没有高能片段，无法抽取候选帧".into()));
    }
    std::fs::create_dir_all(out_dir)?;

    // Top-N by score, keeping (index, highlight).
    let mut ranked: Vec<(usize, &Highlight)> = highlights.iter().enumerate().collect();
    ranked.sort_by(|a, b| b.1.score.partial_cmp(&a.1.score).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(top_n.max(1));

    let mut frames = Vec::new();
    for (hi, h) in &ranked {
        let span = h.end_ms.saturating_sub(h.start_ms);
        if span == 0 {
            continue;
        }
        let n = per_highlight.max(1) as u64;
        for i in 0..n {
            let at_ms = h.start_ms + span * (2 * i + 1) / (2 * n);
            let out = out_dir.join(format!("frame_{hi:02}_{at_ms}.jpg"));
            let ts = format!("{:.3}", at_ms as f64 / 1000.0);
            let status = tokio::process::Command::new(ffmpeg)
                .args(["-y", "-hide_banner", "-loglevel", "error", "-ss", &ts, "-i"])
                .arg(input)
                .args(["-frames:v", "1", "-q:v", "2", "-vf", "scale=1280:-1"])
                .arg(&out)
                .status()
                .await
                .map_err(|e| HighlightError::Ffmpeg(format!("spawn: {e}")))?;
            if status.success() && out.exists() {
                frames.push(CoverFrame {
                    path: out,
                    highlight_index: *hi,
                    at_ms,
                });
            } else {
                tracing::warn!("cover frame extraction failed at {ts}s");
            }
        }
    }
    if frames.is_empty() {
        return Err(HighlightError::Ffmpeg("候选帧全部抽取失败".into()));
    }
    Ok(frames)
}

/// One cover design idea from the LLM.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoverIdea {
    /// 封面主标题文案 (大字).
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    /// 构图建议.
    #[serde(default)]
    pub composition: String,
    /// 配色建议.
    #[serde(default)]
    pub palette: String,
    /// 适配平台与比例, e.g. "B站 16:10 (1146x717)".
    #[serde(default)]
    pub platform: String,
    /// Ready-to-use gpt-image-2 prompt for this idea.
    #[serde(default)]
    pub image_prompt: String,
}

/// Context assembled from the session's artifacts for idea generation.
#[derive(Debug, Default, Clone)]
pub struct IdeasContext {
    pub session_title: String,
    /// Highlight titles/reasons, best first.
    pub highlights: Vec<String>,
    /// 场次报告 summary (弹幕数/热词/SC…), free-form.
    pub stats_summary: String,
    pub transcript_excerpt: String,
    /// User-provided auxiliary text (梗、人设、活动信息…).
    pub extra_text: String,
}

/// Build the (system, user) prompt pair for cover idea generation.
pub fn build_ideas_prompt(ctx: &IdeasContext) -> (String, String) {
    let system = "你是资深 VTuber 切片/录播封面设计师，深谙B站与YouTube的封面点击率\
                  规律（大字标题、高对比、情绪张力、留白给脸）。基于直播内容产出可\
                  直接落地的封面创意。只输出 JSON 数组，不要输出其他文字。"
        .to_string();
    let mut user = String::new();
    if !ctx.session_title.is_empty() {
        user.push_str(&format!("直播场次: {}\n", ctx.session_title));
    }
    if !ctx.highlights.is_empty() {
        user.push_str("高能片段（按分数排序）:\n");
        for h in ctx.highlights.iter().take(8) {
            user.push_str(&format!("- {h}\n"));
        }
    }
    if !ctx.stats_summary.is_empty() {
        user.push_str(&format!("场次数据: {}\n", ctx.stats_summary));
    }
    if !ctx.transcript_excerpt.is_empty() {
        user.push_str(&format!("字幕节选:\n{}\n", ctx.transcript_excerpt));
    }
    if !ctx.extra_text.is_empty() {
        user.push_str(&format!("补充信息（务必参考）: {}\n", ctx.extra_text));
    }
    user.push_str(
        "\n给出 3-5 组封面创意，输出 JSON 数组，每组字段：\
         {\"title\": \"封面大字标题（≤12字，有梗有情绪）\", \
         \"subtitle\": \"副标题/角标（可空）\", \
         \"composition\": \"构图建议（主体位置/表情/裁剪）\", \
         \"palette\": \"配色建议\", \
         \"platform\": \"适配平台与比例\", \
         \"image_prompt\": \"用于图像生成模型的完整英文 prompt，描述画面而非文字排版\"}",
    );
    (system, user)
}

/// Parse the model's idea list (tolerates code fences and prose around
/// the JSON array).
pub fn parse_ideas(text: &str) -> Result<Vec<CoverIdea>> {
    let trimmed = text.trim();
    let start = trimmed
        .find('[')
        .ok_or_else(|| HighlightError::Config("回复中没有 JSON 数组".into()))?;
    let end = trimmed
        .rfind(']')
        .ok_or_else(|| HighlightError::Config("回复中没有 JSON 数组".into()))?;
    if end < start {
        return Err(HighlightError::Config("JSON 数组括号不匹配".into()));
    }
    let ideas: Vec<CoverIdea> = serde_json::from_str(&trimmed[start..=end])?;
    if ideas.is_empty() {
        return Err(HighlightError::Config("创意列表为空".into()));
    }
    Ok(ideas)
}

/// Render ideas as the `cover/ideas.md` document.
pub fn ideas_markdown(ideas: &[CoverIdea]) -> String {
    let mut out = String::from("# 封面创意\n");
    for (i, idea) in ideas.iter().enumerate() {
        out.push_str(&format!("\n## 方案 {}: {}\n", i + 1, idea.title));
        if !idea.subtitle.is_empty() {
            out.push_str(&format!("- 副标题: {}\n", idea.subtitle));
        }
        if !idea.composition.is_empty() {
            out.push_str(&format!("- 构图: {}\n", idea.composition));
        }
        if !idea.palette.is_empty() {
            out.push_str(&format!("- 配色: {}\n", idea.palette));
        }
        if !idea.platform.is_empty() {
            out.push_str(&format!("- 平台: {}\n", idea.platform));
        }
        if !idea.image_prompt.is_empty() {
            out.push_str(&format!("- 生成 prompt: `{}`\n", idea.image_prompt));
        }
    }
    out
}

/// gpt-image-2 endpoint configuration. Everything is overridable — 中转/
/// 自建 endpoints are the norm for CN users.
#[derive(Debug, Clone)]
pub struct ImageGenConfig {
    /// e.g. "https://api.openai.com/v1" or a relay base.
    pub base_url: String,
    pub api_key: String,
    /// e.g. "gpt-image-2".
    pub model: String,
    /// e.g. "1536x1024" (closest landscape size to B站 cover 16:10).
    pub size: String,
}

impl ImageGenConfig {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            size: "1536x1024".into(),
        }
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}/{}", self.base_url.trim_end_matches('/'), path)
    }
}

/// Client for the OpenAI Images API (`images/generations` for pure
/// text-to-image, `images/edits` when reference images steer the result).
pub struct ImageGenClient {
    http: reqwest::Client,
    pub config: ImageGenConfig,
}

fn guess_mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()) {
        Some(ref e) if e == "png" => "image/png",
        Some(ref e) if e == "webp" => "image/webp",
        _ => "image/jpeg",
    }
}

/// Extract image bytes from an Images API response item (`b64_json`
/// preferred; `url` fetched as a fallback for relays that return links).
async fn decode_item(http: &reqwest::Client, item: &serde_json::Value) -> Result<Vec<u8>> {
    if let Some(b64) = item["b64_json"].as_str() {
        return base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| HighlightError::Config(format!("图像 base64 解码失败: {e}")));
    }
    if let Some(url) = item["url"].as_str() {
        let bytes = http
            .get(url)
            .send()
            .await?
            .error_for_status()
            .map_err(|e| HighlightError::Config(format!("下载生成图失败: {e}")))?
            .bytes()
            .await?;
        return Ok(bytes.to_vec());
    }
    Err(HighlightError::Config("响应缺少 b64_json/url".into()))
}

impl ImageGenClient {
    pub fn new(config: ImageGenConfig) -> Self {
        Self {
            // Image generation is slow; allow minutes, not the default.
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .connect_timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("reqwest client builds"),
            config,
        }
    }

    /// Generate `n` images. `reference_images` (any local paths) switch
    /// the call to the multipart `images/edits` endpoint so the model
    /// works from the provided frames/art; `prompt` should already
    /// include any auxiliary text.
    pub async fn generate(
        &self,
        prompt: &str,
        reference_images: &[PathBuf],
        n: u32,
    ) -> Result<Vec<Vec<u8>>> {
        let resp = if reference_images.is_empty() {
            self.http
                .post(self.config.endpoint("images/generations"))
                .bearer_auth(&self.config.api_key)
                .json(&json!({
                    "model": self.config.model,
                    "prompt": prompt,
                    "n": n.clamp(1, 4),
                    "size": self.config.size,
                }))
                .send()
                .await?
        } else {
            let mut form = reqwest::multipart::Form::new()
                .text("model", self.config.model.clone())
                .text("prompt", prompt.to_string())
                .text("n", n.clamp(1, 4).to_string())
                .text("size", self.config.size.clone());
            for p in reference_images {
                let bytes = std::fs::read(p)?;
                let name = p
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "ref.jpg".into());
                let part = reqwest::multipart::Part::bytes(bytes)
                    .file_name(name)
                    .mime_str(guess_mime(p))
                    .map_err(|e| HighlightError::Config(e.to_string()))?;
                form = form.part("image[]", part);
            }
            self.http
                .post(self.config.endpoint("images/edits"))
                .bearer_auth(&self.config.api_key)
                .multipart(form)
                .send()
                .await?
        };

        let status = resp.status().as_u16();
        let body = resp.text().await?;
        if status >= 400 {
            return Err(HighlightError::Api { status, body });
        }
        let v: serde_json::Value = serde_json::from_str(&body)?;
        let items = v["data"]
            .as_array()
            .ok_or_else(|| HighlightError::Config("响应缺少 data 数组".into()))?;
        let mut out = Vec::new();
        for item in items {
            out.push(decode_item(&self.http, item).await?);
        }
        if out.is_empty() {
            return Err(HighlightError::Config("没有生成任何图像".into()));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ideas_prompt_carries_context_and_schema() {
        let ctx = IdeasContext {
            session_title: "7·23 歌回".into(),
            highlights: vec!["爆笑瞬间 (0.95)".into()],
            stats_summary: "弹幕 5000 条, 热词: 草/泪目".into(),
            transcript_excerpt: "今天唱新歌".into(),
            extra_text: "主播人设: 猫耳".into(),
        };
        let (system, user) = build_ideas_prompt(&ctx);
        assert!(system.contains("封面设计师"));
        assert!(user.contains("7·23 歌回"));
        assert!(user.contains("爆笑瞬间"));
        assert!(user.contains("猫耳"));
        assert!(user.contains("image_prompt"));
    }

    #[test]
    fn parse_ideas_tolerates_fences_and_prose() {
        let text = "好的，以下是创意：\n```json\n[
            {\"title\":\"猫耳歌回\",\"subtitle\":\"新歌初披露\",\"composition\":\"脸右置\",\"palette\":\"粉紫\",\"platform\":\"B站 16:10\",\"image_prompt\":\"anime cat-ear vtuber singing\"},
            {\"title\":\"泪目现场\"}
        ]\n```";
        let ideas = parse_ideas(text).unwrap();
        assert_eq!(ideas.len(), 2);
        assert_eq!(ideas[0].title, "猫耳歌回");
        // Missing fields default to empty rather than failing the parse.
        assert_eq!(ideas[1].image_prompt, "");
    }

    #[test]
    fn parse_ideas_rejects_garbage() {
        assert!(parse_ideas("no json here").is_err());
        assert!(parse_ideas("[]").is_err());
    }

    #[test]
    fn ideas_markdown_renders_all_sections() {
        let md = ideas_markdown(&[CoverIdea {
            title: "标题".into(),
            subtitle: "副".into(),
            composition: "居中".into(),
            palette: "红黑".into(),
            platform: "B站".into(),
            image_prompt: "p".into(),
        }]);
        assert!(md.contains("## 方案 1: 标题"));
        assert!(md.contains("- 配色: 红黑"));
        assert!(md.contains("`p`"));
    }

    #[test]
    fn endpoint_join_handles_trailing_slash() {
        let cfg = ImageGenConfig::new("https://relay.example/v1/", "k", "gpt-image-2");
        assert_eq!(
            cfg.endpoint("images/generations"),
            "https://relay.example/v1/images/generations"
        );
    }

    #[test]
    fn mime_guess_by_extension() {
        assert_eq!(guess_mime(Path::new("a.png")), "image/png");
        assert_eq!(guess_mime(Path::new("a.JPG")), "image/jpeg");
        assert_eq!(guess_mime(Path::new("noext")), "image/jpeg");
    }

    #[tokio::test]
    async fn candidate_extraction_with_real_ffmpeg() {
        if !crate::clip::ffmpeg_available() {
            eprintln!("ffmpeg not found; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.mp4");
        let gen = tokio::process::Command::new("ffmpeg")
            .args([
                "-y", "-hide_banner", "-loglevel", "error",
                "-f", "lavfi", "-i", "testsrc=duration=10:size=640x360:rate=10",
                "-c:v", "libx264", "-preset", "ultrafast",
            ])
            .arg(&src)
            .status()
            .await
            .unwrap();
        assert!(gen.success());

        let highlights = vec![
            vtb_common::Highlight {
                start_ms: 1_000, end_ms: 4_000, score: 0.9,
                reason: "a".into(), signals: Default::default(), title: None,
            },
            vtb_common::Highlight {
                start_ms: 6_000, end_ms: 9_000, score: 0.5,
                reason: "b".into(), signals: Default::default(), title: None,
            },
        ];
        let out_dir = dir.path().join("cover");
        let frames = extract_candidates(
            Path::new("ffmpeg"), &src, &highlights, 2, 3, &out_dir,
        )
        .await
        .unwrap();
        assert_eq!(frames.len(), 6, "2 highlights × 3 frames");
        for f in &frames {
            assert!(f.path.exists());
            let bytes = std::fs::read(&f.path).unwrap();
            assert_eq!(&bytes[0..2], &[0xFF, 0xD8], "JPEG magic");
        }
        // Highest-score highlight's frames come first.
        assert_eq!(frames[0].highlight_index, 0);
    }
}
