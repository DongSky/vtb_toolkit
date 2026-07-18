//! Auto-recording orchestrator: reacts to monitor events by starting a
//! supervised recording session (with stall watchdog + URL re-resolution)
//! and finalizing metadata, health check, and MP4 remux when it ends.

use crate::check::{remux_mp4, verify_recording};
use crate::error::Result;
use crate::ffmpeg::{binary_exists, FfmpegRecorder, SegmentPolicy};
use crate::monitor::MonitorEvent;
use crate::session::{RecordingSession, SessionMetadata};
use crate::supervisor::{supervise_recording, SupervisorConfig, SupervisorEnd};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};

/// Abstracts stream URL resolution so the orchestrator is testable.
#[async_trait]
pub trait StreamResolver: Send + Sync {
    /// Return `(url, headers, extension)` for the room's best stream.
    async fn resolve(&self, room_id: u64) -> Result<ResolvedStream>;
}

#[derive(Debug, Clone)]
pub struct ResolvedStream {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub user_agent: Option<String>,
    pub extension: String,
    /// Backup line URLs (备线) for the same stream on other CDN hosts; the
    /// supervisor tries these before re-resolving on a stall/failure.
    pub backup_urls: Vec<String>,
}

/// Events the orchestrator emits for UI display.
#[derive(Debug, Clone, PartialEq)]
pub enum RecorderEvent {
    RecordingStarted { room_id: u64, output_dir: PathBuf },
    RecordingStopped { room_id: u64, metadata: SessionMetadata },
    RecordingError { room_id: u64, message: String },
}

#[derive(Debug, Clone)]
pub struct AutoRecorderConfig {
    pub room_id: u64,
    pub output_root: PathBuf,
    pub segment: SegmentPolicy,
    pub extension_override: Option<String>,
    /// Watchdog: restart when output stops growing for this long.
    pub stall_timeout: Duration,
    /// A (re)start shorter than this counts as a rapid failure.
    pub min_healthy: Duration,
    /// Consecutive rapid failures before giving up.
    pub max_rapid_failures: u32,
    /// Remux each part to MP4 after the session ends.
    pub remux_mp4: bool,
    /// Run the black-screen/silence health probe after the session ends.
    pub health_check: bool,
    /// Stop recording when the output disk's free space drops below this
    /// many bytes (safety net for unattended recording).
    pub min_free_bytes: u64,
    /// Forward decoded PCM from the recording pull (realtime subtitles
    /// without a second stream connection).
    pub pcm_tx: Option<tokio::sync::mpsc::Sender<Vec<f32>>>,
}

impl AutoRecorderConfig {
    pub fn new(room_id: u64, output_root: impl Into<PathBuf>) -> Self {
        Self {
            room_id,
            output_root: output_root.into(),
            segment: SegmentPolicy::Single,
            extension_override: None,
            stall_timeout: Duration::from_secs(15),
            min_healthy: Duration::from_secs(10),
            max_rapid_failures: 3,
            remux_mp4: true,
            health_check: true,
            min_free_bytes: crate::disk::DEFAULT_MIN_FREE,
            pcm_tx: None,
        }
    }
}

/// Drives supervised recordings based on monitor events. One per room.
pub struct AutoRecorder<R: StreamResolver + 'static> {
    config: AutoRecorderConfig,
    resolver: Arc<R>,
    recorder: FfmpegRecorder,
}

struct ActiveRecording {
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<SupervisorEnd>,
    session: RecordingSession,
    dir: PathBuf,
    stem_base: String,
}

