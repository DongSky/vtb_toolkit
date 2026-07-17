//! ffmpeg command construction and process management for pull recording.
//!
//! The recorder pulls a live stream URL (FLV/HLS) and writes it to disk,
//! optionally segmenting by wall-clock time. Segmentation uses ffmpeg's
//! `-f segment` muxer with `-c copy` (stream copy, no re-encode) so it is
//! fast and lossless; because it copies, each segment starts on the next
//! keyframe boundary (documented behavior, acceptable for VOD clips).

use crate::error::{RecorderError, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// How to split a long recording into files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentPolicy {
    /// One continuous file.
    Single,
    /// Split every N seconds (uses the segment muxer, keyframe-aligned).
    ByDuration { seconds: u32 },
    /// Split when a file reaches ~N bytes (uses `-fs` via segment size).
    BySize { bytes: u64 },
}

impl Default for SegmentPolicy {
    fn default() -> Self {
        SegmentPolicy::Single
    }
}

/// Options controlling a recording invocation.
#[derive(Debug, Clone)]
pub struct RecordOptions {
    /// Source stream URL (flv/m3u8/...).
    pub input_url: String,
    /// Output directory.
    pub output_dir: PathBuf,
    /// Base file name without extension, e.g. "room123_20260717".
    pub file_stem: String,
    /// Container extension, e.g. "flv", "mp4", "ts".
    pub extension: String,
    /// Segmentation policy.
    pub segment: SegmentPolicy,
    /// Extra HTTP headers (e.g. Referer, Cookie) for the input.
    pub headers: Vec<(String, String)>,
    /// User agent for the input request.
    pub user_agent: Option<String>,
    /// Also emit 16 kHz mono s16le PCM on stdout (single-pull recording +
    /// realtime subtitles from the same stream).
    pub tee_audio_pcm: bool,
}

impl RecordOptions {
    pub fn new(
        input_url: impl Into<String>,
        output_dir: impl Into<PathBuf>,
        file_stem: impl Into<String>,
    ) -> Self {
        Self {
            input_url: input_url.into(),
            output_dir: output_dir.into(),
            file_stem: file_stem.into(),
            extension: "flv".into(),
            segment: SegmentPolicy::Single,
            headers: Vec::new(),
            user_agent: None,
            tee_audio_pcm: false,
        }
    }

    /// The output path template passed to ffmpeg. For segmented output this
    /// includes a `%03d` counter.
    pub fn output_template(&self) -> PathBuf {
        let name = match self.segment {
            SegmentPolicy::Single | SegmentPolicy::BySize { .. } => {
                format!("{}.{}", self.file_stem, self.extension)
            }
            SegmentPolicy::ByDuration { .. } => {
                format!("{}_%03d.{}", self.file_stem, self.extension)
            }
        };
        self.output_dir.join(name)
    }
}

/// Map a container extension to the ffmpeg muxer name used by the segment
/// sub-muxer. FLV→flv, TS→mpegts, MP4→mp4, others fall back to the extension.
fn segment_muxer(extension: &str) -> String {
    match extension.to_ascii_lowercase().as_str() {
        "flv" => "flv".into(),
        "ts" | "mts" | "m2ts" => "mpegts".into(),
        "mp4" | "m4v" => "mp4".into(),
        "mkv" => "matroska".into(),
        other => other.into(),
    }
}

/// Builds the ffmpeg argument vector for the given options.
///
/// Kept pure (no process spawning) so it is fully unit-testable.
pub fn build_args(opts: &RecordOptions) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();

    // Global: overwrite, hide banner.
    args.push("-y".into());
    args.push("-hide_banner".into());
    args.push("-loglevel".into());
    args.push("warning".into());

    // Reconnect options for flaky live HTTP streams.
    args.push("-reconnect".into());
    args.push("1".into());
    args.push("-reconnect_streamed".into());
    args.push("1".into());
    args.push("-reconnect_delay_max".into());
    args.push("5".into());

    if let Some(ua) = &opts.user_agent {
        args.push("-user_agent".into());
        args.push(ua.into());
    }
    if !opts.headers.is_empty() {
        let joined = opts
            .headers
            .iter()
            .map(|(k, v)| format!("{k}: {v}\r\n"))
            .collect::<String>();
        args.push("-headers".into());
        args.push(joined.into());
    }

    // Input.
    args.push("-i".into());
    args.push(opts.input_url.clone().into());

    // Stream copy: no re-encode.
    args.push("-c".into());
    args.push("copy".into());

    match opts.segment {
        SegmentPolicy::Single => {}
        SegmentPolicy::ByDuration { seconds } => {
            args.push("-f".into());
            args.push("segment".into());
            args.push("-segment_time".into());
            args.push(seconds.to_string().into());
            // Reset timestamps so each segment plays from 0.
            args.push("-reset_timestamps".into());
            args.push("1".into());
            // Force the segment sub-muxer to match the chosen container so
            // FLV/TS aren't fed MP4-only muxer defaults (which fail the
            // header write). Without this ffmpeg guesses from the %03d
            // template extension, but being explicit is safer.
            args.push("-segment_format".into());
            args.push(segment_muxer(&opts.extension).into());
        }
        SegmentPolicy::BySize { bytes } => {
            args.push("-fs".into());
            args.push(bytes.to_string().into());
        }
    }

    args.push(opts.output_template().into());

    // Secondary output: decoded audio for live ASR, same pull.
    if opts.tee_audio_pcm {
        for a in ["-vn", "-ac", "1", "-ar", "16000", "-f", "s16le", "pipe:1"] {
            args.push(a.into());
        }
    }
    args
}

