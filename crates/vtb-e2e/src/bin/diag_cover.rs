//! Diagnostic: 封面辅助 end-to-end.
//!
//! 1. Synthesizes (or takes) a video, extracts cover candidate frames
//!    from fake highlights — validates the ffmpeg path.
//! 2. With OPENAI_BASE_URL/OPENAI_API_KEY (+ optional IMAGE_MODEL,
//!    default gpt-image-2) set, runs ONE real image generation, using a
//!    candidate frame as reference (images/edits multipart path).
//!
//! Usage:
//!   cargo run -p vtb-e2e --bin diag_cover [-- /path/to/video.mp4]
//!   OPENAI_BASE_URL=https://yunwu.ai/v1 OPENAI_API_KEY=sk-... \
//!     cargo run -p vtb-e2e --bin diag_cover

use std::path::{Path, PathBuf};
use vtb_highlight::cover::{
    build_ideas_prompt, extract_candidates, IdeasContext, ImageGenClient, ImageGenConfig,
};

async fn synth_video(path: &Path) {
    let st = tokio::process::Command::new("ffmpeg")
        .args([
            "-y", "-hide_banner", "-loglevel", "error",
            "-f", "lavfi", "-i", "testsrc=duration=20:size=1280x720:rate=10",
            "-c:v", "libx264", "-preset", "ultrafast",
        ])
        .arg(path)
        .status()
        .await
        .expect("ffmpeg spawns");
    assert!(st.success(), "test video generation failed");
}

#[tokio::main]
async fn main() {
    let dir = tempfile::tempdir().unwrap();
    let video: PathBuf = match std::env::args().nth(1) {
        Some(p) => PathBuf::from(p),
        None => {
            let p = dir.path().join("src.mp4");
            synth_video(&p).await;
            p
        }
    };
    println!("视频: {}", video.display());

    let highlights = vec![
        vtb_common::Highlight {
            start_ms: 2_000, end_ms: 8_000, score: 0.95,
            reason: "弹幕爆发".into(), signals: Default::default(),
            title: Some("爆笑瞬间".into()),
        },
        vtb_common::Highlight {
            start_ms: 12_000, end_ms: 18_000, score: 0.6,
            reason: "礼物峰值".into(), signals: Default::default(), title: None,
        },
    ];

    // 1. Candidate frames.
    let cover_dir = dir.path().join("cover");
    let frames = extract_candidates(
        Path::new("ffmpeg"), &video, &highlights, 4, 3, &cover_dir,
    )
    .await
    .expect("candidate extraction");
    println!("候选帧 {} 张:", frames.len());
    for f in &frames {
        println!("  {} (hl#{} @{}ms)", f.path.display(), f.highlight_index, f.at_ms);
    }
    assert!(frames.len() >= 6, "acceptance: ≥6 candidate frames");

    // 2. Ideas prompt (offline check — actual LLM call is app-side).
    let (system, user) = build_ideas_prompt(&IdeasContext {
        session_title: "diag".into(),
        highlights: vec!["爆笑瞬间 (0.95)".into()],
        stats_summary: "弹幕 1000 条".into(),
        transcript_excerpt: String::new(),
        extra_text: "猫耳人设".into(),
    });
    assert!(system.contains("封面设计师") && user.contains("猫耳人设"));
    println!("\nideas prompt OK ({} + {} chars)", system.len(), user.len());

    // 3. Real image generation, only when the env provides an endpoint.
    let (base, key) = match (
        std::env::var("OPENAI_BASE_URL"),
        std::env::var("OPENAI_API_KEY"),
    ) {
        (Ok(b), Ok(k)) => (b, k),
        _ => {
            println!("\n(未设置 OPENAI_BASE_URL/OPENAI_API_KEY，跳过真实图像生成)");
            println!("\n✅ diag_cover 离线部分全部通过");
            return;
        }
    };
    let model =
        std::env::var("IMAGE_MODEL").unwrap_or_else(|_| "gpt-image-2".into());
    println!("\n真实图像生成: {base} model={model}");
    let client = ImageGenClient::new(ImageGenConfig::new(base, key, model));
    let refs = vec![frames[0].path.clone()];
    match client
        .generate("VTuber 直播切片封面，动漫风，高对比大字排版留白", &refs, 1)
        .await
    {
        Ok(images) => {
            let out = dir.path().join("gen_000.png");
            std::fs::write(&out, &images[0]).unwrap();
            println!("生成成功: {} ({} bytes)", out.display(), images[0].len());
        }
        Err(e) => {
            eprintln!("生成失败: {e}");
            std::process::exit(1);
        }
    }
    println!("\n✅ diag_cover 全部通过");
}
