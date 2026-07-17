//! Individual test stages, each printing a pass/fail banner.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use vtb_common::{LiveEvent, LiveStatus};

fn banner(title: &str) {
    println!("\n──────── {title} ────────");
}

fn ok(msg: impl std::fmt::Display) {
    println!("  ✅ {msg}");
}
fn warn(msg: impl std::fmt::Display) {
    println!("  ⚠️  {msg}");
}

// ================= Stage 0: resolve room =================

pub async fn resolve_room(room_id: u64) -> (u64, LiveStatus) {
    banner("Stage 0: 房间解析 (room_init)");
    let api = vtb_danmaku::api::BiliApi::default_client().unwrap();
    let info = api.room_init(room_id).await.expect("room_init failed");
    ok(format!(
        "短号 {room_id} → 真实房号 {}，状态 {:?}",
        info.real_room_id, info.status
    ));
    (info.real_room_id, info.status)
}

// ================= Stage 1: danmaku =================

pub async fn danmaku_stage(
    real_room: u64,
    workdir: &Path,
    duration: Duration,
) -> (Vec<LiveEvent>, PathBuf) {
    banner("Stage 1: 弹幕实时连接 + JSONL 落盘");
    use vtb_danmaku::{DanmakuClient, DanmakuClientConfig};
    use vtb_pipeline::danmaku_log::{DanmakuLogWriter, LogEntry};

    let log_path = workdir.join("danmaku.jsonl");
    let mut writer = DanmakuLogWriter::create(&log_path).unwrap();

    let client = DanmakuClient::new(DanmakuClientConfig::anonymous(real_room)).unwrap();
    let (mut rx, handle) = match client.connect().await {
        Ok(v) => v,
        Err(e) => {
            warn(format!("弹幕连接失败: {e}"));
            return (vec![], log_path);
        }
    };
    ok("已连接弹幕服务器，开始采集…");

    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + duration;
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            msg = rx.recv() => match msg {
                Some(ev) => {
                    let _ = writer.write(&LogEntry { received_at: Utc::now(), event: ev.clone() });
                    events.push(ev);
                }
                None => break,
            }
        }
    }
    let _ = writer.flush();
    handle.abort();

    let tally = crate::tally(&events);
    ok(format!("采集到 {} 个事件: {:?}", events.len(), tally));
    if let Some(LiveEvent::Danmaku(d)) = events.iter().find(|e| matches!(e, LiveEvent::Danmaku(_))) {
        ok(format!("示例弹幕: [{}] {}", d.username, d.text));
    }
    ok(format!("弹幕日志已写入 {}", log_path.display()));
    (events, log_path)
}

// ================= Stage 2: recording =================

pub struct Recording {
    pub file: PathBuf,
}

pub async fn record_stage(
    real_room: u64,
    workdir: &Path,
    secs: u64,
) -> Option<Recording> {
    banner("Stage 2: 取流 (playurl) + ffmpeg 录制");
    use vtb_recorder::stream::{qn, StreamApi};

    let client = reqwest::Client::builder()
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
        )
        .build()
        .unwrap();
    let api = StreamApi::new(client);

    let streams = match api.play_info(real_room, qn::HD).await {
        Ok(s) => s,
        Err(e) => {
            warn(format!("取流失败: {e}"));
            return None;
        }
    };
    let best = vtb_recorder::stream::pick_best(&streams)?;
    ok(format!(
        "拿到 {} 条流，选用 {}/{} qn={}",
        streams.len(),
        best.protocol,
        best.format,
        best.qn
    ));

    // Record `secs` via ffmpeg directly (bounded by -t).
    let out = workdir.join(format!("record.{}", if best.format == "flv" { "flv" } else { "ts" }));
    let status = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args([
            "-user_agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
            "-headers",
            "Referer: https://live.bilibili.com/\r\n",
        ])
        .args(["-i", &best.url])
        .args(["-t", &secs.to_string()])
        .args(["-c", "copy"])
        .arg(&out)
        .status()
        .await
        .ok()?;

    if !status.success() || !out.exists() {
        warn("ffmpeg 录制失败");
        return None;
    }
    let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    ok(format!(
        "录制完成 {} ({:.1} MB)",
        out.display(),
        size as f64 / 1_048_576.0
    ));

    // Verify with ffprobe.
    if let Ok(o) = tokio::process::Command::new("ffprobe")
        .args([
            "-v", "quiet", "-show_entries", "format=duration",
            "-of", "default=nw=1:nk=1",
        ])
        .arg(&out)
        .output()
        .await
    {
        let dur = String::from_utf8_lossy(&o.stdout);
        ok(format!("ffprobe 时长: {}s", dur.trim()));
    }

    Some(Recording { file: out })
}

