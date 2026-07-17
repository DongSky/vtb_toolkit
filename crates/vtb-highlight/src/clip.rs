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
}

/// Build the ffmpeg args to cut `[start_ms, end_ms]` out of `input`.
///
/// Stream-copy mode places `-ss` BEFORE `-i` (fast keyframe seek) and cuts
/// with `-t`; boundaries snap to keyframes. Re-encode mode is
/// frame-accurate at the cost of speed.
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
        "-ss".into(),
        start.into(),
        "-i".into(),
        opts.input.clone().into(),
        "-t".into(),
        dur.into(),
    ];
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

/// Cut a single highlight; returns the output path.
pub async fn cut_clip(
    ffmpeg: &Path,
    opts: &ClipOptions,
    highlight: &Highlight,
) -> Result<PathBuf> {
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
