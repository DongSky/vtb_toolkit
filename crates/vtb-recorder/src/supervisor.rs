//! Per-session recording supervisor: keeps ffmpeg pulling the stream for
//! the whole live session, restarting with a freshly resolved URL when the
//! stream stalls (URL expiry, CDN hiccups) or ffmpeg exits early.
//!
//! Each restart writes a new `-pNN` part file; all parts belong to one
//! session and are collected into a single manifest at the end.

use crate::auto::{ResolvedStream, StreamResolver};
use crate::ffmpeg::{FfmpegRecorder, RecordOptions, SegmentPolicy};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

/// Injectable free-space probe (tests); `None` uses [`crate::disk::free_space`].
pub type FreeSpaceFn = Arc<dyn Fn(&Path) -> std::io::Result<u64> + Send + Sync>;

#[derive(Clone)]
pub struct SupervisorConfig {
    pub room_id: u64,
    pub output_dir: PathBuf,
    /// Base stem; parts append `-pNN`.
    pub stem_base: String,
    pub segment: SegmentPolicy,
    /// No output growth for this long → stalled, restart with a new URL.
    pub stall_timeout: Duration,
    /// A (re)start ending sooner than this counts as a rapid failure.
    pub min_healthy: Duration,
    /// Give up after this many consecutive rapid failures.
    pub max_rapid_failures: u32,
    /// Force this container extension instead of the stream's native one.
    pub extension_override: Option<String>,
    /// Stop recording when the output filesystem's free space drops below
    /// this many bytes (safety net for unattended recording).
    pub min_free_bytes: u64,
    /// Free-space probe override for tests; `None` = real `disk::free_space`.
    pub free_space_fn: Option<FreeSpaceFn>,
    /// When set, decoded 16 kHz mono PCM from the SAME pull is forwarded
    /// here (single-stream recording + realtime subtitles).
    pub pcm_tx: Option<tokio::sync::mpsc::Sender<Vec<f32>>>,
}

impl std::fmt::Debug for SupervisorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SupervisorConfig")
            .field("room_id", &self.room_id)
            .field("output_dir", &self.output_dir)
            .field("stem_base", &self.stem_base)
            .field("segment", &self.segment)
            .field("stall_timeout", &self.stall_timeout)
            .field("min_healthy", &self.min_healthy)
            .field("max_rapid_failures", &self.max_rapid_failures)
            .field("extension_override", &self.extension_override)
            .field("min_free_bytes", &self.min_free_bytes)
            .field("free_space_fn", &self.free_space_fn.as_ref().map(|_| "<fn>"))
            .field("pcm_tx", &self.pcm_tx.as_ref().map(|_| "<tx>"))
            .finish()
    }
}

impl SupervisorConfig {
    pub fn new(room_id: u64, output_dir: impl Into<PathBuf>, stem_base: impl Into<String>) -> Self {
        Self {
            room_id,
            output_dir: output_dir.into(),
            stem_base: stem_base.into(),
            segment: SegmentPolicy::Single,
            stall_timeout: Duration::from_secs(15),
            min_healthy: Duration::from_secs(10),
            max_rapid_failures: 3,
            extension_override: None,
            min_free_bytes: crate::disk::DEFAULT_MIN_FREE,
            free_space_fn: None,
            pcm_tx: None,
        }
    }
}

/// Why the supervisor loop ended.
#[derive(Debug, Clone, PartialEq)]
pub enum SupervisorEnd {
    /// Stop requested (stream went offline / user stop).
    Stopped,
    /// Too many consecutive rapid failures (stream gone or persistent error).
    GaveUp { failures: u32, last_error: String },
    /// Output filesystem dropped below the free-space floor; recording was
    /// stopped to avoid filling the disk. Not retried.
    DiskFull { free_bytes: u64 },
}