impl<R: StreamResolver + 'static> AutoRecorder<R> {
    pub fn new(config: AutoRecorderConfig, resolver: R, recorder: FfmpegRecorder) -> Self {
        Self {
            config,
            resolver: Arc::new(resolver),
            recorder,
        }
    }

    fn session_dir(&self, stamp: &str) -> PathBuf {
        self.config
            .output_root
            .join(format!("room{}", self.config.room_id))
            .join(stamp)
    }

    /// Consume monitor events until the channel closes; emit recorder
    /// events. Handles start → (supervised run) → stop → export.
    pub async fn run(
        self,
        mut events: mpsc::Receiver<MonitorEvent>,
        tx: mpsc::Sender<RecorderEvent>,
    ) {
        let mut active: Option<ActiveRecording> = None;
        loop {
            // When a recording is active, also watch for its supervisor
            // ending on its own (gave up after repeated failures).
            if let Some(rec) = active.as_mut() {
                tokio::select! {
                    ev = events.recv() => match ev {
                        Some(ev) => self.handle_event(ev, &mut active, &tx).await,
                        None => break,
                    },
                    end = &mut rec.task => {
                        let end = end.unwrap_or(SupervisorEnd::Stopped);
                        let rec = active.take().expect("active present");
                        let room_id = self.config.room_id;
                        match &end {
                            SupervisorEnd::GaveUp { failures, last_error } => {
                                let _ = tx.send(RecorderEvent::RecordingError {
                                    room_id,
                                    message: format!(
                                        "录制中止（连续失败{failures}次）: {last_error}"
                                    ),
                                }).await;
                            }
                            SupervisorEnd::DiskFull { free_bytes } => {
                                let _ = tx.send(RecorderEvent::RecordingError {
                                    room_id,
                                    message: format!(
                                        "录制中止：磁盘空间不足（剩余 {free_bytes} 字节）"
                                    ),
                                }).await;
                            }
                            _ => {}
                        }
                        match self.finalize(rec).await {
                            Ok(metadata) => {
                                let _ = tx.send(RecorderEvent::RecordingStopped {
                                    room_id, metadata,
                                }).await;
                            }
                            Err(e) => {
                                let _ = tx.send(RecorderEvent::RecordingError {
                                    room_id, message: e.to_string(),
                                }).await;
                            }
                        }
                    }
                }
            } else {
                match events.recv().await {
                    Some(ev) => self.handle_event(ev, &mut active, &tx).await,
                    None => break,
                }
            }
        }
        // Channel closed: stop any active recording gracefully.
        if let Some(rec) = active.take() {
            let _ = self.stop_and_finalize(rec).await;
        }
    }

    async fn handle_event(
        &self,
        ev: MonitorEvent,
        active: &mut Option<ActiveRecording>,
        tx: &mpsc::Sender<RecorderEvent>,
    ) {
        match ev {
            MonitorEvent::WentLive { room_id } => {
                if active.is_some() {
                    return;
                }
                match self.start_recording(room_id) {
                    Ok(rec) => {
                        let _ = tx
                            .send(RecorderEvent::RecordingStarted {
                                room_id,
                                output_dir: rec.dir.clone(),
                            })
                            .await;
                        *active = Some(rec);
                    }
                    Err(e) => {
                        let _ = tx
                            .send(RecorderEvent::RecordingError {
                                room_id,
                                message: e.to_string(),
                            })
                            .await;
                    }
                }
            }
            MonitorEvent::WentOffline { room_id } => {
                if let Some(rec) = active.take() {
                    match self.stop_and_finalize(rec).await {
                        Ok(metadata) => {
                            let _ = tx
                                .send(RecorderEvent::RecordingStopped { room_id, metadata })
                                .await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(RecorderEvent::RecordingError {
                                    room_id,
                                    message: e.to_string(),
                                })
                                .await;
                        }
                    }
                }
            }
        }
    }

    fn start_recording(&self, room_id: u64) -> Result<ActiveRecording> {
        let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
        let dir = self.session_dir(&stamp);
        std::fs::create_dir_all(&dir)?;
        let stem_base = format!("room{room_id}-{stamp}");

        let sup_config = SupervisorConfig {
            room_id,
            output_dir: dir.clone(),
            stem_base: stem_base.clone(),
            segment: self.config.segment.clone(),
            stall_timeout: self.config.stall_timeout,
            min_healthy: self.config.min_healthy,
            max_rapid_failures: self.config.max_rapid_failures,
            extension_override: self.config.extension_override.clone(),
            min_free_bytes: self.config.min_free_bytes,
            free_space_fn: None,
            pcm_tx: self.config.pcm_tx.clone(),
        };
        let (stop_tx, stop_rx) = watch::channel(false);
        let resolver = self.resolver.clone();
        let recorder = self.recorder.clone();
        let task = tokio::spawn(async move {
            supervise_recording(resolver.as_ref(), &recorder, &sup_config, stop_rx, None).await
        });

        let session = RecordingSession::start(room_id, None, &dir);
        Ok(ActiveRecording {
            stop: stop_tx,
            task,
            session,
            dir,
            stem_base,
        })
    }

    async fn stop_and_finalize(&self, rec: ActiveRecording) -> Result<SessionMetadata> {
        let _ = rec.stop.send(true);
        // Graceful stop is bounded inside the supervisor (SIGINT + 8s).
        let _ = tokio::time::timeout(Duration::from_secs(20), rec.task).await;
        self.finalize_inner(rec.session, &rec.stem_base).await
    }

    async fn finalize(&self, rec: ActiveRecording) -> Result<SessionMetadata> {
        self.finalize_inner(rec.session, &rec.stem_base).await
    }

    async fn finalize_inner(
        &self,
        session: RecordingSession,
        stem_base: &str,
    ) -> Result<SessionMetadata> {
        let mut metadata = session.finalize_media(stem_base)?;

        let have_ffmpeg = binary_exists(&self.recorder.binary);
        if have_ffmpeg {
            // Health check on the largest part.
            if self.config.health_check {
                if let Some(largest) = metadata
                    .segments
                    .iter()
                    .max_by_key(|s| s.size_bytes)
                    .map(|s| s.path.clone())
                {
                    metadata.health =
                        Some(verify_recording(&self.recorder.binary, &largest).await);
                }
            }
            // Player-friendly MP4 next to each part.
            if self.config.remux_mp4 {
                for seg in &metadata.segments {
                    if seg.path.extension().map(|e| e != "mp4").unwrap_or(true) {
                        if let Err(e) = remux_mp4(&self.recorder.binary, &seg.path).await {
                            tracing::warn!("remux failed for {:?}: {e}", seg.path);
                        }
                    }
                }
            }
        }

        session.export_manifest(&metadata, stem_base)?;
        Ok(metadata)
    }
}