/// A recorder that shells out to ffmpeg.
#[derive(Debug, Clone)]
pub struct FfmpegRecorder {
    /// Path to the ffmpeg binary (defaults to "ffmpeg").
    pub binary: PathBuf,
}

impl Default for FfmpegRecorder {
    fn default() -> Self {
        Self {
            binary: PathBuf::from("ffmpeg"),
        }
    }
}

impl FfmpegRecorder {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
        }
    }

    /// Verify the ffmpeg binary is invocable.
    pub async fn probe(&self) -> Result<String> {
        let out = tokio::process::Command::new(&self.binary)
            .arg("-version")
            .output()
            .await
            .map_err(|_| RecorderError::FfmpegMissing)?;
        if !out.status.success() {
            return Err(RecorderError::FfmpegMissing);
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .to_string())
    }

    /// Spawn a recording process. Returns the child so the caller can await
    /// or kill it (e.g. when the stream ends).
    pub fn spawn(&self, opts: &RecordOptions) -> Result<tokio::process::Child> {
        std::fs::create_dir_all(&opts.output_dir)?;
        let args = build_args(opts);
        tracing::info!("spawning ffmpeg: {:?} {:?}", self.binary, args);
        let mut cmd = tokio::process::Command::new(&self.binary);
        cmd.args(&args)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        if opts.tee_audio_pcm {
            cmd.stdout(std::process::Stdio::piped());
        }
        let child = cmd.spawn().map_err(|_| RecorderError::FfmpegMissing)?;
        Ok(child)
    }
}

/// List the segment files produced for a given options set (by prefix).
pub fn list_outputs(opts: &RecordOptions) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    let prefix = format!("{}", opts.file_stem);
    for entry in std::fs::read_dir(&opts.output_dir)? {
        let entry = entry?;
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with(&prefix)
                && name.ends_with(&format!(".{}", opts.extension))
            {
                result.push(path);
            }
        }
    }
    result.sort();
    Ok(result)
}