/// Progress notes emitted while supervising.
#[derive(Debug, Clone, PartialEq)]
pub enum SupervisorNote {
    PartStarted { part: u32 },
    PartEnded { part: u32, reason: String },
}

/// How one ffmpeg run ended.
#[derive(Debug)]
enum RunEnd {
    Stopped,
    Stalled,
    Exited(String),
    DiskFull(u64),
}

pub async fn supervise_recording<R: StreamResolver>(
    resolver: &R,
    recorder: &FfmpegRecorder,
    config: &SupervisorConfig,
    mut stop: watch::Receiver<bool>,
    notes: Option<tokio::sync::mpsc::Sender<SupervisorNote>>,
) -> SupervisorEnd {
    let mut part: u32 = 0;
    let mut rapid_failures: u32 = 0;
    // Backup CDN lines (备线) from the last resolve; consumed before a full
    // re-resolve so a single dead host doesn't force a fresh playurl call.
    let mut backup_lines: std::collections::VecDeque<String> = Default::default();
    let mut last_resolved: Option<ResolvedStream> = None;
    loop {
        if *stop.borrow() {
            return SupervisorEnd::Stopped;
        }
        let started = tokio::time::Instant::now();
        // Prefer an unused backup line; otherwise resolve fresh (stream URLs
        // expire quickly, so re-resolving also refreshes the primary).
        let stream = if let Some(url) = backup_lines.pop_front() {
            tracing::info!("切换备线 (剩余 {} 条)", backup_lines.len());
            ResolvedStream {
                url,
                ..last_stream_shell(&last_resolved)
            }
        } else {
            match resolver.resolve(config.room_id).await {
                Ok(s) => {
                    backup_lines = s.backup_urls.iter().cloned().collect();
                    last_resolved = Some(s.clone());
                    s
                }
                Err(e) => {
                    rapid_failures += 1;
                    if rapid_failures >= config.max_rapid_failures {
                        return SupervisorEnd::GaveUp {
                            failures: rapid_failures,
                            last_error: e.to_string(),
                        };
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(3)) => continue,
                        _ = stop.changed() => continue,
                    }
                }
            }
        };

        let stem = format!("{}-p{:02}", config.stem_base, part);
        let opts = build_opts(config, &stream, &stem);
        let mut child = match recorder.spawn(&opts) {
            Ok(c) => c,
            Err(e) => {
                return SupervisorEnd::GaveUp {
                    failures: rapid_failures + 1,
                    last_error: e.to_string(),
                }
            }
        };
        // Forward tee'd PCM to the subtitle pipeline (per part; the reader
        // ends when this ffmpeg exits).
        if let (Some(tx), Some(stdout)) = (config.pcm_tx.clone(), child.stdout.take()) {
            tokio::spawn(pump_pcm(stdout, tx));
        }
        if let Some(n) = &notes {
            let _ = n.send(SupervisorNote::PartStarted { part }).await;
        }

        let end = run_with_watchdog(child, &opts, config, &mut stop).await;
        let reason = match &end {
            RunEnd::Stopped => "stop".to_string(),
            RunEnd::Stalled => "stalled".to_string(),
            RunEnd::Exited(msg) => msg.clone(),
            RunEnd::DiskFull(free) => format!("disk full: {free} bytes free"),
        };
        if let Some(n) = &notes {
            let _ = n.send(SupervisorNote::PartEnded { part, reason: reason.clone() }).await;
        }

        match end {
            RunEnd::Stopped => return SupervisorEnd::Stopped,
            // A full disk won't clear itself: don't retry, surface it.
            RunEnd::DiskFull(free_bytes) => return SupervisorEnd::DiskFull { free_bytes },
            RunEnd::Stalled | RunEnd::Exited(_) => {
                if started.elapsed() < config.min_healthy {
                    rapid_failures += 1;
                    if rapid_failures >= config.max_rapid_failures {
                        return SupervisorEnd::GaveUp {
                            failures: rapid_failures,
                            last_error: reason,
                        };
                    }
                } else {
                    // A healthy run means the primary line was fine; its
                    // backups have since expired, so force a fresh resolve
                    // for the next part rather than trying stale hosts.
                    rapid_failures = 0;
                    backup_lines.clear();
                }
                part += 1;
            }
        }
    }
}

