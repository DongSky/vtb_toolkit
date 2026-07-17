//! Per-session recording supervisor: keeps ffmpeg pulling the stream for
//! the whole live session, restarting with a freshly resolved URL when the
//! stream stalls (URL expiry, CDN hiccups) or ffmpeg exits early.
//!
//! Each restart writes a new `-pNN` part file; all parts belong to one
//! session and are collected into a single manifest at the end.

use crate::auto::{ResolvedStream, StreamResolver};
use crate::ffmpeg::{FfmpegRecorder, RecordOptions, SegmentPolicy};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::watch;

#[derive(Debug, Clone)]
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
    loop {
        if *stop.borrow() {
            return SupervisorEnd::Stopped;
        }
        let started = tokio::time::Instant::now();
        // Fresh URL every (re)start — stream URLs expire quickly.
        let stream = match resolver.resolve(config.room_id).await {
            Ok(s) => s,
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
        };

        let stem = format!("{}-p{:02}", config.stem_base, part);
        let opts = build_opts(config, &stream, &stem);
        let child = match recorder.spawn(&opts) {
            Ok(c) => c,
            Err(e) => {
                return SupervisorEnd::GaveUp {
                    failures: rapid_failures + 1,
                    last_error: e.to_string(),
                }
            }
        };
        if let Some(n) = &notes {
            let _ = n.send(SupervisorNote::PartStarted { part }).await;
        }

        let end = run_with_watchdog(child, &opts, config.stall_timeout, &mut stop).await;
        let reason = match &end {
            RunEnd::Stopped => "stop".to_string(),
            RunEnd::Stalled => "stalled".to_string(),
            RunEnd::Exited(msg) => msg.clone(),
        };
        if let Some(n) = &notes {
            let _ = n.send(SupervisorNote::PartEnded { part, reason: reason.clone() }).await;
        }

        match end {
            RunEnd::Stopped => return SupervisorEnd::Stopped,
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
                    rapid_failures = 0;
                }
                part += 1;
            }
        }
    }
}

fn build_opts(config: &SupervisorConfig, stream: &ResolvedStream, stem: &str) -> RecordOptions {
    let mut opts = RecordOptions::new(stream.url.clone(), &config.output_dir, stem);
    opts.extension = config
        .extension_override
        .clone()
        .unwrap_or_else(|| stream.extension.clone());
    opts.segment = config.segment.clone();
    opts.headers = stream.headers.clone();
    opts.user_agent = stream.user_agent.clone();
    opts
}

/// Run one ffmpeg child until stop / stall / exit.
async fn run_with_watchdog(
    mut child: tokio::process::Child,
    opts: &RecordOptions,
    stall_timeout: Duration,
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
                } else if last_growth.elapsed() >= stall_timeout {
                    graceful_stop(&mut child).await;
                    return RunEnd::Stalled;
                }
            }
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
    use std::sync::atomic::{AtomicU32, Ordering};
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
            })
        }
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
