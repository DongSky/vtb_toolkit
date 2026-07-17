//! Clip extraction: cut highlight windows out of a recording with ffmpeg.

use crate::error::{HighlightError, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use vtb_common::Highlight;

#[derive(Debug, Clone)]
pub struct ClipOptions {
    /// Source recording file.
    pub input: PathBuf,
    /// Output directory for clips.
    pub output_dir: PathBuf,
    /// Re-encode instead of stream copy (frame-accurate but slow).
    pub reencode: bool,
    /// Burn this subtitle file (ASS/SRT) into the clip. Implies re-encoding
    /// and accurate (output-side) seeking so subtitle timing stays aligned
    /// with the original recording.
    pub burn_subtitles: Option<PathBuf>,
}

/// Escape a path for use inside an ffmpeg filtergraph argument. Args are
/// passed without a shell, so no outer quoting — instead escape the
/// filtergraph metacharacters themselves (`\ : , ; [ ] '`).
fn filter_escape(path: &Path) -> String {
    let mut out = String::new();
    for c in path.to_string_lossy().chars() {
        if matches!(c, '\\' | ':' | ',' | ';' | '[' | ']' | '\'') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Build the ffmpeg args to cut `[start_ms, end_ms]` out of `input`.
///
/// Stream-copy mode places `-ss` BEFORE `-i` (fast keyframe seek) and cuts
/// with `-t`; boundaries snap to keyframes. Re-encode mode is
/// frame-accurate at the cost of speed. Burn mode seeks on the OUTPUT side
/// so the subtitles filter sees original timestamps (otherwise burned subs
/// would be shifted by `start_ms`).
pub fn build_clip_args(
    opts: &ClipOptions,
    start_ms: u64,
    end_ms: u64,
    output: &Path,
) -> Vec<OsString> {
    let start = format!("{:.3}", start_ms as f64 / 1000.0);
    let dur = format!("{:.3}", (end_ms.saturating_sub(start_ms)) as f64 / 1000.0);
    let mut args: Vec<OsString> = vec![
        "-y".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "warning".into(),
    ];

    if let Some(subs) = &opts.burn_subtitles {
        // Accurate mode: decode from 0 so subtitle PTS align, trim on output.
        args.extend::<[OsString; 2]>(["-i".into(), opts.input.clone().into()]);
        args.extend::<[OsString; 4]>([
            "-ss".into(),
            start.into(),
            "-t".into(),
            dur.into(),
        ]);
        args.extend::<[OsString; 2]>([
            "-vf".into(),
            format!("subtitles={}", filter_escape(subs)).into(),
        ]);
        args.extend::<[OsString; 6]>([
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            "fast".into(),
            "-c:a".into(),
            "aac".into(),
        ]);
        args.push(output.into());
        return args;
    }

    args.extend::<[OsString; 4]>([
        "-ss".into(),
        start.into(),
        "-i".into(),
        opts.input.clone().into(),
    ]);
    args.extend::<[OsString; 2]>(["-t".into(), dur.into()]);
    if opts.reencode {
        args.extend::<[OsString; 6]>([
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            "fast".into(),
            "-c:a".into(),
            "aac".into(),
        ]);
    } else {
        args.extend::<[OsString; 4]>([
            "-c".into(),
            "copy".into(),
            "-avoid_negative_ts".into(),
            "make_zero".into(),
        ]);
    }
    args.push(output.into());
    args
}

/// File name for a highlight clip: `clip_<start>s-<end>s.mp4`.
pub fn clip_filename(h: &Highlight) -> String {
    format!("clip_{}s-{}s.mp4", h.start_ms / 1000, h.end_ms / 1000)
}

/// Whether this ffmpeg build has the `subtitles` filter (needs libass —
/// e.g. Homebrew's default build lacks it).
pub async fn subtitles_filter_available(ffmpeg: &Path) -> bool {
    match tokio::process::Command::new(ffmpeg)
        .args(["-hide_banner", "-filters"])
        .output()
        .await
    {
        Ok(out) => String::from_utf8_lossy(&out.stdout)
            .lines()
            .any(|l| l.split_whitespace().nth(1) == Some("subtitles")),
        Err(_) => false,
    }
}

/// Cut a single highlight; returns the output path.
pub async fn cut_clip(
    ffmpeg: &Path,
    opts: &ClipOptions,
    highlight: &Highlight,
) -> Result<PathBuf> {
    if opts.burn_subtitles.is_some() && !subtitles_filter_available(ffmpeg).await {
        return Err(HighlightError::Ffmpeg(
            "当前 ffmpeg 未编译 libass（无 subtitles 滤镜），无法烧录字幕；\
             请安装带 libass 的 ffmpeg 或关闭字幕烧录"
                .into(),
        ));
    }
    std::fs::create_dir_all(&opts.output_dir)?;
    let output = opts.output_dir.join(clip_filename(highlight));
    let args = build_clip_args(opts, highlight.start_ms, highlight.end_ms, &output);
    let status = tokio::process::Command::new(ffmpeg)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .status()
        .await
        .map_err(|e| HighlightError::Ffmpeg(format!("spawn: {e}")))?;
    if !status.success() {
        return Err(HighlightError::Ffmpeg(format!("exit status {status}")));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vtb_common::HighlightSignals;

    fn opts() -> ClipOptions {
        ClipOptions {
            input: PathBuf::from("/rec/full.flv"),
            output_dir: PathBuf::from("/rec/clips"),
            reencode: false,
            burn_subtitles: None,
        }
    }

    fn to_strings(args: &[OsString]) -> Vec<String> {
        args.iter().map(|a| a.to_string_lossy().into()).collect()
    }

    #[test]
    fn copy_mode_args() {
        let s = to_strings(&build_clip_args(
            &opts(),
            65_000,
            125_500,
            Path::new("/rec/clips/out.mp4"),
        ));
        let ss = s.iter().position(|x| x == "-ss").unwrap();
        assert_eq!(s[ss + 1], "65.000");
        let t = s.iter().position(|x| x == "-t").unwrap();
        assert_eq!(s[t + 1], "60.500");
        assert!(s.contains(&"copy".to_string()));
        // -ss must come before -i for fast seek.
        let i = s.iter().position(|x| x == "-i").unwrap();
        assert!(ss < i);
    }

    #[test]
    fn reencode_mode_uses_x264() {
        let mut o = opts();
        o.reencode = true;
        let s = to_strings(&build_clip_args(&o, 0, 1000, Path::new("/o.mp4")));
        assert!(s.contains(&"libx264".to_string()));
        assert!(!s.contains(&"copy".to_string()));
    }

    #[test]
    fn burn_mode_seeks_after_input_and_adds_filter() {
        let mut o = opts();
        o.burn_subtitles = Some(PathBuf::from("/subs/bi lingual.ass"));
        let s = to_strings(&build_clip_args(&o, 65_000, 95_000, Path::new("/o.mp4")));
        // Output-side seek: -i comes BEFORE -ss so subtitle PTS align.
        let i = s.iter().position(|x| x == "-i").unwrap();
        let ss = s.iter().position(|x| x == "-ss").unwrap();
        assert!(i < ss, "burn mode must seek after input: {s:?}");
        // Subtitles filter present and re-encoding forced.
        let vf = s.iter().position(|x| x == "-vf").unwrap();
        assert!(s[vf + 1].starts_with("subtitles="));
        assert!(s.contains(&"libx264".to_string()));
        assert!(!s.contains(&"copy".to_string()));
    }

    #[test]
    fn filter_escape_handles_specials() {
        assert_eq!(
            filter_escape(Path::new("/a/b's:file,x.ass")),
            "/a/b\\'s\\:file\\,x.ass"
        );
    }

    #[test]
    fn filename_from_highlight() {
        let h = Highlight {
            start_ms: 65_000,
            end_ms: 125_000,
            score: 0.9,
            reason: "x".into(),
            signals: HighlightSignals::default(),
            title: None,
        };
        assert_eq!(clip_filename(&h), "clip_65s-125s.mp4");
    }

    #[tokio::test]
    async fn cut_clip_end_to_end_with_real_ffmpeg() {
        // Skip when ffmpeg is unavailable (CI without ffmpeg).
        if !crate::clip::ffmpeg_available() {
            eprintln!("ffmpeg not found; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.mp4");
        // Generate a 3s test clip.
        let gen = tokio::process::Command::new("ffmpeg")
            .args([
                "-y", "-hide_banner", "-loglevel", "error",
                "-f", "lavfi", "-i", "testsrc=duration=3:size=320x240:rate=10",
                "-f", "lavfi", "-i", "sine=frequency=440:duration=3",
                "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac",
            ])
            .arg(&src)
            .status()
            .await
            .unwrap();
        assert!(gen.success());

        let o = ClipOptions {
            input: src,
            output_dir: dir.path().join("clips"),
            reencode: false,
            burn_subtitles: None,
        };
        let h = Highlight {
            start_ms: 500,
            end_ms: 2_000,
            score: 1.0,
            reason: "test".into(),
            signals: HighlightSignals::default(),
            title: None,
        };
        let out = cut_clip(Path::new("ffmpeg"), &o, &h).await.unwrap();
        assert!(out.exists());
        assert!(std::fs::metadata(&out).unwrap().len() > 0);
    }

    #[tokio::test]
    async fn burned_clip_end_to_end_with_real_ffmpeg() {
        if !crate::clip::ffmpeg_available() {
            eprintln!("ffmpeg not found; skipping");
            return;
        }
        if !subtitles_filter_available(Path::new("ffmpeg")).await {
            // Verify the friendly error path instead.
            let o = ClipOptions {
                input: PathBuf::from("/nonexistent.mp4"),
                output_dir: std::env::temp_dir(),
                reencode: false,
                burn_subtitles: Some(PathBuf::from("/s.ass")),
            };
            let h = Highlight {
                start_ms: 0,
                end_ms: 1000,
                score: 1.0,
                reason: "x".into(),
                signals: HighlightSignals::default(),
                title: None,
            };
            let err = cut_clip(Path::new("ffmpeg"), &o, &h).await.unwrap_err();
            assert!(err.to_string().contains("libass"), "err: {err}");
            eprintln!("ffmpeg lacks libass; verified error path instead");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.mp4");
        let gen = tokio::process::Command::new("ffmpeg")
            .args([
                "-y", "-hide_banner", "-loglevel", "error",
                "-f", "lavfi", "-i", "testsrc=duration=3:size=320x240:rate=10",
                "-f", "lavfi", "-i", "sine=frequency=440:duration=3",
                "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac",
            ])
            .arg(&src)
            .status()
            .await
            .unwrap();
        assert!(gen.success());

        // Minimal SRT covering the clip window.
        let subs = dir.path().join("subs.srt");
        std::fs::write(
            &subs,
            "1\n00:00:00,500 --> 00:00:02,000\n烧录测试字幕\n\n",
        )
        .unwrap();

        let o = ClipOptions {
            input: src,
            output_dir: dir.path().join("clips"),
            reencode: false,
            burn_subtitles: Some(subs),
        };
        let h = Highlight {
            start_ms: 500,
            end_ms: 2_000,
            score: 1.0,
            reason: "burn".into(),
            signals: HighlightSignals::default(),
            title: None,
        };
        let out = cut_clip(Path::new("ffmpeg"), &o, &h).await.unwrap();
        assert!(out.exists());
        assert!(std::fs::metadata(&out).unwrap().len() > 0);
    }
}

/// Whether `ffmpeg` is on PATH (used by tests to skip gracefully).
pub fn ffmpeg_available() -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|d| {
                d.join("ffmpeg").exists() || d.join("ffmpeg.exe").exists()
            })
        })
        .unwrap_or(false)
}