/// Headers/UA/extension of the last resolve, with an empty url and no
/// backups — the caller fills in the backup url. Falls back to empty
/// metadata if nothing was resolved yet (shouldn't happen: backups only
/// exist after a successful resolve).
fn last_stream_shell(last: &Option<ResolvedStream>) -> ResolvedStream {
    match last {
        Some(s) => ResolvedStream {
            url: String::new(),
            headers: s.headers.clone(),
            user_agent: s.user_agent.clone(),
            extension: s.extension.clone(),
            backup_urls: vec![],
        },
        None => ResolvedStream {
            url: String::new(),
            headers: vec![],
            user_agent: None,
            extension: "flv".into(),
            backup_urls: vec![],
        },
    }
}

fn build_opts(config: &SupervisorConfig, stream: &ResolvedStream, stem: &str) -> RecordOptions {
    let mut opts = RecordOptions::new(stream.url.clone(), &config.output_dir, stem);
    opts.tee_audio_pcm = config.pcm_tx.is_some();
    opts.extension = config
        .extension_override
        .clone()
        .unwrap_or_else(|| stream.extension.clone());
    opts.segment = config.segment.clone();
    opts.headers = stream.headers.clone();
    opts.user_agent = stream.user_agent.clone();
    opts
}

/// Run one ffmpeg child until stop / stall / low disk / exit.
async fn run_with_watchdog(
    mut child: tokio::process::Child,
    opts: &RecordOptions,
    config: &SupervisorConfig,
    stop: &mut watch::Receiver<bool>,
) -> RunEnd {
    let mut last_size = total_output_bytes(opts);
    let mut last_growth = tokio::time::Instant::now();
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = stop.changed() => {
                if *stop.borrow() {
                    graceful_stop(&mut child).await;
                    return RunEnd::Stopped;
                }
            }
            status = child.wait() => {
                let msg = status
                    .map(|s| format!("ffmpeg exited: {s}"))
                    .unwrap_or_else(|e| format!("wait failed: {e}"));
                return RunEnd::Exited(msg);
            }
            _ = tick.tick() => {
                let size = total_output_bytes(opts);
                if size > last_size {
                    last_size = size;
                    last_growth = tokio::time::Instant::now();
                } else if last_growth.elapsed() >= config.stall_timeout {
                    graceful_stop(&mut child).await;
                    return RunEnd::Stalled;
                }
                // Disk safety net: stop before the volume fills up. A
                // failed probe is ignored — don't kill a healthy recording
                // because statvfs hiccuped.
                let free = match &config.free_space_fn {
                    Some(f) => f(&opts.output_dir),
                    None => crate::disk::free_space(&opts.output_dir),
                };
                if let Ok(free) = free {
                    if free < config.min_free_bytes {
                        graceful_stop(&mut child).await;
                        return RunEnd::DiskFull(free);
                    }
                }
            }
        }
    }
}

/// Read s16le from ffmpeg stdout, convert to f32 chunks (~0.5 s), forward.
async fn pump_pcm(
    mut stdout: tokio::process::ChildStdout,
    tx: tokio::sync::mpsc::Sender<Vec<f32>>,
) {
    use tokio::io::AsyncReadExt;
    let mut buf = vec![0u8; 16000]; // 0.5 s of 16 kHz mono s16le
    loop {
        match stdout.read_exact(&mut buf).await {
            Ok(_) => {
                let pcm: Vec<f32> = buf
                    .chunks_exact(2)
                    .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32767.0)
                    .collect();
                if tx.send(pcm).await.is_err() {
                    break;
                }
            }
            Err(_) => break, // part ended
        }
    }
}