// ================= model download =================

pub async fn ensure_model() -> Option<PathBuf> {
    let cache = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".cache/vtb-toolkit"))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let _ = std::fs::create_dir_all(&cache);
    let path = cache.join("ggml-tiny.bin");
    if path.exists() {
        return Some(path);
    }
    println!("  下载 whisper tiny 模型…");
    let url =
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin";
    let bytes = reqwest::get(url).await.ok()?.bytes().await.ok()?;
    std::fs::write(&path, &bytes).ok()?;
    Some(path)
}

// ================= Stage 3: ASR =================

pub async fn asr_stage(
    recording: &Recording,
    model: Option<&Path>,
) -> Vec<vtb_common::TranscriptSegment> {
    banner("Stage 3: ASR 转写 (whisper)");
    let Some(model) = model else {
        warn("无模型，跳过 ASR");
        return vec![];
    };
    use vtb_asr::engine::WhisperEngine;
    use vtb_asr::streaming::{transcribe_buffer, StreamingConfig};

    let pcm = match vtb_asr::audio::extract_audio_ffmpeg(Path::new("ffmpeg"), &recording.file).await
    {
        Ok(p) => p,
        Err(e) => {
            warn(format!("抽音频失败: {e}"));
            return vec![];
        }
    };
    ok(format!("抽取音频 {} 采样 ({:.1}s @16k)", pcm.len(), pcm.len() as f64 / 16000.0));

    let engine = match WhisperEngine::new(model, None) {
        Ok(e) => Arc::new(e),
        Err(e) => {
            warn(format!("加载模型失败: {e}"));
            return vec![];
        }
    };
    let segs = transcribe_buffer(engine, &pcm, StreamingConfig::default()).await;
    ok(format!("VAD+ASR 得到 {} 段", segs.len()));
    for s in segs.iter().take(5) {
        println!(
            "     [{:.1}-{:.1}s]{} {}",
            s.start_ms as f64 / 1000.0,
            s.end_ms as f64 / 1000.0,
            s.lang.as_deref().map(|l| format!(" ({l})")).unwrap_or_default(),
            s.text
        );
    }
    segs
}

// ================= Stage 4: highlight =================

pub async fn highlight_stage(
    recording: &Recording,
    events: &[LiveEvent],
    session_start: DateTime<Utc>,
    workdir: &Path,
) {
    banner("Stage 4: Highlight 多信号融合 + 切片");
    use vtb_highlight::fusion::{detect_highlights, FusionConfig, SignalSet};
    use vtb_highlight::signals::{audio_energy_scores, danmaku_density, gift_value, keyword_score};
    use vtb_pipeline::energy::rms_series;

    let pcm = match vtb_asr::audio::extract_audio_ffmpeg(Path::new("ffmpeg"), &recording.file).await
    {
        Ok(p) => p,
        Err(_) => {
            warn("无法抽音频，跳过");
            return;
        }
    };
    let total_ms = (pcm.len() as u64 * 1000) / 16000;
    let window_ms = 3000u64;

    // Build danmaku offsets relative to recording start.
    let offsets: Vec<(u64, &LiveEvent)> = events
        .iter()
        .filter_map(|e| {
            let ts = match e {
                LiveEvent::Danmaku(d) => d.timestamp,
                _ => return None,
            };
            let ms = (ts - session_start).num_milliseconds();
            (ms >= 0).then_some((ms as u64, e))
        })
        .collect();

    let signals = SignalSet {
        density: Some(danmaku_density(&offsets, window_ms, total_ms)),
        keyword: Some(keyword_score(&offsets, window_ms, total_ms)),
        gift: Some(gift_value(&offsets, window_ms, total_ms)),
        audio: Some(audio_energy_scores(&rms_series(&pcm, 500), window_ms, total_ms)),
    };

    // Permissive config so short test recordings yield candidates.
    let cfg = FusionConfig {
        threshold: 0.8,
        min_len_ms: 0,
        pad_ms: 1000,
        ..Default::default()
    };
    let highlights = detect_highlights(&signals, &cfg, total_ms);
    ok(format!("检测到 {} 个高能候选", highlights.len()));
    for h in highlights.iter().take(3) {
        println!(
            "     [{:.1}-{:.1}s] 分数 {:.2} 原因 {}",
            h.start_ms as f64 / 1000.0,
            h.end_ms as f64 / 1000.0,
            h.score,
            h.reason
        );
    }

    if let Some(h) = highlights.first() {
        use vtb_highlight::clip::{cut_clip, ClipOptions};
        let opts = ClipOptions {
            input: recording.file.clone(),
            output_dir: workdir.join("clips"),
            reencode: false,
        };
        match cut_clip(Path::new("ffmpeg"), &opts, h).await {
            Ok(p) => ok(format!(
                "切片导出成功: {} ({:.1} KB)",
                p.display(),
                std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) as f64 / 1024.0
            )),
            Err(e) => warn(format!("切片失败: {e}")),
        }
    } else {
        warn("无候选，跳过切片（信号平坦，属正常）");
    }
}