/// Production resolver: uses the Bilibili playurl API, preferring FLV at
/// the highest available quality (AVC over HEVC for player compat).
pub struct BiliResolver {
    api: crate::stream::StreamApi,
    /// Requested quality (see [`crate::stream::qn`]).
    pub qn: u32,
}

impl BiliResolver {
    pub fn new(api: crate::stream::StreamApi, qn: u32) -> Self {
        Self { api, qn }
    }
}

#[async_trait]
impl StreamResolver for BiliResolver {
    async fn resolve(&self, room_id: u64) -> Result<ResolvedStream> {
        let streams = self.api.play_info(room_id, self.qn).await?;
        let best = crate::stream::pick_best(&streams)
            .ok_or(crate::error::RecorderError::NoStreamUrl)?;
        let extension = match best.format.as_str() {
            "flv" => "flv",
            "ts" => "ts",
            _ => "mp4", // fmp4/hls
        };
        Ok(ResolvedStream {
            url: best.url.clone(),
            headers: vec![(
                "Referer".to_string(),
                "https://live.bilibili.com/".to_string(),
            )],
            user_agent: Some(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/120.0 Safari/537.36"
                    .into(),
            ),
            extension: extension.into(),
            backup_urls: best.backup_urls.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct FakeResolver {
        calls: Arc<AtomicU32>,
    }

    #[async_trait]
    impl StreamResolver for FakeResolver {
        async fn resolve(&self, _room_id: u64) -> Result<ResolvedStream> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ResolvedStream {
                url: "ignored".into(),
                headers: vec![],
                user_agent: None,
                extension: "flv".into(),
                backup_urls: vec![],
            })
        }
    }

    struct FailingResolver;

    #[async_trait]
    impl StreamResolver for FailingResolver {
        async fn resolve(&self, _room_id: u64) -> Result<ResolvedStream> {
            Err(crate::error::RecorderError::NoStreamUrl)
        }
    }

    /// A fake ffmpeg that keeps writing to its output (last arg) so the
    /// watchdog sees healthy growth. Unique path per call — parallel tests
    /// must not overwrite a script another test is executing.
    fn fake_recorder() -> FfmpegRecorder {
        use std::sync::atomic::AtomicU64;
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join("vtb-recorder-test-bin");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!(
            "fake-writer-{}-{}.sh",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::write(
            &path,
            "#!/bin/sh\ntrap 'exit 0' INT TERM\nout=$(eval echo \\${$#})\nwhile true; do echo data >> \"$out\"; sleep 1; done\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        FfmpegRecorder::new(path)
    }

    fn test_config(room_id: u64, root: &std::path::Path) -> AutoRecorderConfig {
        let mut cfg = AutoRecorderConfig::new(room_id, root);
        cfg.remux_mp4 = false; // fake recorder produces non-media data
        cfg.health_check = false; // fake "ffmpeg" must never be probed
        cfg.max_rapid_failures = 1; // fail fast in tests
        cfg
    }

    #[tokio::test]
    async fn live_then_offline_produces_start_stop_events() {
        let dir = tempfile::tempdir().unwrap();
        let calls = Arc::new(AtomicU32::new(0));
        let auto = AutoRecorder::new(
            test_config(42, dir.path()),
            FakeResolver { calls: calls.clone() },
            fake_recorder(),
        );

        let (mtx, mrx) = mpsc::channel(8);
        let (etx, mut erx) = mpsc::channel(8);
        let handle = tokio::spawn(auto.run(mrx, etx));

        mtx.send(MonitorEvent::WentLive { room_id: 42 }).await.unwrap();
        let started = erx.recv().await.unwrap();
        let session_dir = match &started {
            RecorderEvent::RecordingStarted { room_id: 42, output_dir } => output_dir.clone(),
            other => panic!("expected start, got {other:?}"),
        };

        // Wait until the fake recorder has produced a non-empty part file
        // (it appends once per second; a fixed sleep is flaky under
        // parallel-suite load).
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            let has_part = std::fs::read_dir(&session_dir)
                .map(|rd| {
                    rd.flatten().any(|e| {
                        e.path().extension().map(|x| x == "flv").unwrap_or(false)
                            && e.metadata().map(|m| m.len() > 0).unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if has_part {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "fake recorder never wrote a part file"
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        mtx.send(MonitorEvent::WentOffline { room_id: 42 }).await.unwrap();
        let stopped = erx.recv().await.unwrap();
        match stopped {
            RecorderEvent::RecordingStopped { room_id, metadata } => {
                assert_eq!(room_id, 42);
                assert_eq!(metadata.room_id, 42);
                assert!(metadata.ended_at.is_some());
                assert!(!metadata.segments.is_empty(), "part file expected");
            }
            other => panic!("expected stop, got {other:?}"),
        }

        drop(mtx);
        handle.await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let mut found_manifest = false;
        for entry in walk(dir.path()) {
            if entry.to_string_lossy().ends_with(".meta.json") {
                found_manifest = true;
            }
        }
        assert!(found_manifest, "manifest json should exist");
    }

    #[tokio::test]
    async fn duplicate_live_events_do_not_double_record() {
        let dir = tempfile::tempdir().unwrap();
        let calls = Arc::new(AtomicU32::new(0));
        let auto = AutoRecorder::new(
            test_config(1, dir.path()),
            FakeResolver { calls: calls.clone() },
            fake_recorder(),
        );
        let (mtx, mrx) = mpsc::channel(8);
        let (etx, mut erx) = mpsc::channel(8);
        let handle = tokio::spawn(auto.run(mrx, etx));

        mtx.send(MonitorEvent::WentLive { room_id: 1 }).await.unwrap();
        let _ = erx.recv().await.unwrap();
        mtx.send(MonitorEvent::WentLive { room_id: 1 }).await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        drop(mtx);
        handle.await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "second live must be ignored");
    }

    #[tokio::test]
    async fn resolver_failure_reports_error_event() {
        let dir = tempfile::tempdir().unwrap();
        let auto = AutoRecorder::new(
            test_config(9, dir.path()),
            FailingResolver,
            fake_recorder(),
        );
        let (mtx, mrx) = mpsc::channel(8);
        let (etx, mut erx) = mpsc::channel(8);
        let handle = tokio::spawn(auto.run(mrx, etx));

        mtx.send(MonitorEvent::WentLive { room_id: 9 }).await.unwrap();
        // Start event fires (session dir created), then the supervisor
        // gives up and an error event follows.
        let mut saw_error = false;
        for _ in 0..3 {
            match tokio::time::timeout(Duration::from_secs(15), erx.recv()).await {
                Ok(Some(RecorderEvent::RecordingError { room_id: 9, .. })) => {
                    saw_error = true;
                    break;
                }
                Ok(Some(_)) => {}
                _ => break,
            }
        }
        assert!(saw_error, "expected an error event");
        drop(mtx);
        handle.await.unwrap();
    }

    fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut out = vec![];
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    out.extend(walk(&p));
                } else {
                    out.push(p);
                }
            }
        }
        out
    }
}