fn total_output_bytes(opts: &RecordOptions) -> u64 {
    crate::ffmpeg::list_outputs(opts)
        .unwrap_or_default()
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum()
}

/// SIGINT first so ffmpeg finalizes the container; SIGKILL as backstop.
pub async fn graceful_stop(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        unsafe {
            libc::kill(pid as i32, libc::SIGINT);
        }
        if tokio::time::timeout(Duration::from_secs(8), child.wait())
            .await
            .is_ok()
        {
            return;
        }
    }
    let _ = child.start_kill();
    let _ = child.wait().await;
}

/// Collect part files for a whole session (all `-pNN` stems).
pub fn session_outputs(output_dir: &Path, stem_base: &str, extension: &str) -> Vec<PathBuf> {
    let mut opts = RecordOptions::new("", output_dir, stem_base);
    opts.extension = extension.into();
    crate::ffmpeg::list_outputs(&opts).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auto::ResolvedStream;
    use crate::error::Result;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
    use std::sync::Arc;

    struct CountingResolver {
        calls: Arc<AtomicU32>,
        fail: bool,
    }

    #[async_trait]
    impl StreamResolver for CountingResolver {
        async fn resolve(&self, _room_id: u64) -> Result<ResolvedStream> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(crate::error::RecorderError::NoStreamUrl);
            }
            Ok(ResolvedStream {
                url: "ignored".into(),
                headers: vec![],
                user_agent: None,
                extension: "flv".into(),
                backup_urls: vec![],
            })
        }
    }

    /// Resolver that hands out backup CDN lines (备线) with each resolve.
    struct BackupResolver {
        calls: Arc<AtomicU32>,
    }

    #[async_trait]
    impl StreamResolver for BackupResolver {
        async fn resolve(&self, _room_id: u64) -> Result<ResolvedStream> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ResolvedStream {
                url: "primary".into(),
                headers: vec![("Referer".into(), "https://x/".into())],
                user_agent: None,
                extension: "flv".into(),
                backup_urls: vec!["backup-1".into(), "backup-2".into()],
            })
        }
    }

    /// Fake "ffmpeg" that exits immediately (simulating a dead CDN line).
    fn failing_recorder(dir: &Path) -> FfmpegRecorder {
        let script = dir.join("fake-fail.sh");
        std::fs::write(&script, "#!/bin/sh\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        FfmpegRecorder::new(script)
    }

    #[tokio::test]
    async fn backup_lines_consumed_before_re_resolving() {
        let dir = tempfile::tempdir().unwrap();
        let calls = Arc::new(AtomicU32::new(0));
        let resolver = BackupResolver { calls: calls.clone() };
        let recorder = failing_recorder(dir.path());

        let mut cfg = SupervisorConfig::new(1, dir.path(), "sess");
        cfg.min_healthy = Duration::from_secs(60); // every run is "rapid"
        cfg.max_rapid_failures = 4;

        let (_stop_tx, stop_rx) = watch::channel(false);
        let end = supervise_recording(&resolver, &recorder, &cfg, stop_rx, None).await;

        assert!(matches!(end, SupervisorEnd::GaveUp { failures: 4, .. }), "{end:?}");
        // Runs: resolve#1(primary) → backup-1 → backup-2 → resolve#2 = 4
        // failures with only TWO resolver calls (backups consumed first).
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    /// Fake "ffmpeg" that appends to the output file for `write_secs`
    /// seconds, then sleeps forever (simulating a stalled stream). Traps
    /// INT/TERM so graceful stop is prompt. Unique path per call.
    fn stalling_recorder(dir: &Path, write_secs: u32) -> FfmpegRecorder {
        let script = dir.join(format!("fake-stall-{}.sh", std::process::id()));
        // The last CLI arg is the output path (matches build_args layout).
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ntrap 'exit 0' INT TERM\nout=$(eval echo \\${{$#}})\nfor i in $(seq 1 {write_secs}); do\n  echo data >> \"$out\"\n  sleep 1\ndone\nsleep 300\n"
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        FfmpegRecorder::new(script)
    }

    #[tokio::test]
    async fn stall_triggers_restart_with_new_resolve() {
        let dir = tempfile::tempdir().unwrap();
        let calls = Arc::new(AtomicU32::new(0));
        let resolver = CountingResolver { calls: calls.clone(), fail: false };
        let recorder = stalling_recorder(dir.path(), 2);

        let mut cfg = SupervisorConfig::new(1, dir.path(), "sess");
        cfg.stall_timeout = Duration::from_secs(3);
        cfg.min_healthy = Duration::from_secs(1); // 2s of writing counts healthy
        cfg.max_rapid_failures = 2;

        let (stop_tx, stop_rx) = watch::channel(false);
        let (ntx, mut nrx) = tokio::sync::mpsc::channel(16);

        let sup = tokio::spawn({
            let cfg = cfg.clone();
            async move {
                supervise_recording(&resolver, &recorder, &cfg, stop_rx, Some(ntx)).await
            }
        });

        // Wait until the second part starts (proves stall → restart).
        let mut saw_part1 = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_secs(60), nrx.recv()).await {
                Ok(Some(SupervisorNote::PartStarted { part: 1 })) => {
                    saw_part1 = true;
                    break;
                }
                Ok(Some(_)) => {}
                other => panic!("notes channel ended early: {other:?}"),
            }
        }
        assert!(saw_part1, "expected part 1 after stall restart");
        assert!(calls.load(Ordering::SeqCst) >= 2, "URL must be re-resolved");

        // Wait for part 1's file to materialize before stopping (the fake
        // writer appends once per second; under parallel-test load the stop
        // could otherwise land before its first write).
        let p1 = dir.path().join("sess-p01.flv");
        let wait_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < wait_deadline {
            if std::fs::metadata(&p1).map(|m| m.len() > 0).unwrap_or(false) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        stop_tx.send(true).unwrap();
        let end = sup.await.unwrap();
        assert_eq!(end, SupervisorEnd::Stopped);

        // Both part files exist.
        let parts = session_outputs(dir.path(), "sess", "flv");
        assert!(parts.len() >= 2, "parts: {parts:?}");
    }

    #[tokio::test]
    async fn resolver_failures_give_up_after_budget() {
        let dir = tempfile::tempdir().unwrap();
        let resolver = CountingResolver { calls: Arc::new(AtomicU32::new(0)), fail: true };
        let recorder = FfmpegRecorder::new("/nonexistent-ffmpeg");
        let mut cfg = SupervisorConfig::new(1, dir.path(), "s");
        cfg.max_rapid_failures = 2;
        let (_stop_tx, stop_rx) = watch::channel(false);
        let end = supervise_recording(&resolver, &recorder, &cfg, stop_rx, None).await;
        match end {
            SupervisorEnd::GaveUp { failures, .. } => assert_eq!(failures, 2),
            other => panic!("expected GaveUp, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn low_disk_space_stops_recording_without_retry() {
        let dir = tempfile::tempdir().unwrap();
        let calls = Arc::new(AtomicU32::new(0));
        let resolver = CountingResolver { calls: calls.clone(), fail: false };
        let recorder = stalling_recorder(dir.path(), 300); // healthy writer

        let mut cfg = SupervisorConfig::new(1, dir.path(), "disk");
        cfg.min_free_bytes = 1024;
        // Injected probe: plenty of space for the first two checks, then
        // below the floor.
        let probes = Arc::new(AtomicU64::new(0));
        cfg.free_space_fn = Some(Arc::new({
            let probes = probes.clone();
            move |_: &Path| {
                if probes.fetch_add(1, Ordering::SeqCst) < 2 {
                    Ok(u64::MAX)
                } else {
                    Ok(512)
                }
            }
        }));

        let (_stop_tx, stop_rx) = watch::channel(false);
        // Generous timeout: parallel-suite load can slow the 2s ticks down.
        let end = tokio::time::timeout(
            Duration::from_secs(60),
            supervise_recording(&resolver, &recorder, &cfg, stop_rx, None),
        )
        .await
        .expect("supervisor must end on its own when the disk fills");

        assert_eq!(end, SupervisorEnd::DiskFull { free_bytes: 512 });
        assert_eq!(calls.load(Ordering::SeqCst), 1, "disk full must not retry");
        assert!(probes.load(Ordering::SeqCst) >= 3, "probe ran each tick");

        // The fake ffmpeg appends once per second while alive; a flat file
        // size over a few seconds proves it was stopped.
        let out = dir.path().join("disk-p00.flv");
        let size_after_end = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        tokio::time::sleep(Duration::from_secs(4)).await;
        let size_later = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        assert_eq!(size_after_end, size_later, "ffmpeg must be stopped");
    }

    /// Fake ffmpeg that writes the file AND streams bytes to stdout,
    /// mimicking tee_audio_pcm mode.
    fn stdout_recorder(dir: &Path) -> FfmpegRecorder {
        let script = dir.join(format!("fake-stdout-{}.sh", std::process::id()));
        std::fs::write(
            &script,
            "#!/bin/sh\ntrap 'exit 0' INT TERM\nout=$(eval echo \\${$#})\nwhile true; do\n  echo data >> \"$out\"\n  head -c 16000 /dev/zero\n  sleep 1\ndone\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        FfmpegRecorder::new(script)
    }

    #[tokio::test]
    async fn pcm_tee_forwards_audio_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let resolver = CountingResolver { calls: Arc::new(AtomicU32::new(0)), fail: false };
        let recorder = stdout_recorder(dir.path());
        let (pcm_tx, mut pcm_rx) = tokio::sync::mpsc::channel(16);
        let mut cfg = SupervisorConfig::new(1, dir.path(), "pcm");
        cfg.pcm_tx = Some(pcm_tx);
        let (stop_tx, stop_rx) = watch::channel(false);
        let sup = tokio::spawn({
            let cfg = cfg.clone();
            async move { supervise_recording(&resolver, &recorder, &cfg, stop_rx, None).await }
        });

        // Expect at least one 0.5s PCM chunk (8000 samples of silence).
        let chunk = tokio::time::timeout(Duration::from_secs(20), pcm_rx.recv())
            .await
            .expect("pcm chunk timely")
            .expect("channel open");
        assert_eq!(chunk.len(), 8000);
        assert!(chunk.iter().all(|s| *s == 0.0), "silence expected from /dev/zero");

        stop_tx.send(true).unwrap();
        let end = tokio::time::timeout(Duration::from_secs(15), sup).await.unwrap().unwrap();
        assert_eq!(end, SupervisorEnd::Stopped);
    }

    #[tokio::test]
    async fn stop_during_recording_is_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let resolver = CountingResolver { calls: Arc::new(AtomicU32::new(0)), fail: false };
        let recorder = stalling_recorder(dir.path(), 300); // writes "forever"
        let cfg = SupervisorConfig::new(1, dir.path(), "s");
        let (stop_tx, stop_rx) = watch::channel(false);
        let sup = tokio::spawn({
            let cfg = cfg.clone();
            async move { supervise_recording(&resolver, &recorder, &cfg, stop_rx, None).await }
        });
        tokio::time::sleep(Duration::from_millis(1500)).await;
        stop_tx.send(true).unwrap();
        let end = tokio::time::timeout(Duration::from_secs(15), sup)
            .await
            .expect("stop must be prompt")
            .unwrap();
        assert_eq!(end, SupervisorEnd::Stopped);
    }
}