// ================= Stage 5: translation =================

pub async fn translate_stage() {
    banner("Stage 5: 个性化翻译管线 (mock 后端)");
    use vtb_common::TranscriptSegment;
    use vtb_translate::backend::mock::MockBackend;
    use vtb_translate::glossary::{Glossary, GlossaryEntry, ReferencePair};
    use vtb_translate::{StreamerProfile, TranslateConfig, TranslatePipeline};

    let profile = StreamerProfile {
        name: "星街すいせい".into(),
        description: "偶像系VTuber".into(),
        glossary: Glossary {
            entries: vec![GlossaryEntry {
                source: "すいちゃん".into(),
                target: "彗酱".into(),
                note: Some("昵称".into()),
            }],
        },
        references: vec![ReferencePair {
            source: "頑張るぞい".into(),
            target: "加油鸭".into(),
        }],
    };

    let backend = Arc::new(MockBackend::echo());
    let mut pipeline = TranslatePipeline::new(
        backend.clone(),
        profile,
        TranslateConfig {
            target_lang: "zh".into(),
            ..Default::default()
        },
    );

    let seg = TranscriptSegment {
        start_ms: 0,
        end_ms: 2000,
        text: "すいちゃん、今日も頑張るぞい".into(),
        lang: Some("ja".into()),
        is_final: true,
    };
    let out = pipeline.translate_segment(&seg).await.unwrap();
    ok(format!("翻译输出 (mock): {}", out.translated_text));

    // Verify the system prompt carried the personalization.
    let calls = backend.calls.lock().unwrap();
    let system = &calls[0].0;
    let has_term = system.contains("すいちゃん => 彗酱");
    let has_ref = system.contains("加油鸭");
    if has_term && has_ref {
        ok("术语表 + 参考翻译已正确注入 system prompt");
    } else {
        warn("个性化注入缺失");
    }
    println!("     (无 ANTHROPIC_API_KEY，故用 mock；真实后端逻辑同路径)");
}

// ================= Stage 6: offline pipeline =================

pub async fn offline_stage(
    recording: &Recording,
    log_path: &Path,
    session_start: DateTime<Utc>,
    model: &Path,
    workdir: &Path,
) {
    banner("Stage 6: 离线处理管线端到端");
    use vtb_asr::engine::WhisperEngine;
    use vtb_pipeline::{JobConfig, OfflineJob};

    let engine = match WhisperEngine::new(model, None) {
        Ok(e) => Arc::new(e),
        Err(e) => {
            warn(format!("加载模型失败: {e}"));
            return;
        }
    };

    let mut cfg = JobConfig::new(&recording.file, workdir.join("offline_out"));
    cfg.danmaku_log = Some(log_path.to_path_buf());
    cfg.session_start = Some(session_start);
    cfg.translate = false; // no API key
    cfg.highlights = true;
    cfg.fusion.threshold = 0.8;
    cfg.fusion.min_len_ms = 0;
    cfg.fusion.pad_ms = 1000;

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let job = OfflineJob::new(cfg, engine).with_progress(tx);

    let pump = tokio::spawn(async move {
        while let Some(p) = rx.recv().await {
            println!("     [{:?}] {}", p.stage, p.message);
        }
    });

    match job.run().await {
        Ok(out) => {
            let _ = pump.await;
            ok(format!(
                "完成: {} 段字幕, {} 高能, {} 切片, {} 字幕文件",
                out.transcript.len(),
                out.highlights.len(),
                out.clip_files.len(),
                out.subtitle_files.len()
            ));
            for f in &out.subtitle_files {
                ok(format!("字幕产物: {}", f.display()));
            }
        }
        Err(e) => warn(format!("离线管线失败: {e}")),
    }
}

// ================= summary =================

pub fn summary() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  端到端测试完成                                            ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!("产物目录: /tmp/vtb-e2e/");
}