/// Whether ffmpeg appears available at the given path.
pub fn binary_exists(binary: &Path) -> bool {
    if binary.components().count() > 1 {
        binary.exists()
    } else {
        // bare name: search PATH.
        std::env::var_os("PATH")
            .map(|paths| {
                std::env::split_paths(&paths).any(|dir| dir.join(binary).exists())
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> RecordOptions {
        RecordOptions::new("http://x/live.flv", "/tmp/rec", "room1")
    }

    #[test]
    fn single_output_template() {
        let o = opts();
        assert_eq!(o.output_template(), PathBuf::from("/tmp/rec/room1.flv"));
    }

    #[test]
    fn segmented_output_template_has_counter() {
        let mut o = opts();
        o.segment = SegmentPolicy::ByDuration { seconds: 600 };
        assert_eq!(
            o.output_template(),
            PathBuf::from("/tmp/rec/room1_%03d.flv")
        );
    }

    #[test]
    fn args_include_stream_copy_and_input() {
        let args = build_args(&opts());
        let s: Vec<String> = args.iter().map(|a| a.to_string_lossy().into()).collect();
        assert!(s.contains(&"-i".to_string()));
        assert!(s.contains(&"http://x/live.flv".to_string()));
        let ci = s.iter().position(|x| x == "-c").unwrap();
        assert_eq!(s[ci + 1], "copy");
    }

    #[test]
    fn args_include_reconnect() {
        let s: Vec<String> = build_args(&opts())
            .iter()
            .map(|a| a.to_string_lossy().into())
            .collect();
        assert!(s.contains(&"-reconnect".to_string()));
    }

    #[test]
    fn duration_segmenting_adds_segment_muxer() {
        let mut o = opts();
        o.extension = "flv".into();
        o.segment = SegmentPolicy::ByDuration { seconds: 300 };
        let s: Vec<String> = build_args(&o)
            .iter()
            .map(|a| a.to_string_lossy().into())
            .collect();
        let fi = s.iter().position(|x| x == "-f").unwrap();
        assert_eq!(s[fi + 1], "segment");
        let ti = s.iter().position(|x| x == "-segment_time").unwrap();
        assert_eq!(s[ti + 1], "300");
        // Must pin the sub-muxer to the container (regression: MP4-only
        // movflags fed to an FLV segment broke the header write).
        let mi = s.iter().position(|x| x == "-segment_format").unwrap();
        assert_eq!(s[mi + 1], "flv");
        assert!(!s.iter().any(|x| x.contains("movflags")));
    }

    #[test]
    fn segment_muxer_maps_containers() {
        assert_eq!(segment_muxer("flv"), "flv");
        assert_eq!(segment_muxer("ts"), "mpegts");
        assert_eq!(segment_muxer("MP4"), "mp4");
        assert_eq!(segment_muxer("mkv"), "matroska");
    }

    #[test]
    fn size_segmenting_adds_fs_limit() {
        let mut o = opts();
        o.segment = SegmentPolicy::BySize { bytes: 1_000_000 };
        let s: Vec<String> = build_args(&o)
            .iter()
            .map(|a| a.to_string_lossy().into())
            .collect();
        let fi = s.iter().position(|x| x == "-fs").unwrap();
        assert_eq!(s[fi + 1], "1000000");
    }

    #[test]
    fn headers_are_joined_with_crlf() {
        let mut o = opts();
        o.headers = vec![
            ("Referer".into(), "https://live.bilibili.com/".into()),
            ("Cookie".into(), "SESSDATA=x".into()),
        ];
        let s: Vec<String> = build_args(&o)
            .iter()
            .map(|a| a.to_string_lossy().into())
            .collect();
        let hi = s.iter().position(|x| x == "-headers").unwrap();
        assert!(s[hi + 1].contains("Referer: https://live.bilibili.com/\r\n"));
        assert!(s[hi + 1].contains("Cookie: SESSDATA=x\r\n"));
    }

    #[test]
    fn user_agent_arg() {
        let mut o = opts();
        o.user_agent = Some("UA/1.0".into());
        let s: Vec<String> = build_args(&o)
            .iter()
            .map(|a| a.to_string_lossy().into())
            .collect();
        let ui = s.iter().position(|x| x == "-user_agent").unwrap();
        assert_eq!(s[ui + 1], "UA/1.0");
    }

    #[test]
    fn tee_audio_adds_second_pcm_output() {
        let mut o = opts();
        o.tee_audio_pcm = true;
        let s: Vec<String> = build_args(&o)
            .iter()
            .map(|a| a.to_string_lossy().into())
            .collect();
        // File output comes first, then the PCM pipe output.
        let file_pos = s.iter().position(|x| x.ends_with("room1.flv")).unwrap();
        let pipe_pos = s.iter().position(|x| x == "pipe:1").unwrap();
        assert!(file_pos < pipe_pos);
        let vn = s.iter().position(|x| x == "-vn").unwrap();
        assert!(vn > file_pos && vn < pipe_pos);
        assert!(s.contains(&"s16le".to_string()));
        // Without the flag there is no pipe output.
        let s2: Vec<String> = build_args(&opts())
            .iter()
            .map(|a| a.to_string_lossy().into())
            .collect();
        assert!(!s2.contains(&"pipe:1".to_string()));
    }

    #[test]
    fn list_outputs_matches_prefix_and_ext() {
        let dir = tempfile::tempdir().unwrap();
        let mut o = RecordOptions::new("u", dir.path(), "roomX");
        o.segment = SegmentPolicy::ByDuration { seconds: 60 };
        std::fs::write(dir.path().join("roomX_000.flv"), b"a").unwrap();
        std::fs::write(dir.path().join("roomX_001.flv"), b"b").unwrap();
        std::fs::write(dir.path().join("other.flv"), b"c").unwrap();
        std::fs::write(dir.path().join("roomX_000.txt"), b"d").unwrap();
        let outs = list_outputs(&o).unwrap();
        assert_eq!(outs.len(), 2);
        assert!(outs[0].ends_with("roomX_000.flv"));
        assert!(outs[1].ends_with("roomX_001.flv"));
    }
}
