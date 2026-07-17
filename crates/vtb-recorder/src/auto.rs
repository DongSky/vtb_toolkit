//! Auto-recording orchestrator: reacts to monitor events by resolving a
//! stream URL, spawning/stopping the recorder, and exporting session
//! metadata when the stream ends.

use crate::error::Result;
use crate::ffmpeg::{FfmpegRecorder, RecordOptions, SegmentPolicy};
use crate::monitor::MonitorEvent;
use crate::session::{RecordingSession, SessionMetadata};
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::sync::mpsc;

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
}

/// Drives ffmpeg based on monitor events. One instance per room.
pub struct AutoRecorder<R: StreamResolver> {
    config: AutoRecorderConfig,
    resolver: R,
    recorder: FfmpegRecorder,
}

struct ActiveRecording {
    child: tokio::process::Child,
    session: RecordingSession,
    dir: PathBuf,
    stem: String,
    ext: String,
}

impl<R: StreamResolver> AutoRecorder<R> {
    pub fn new(config: AutoRecorderConfig, resolver: R, recorder: FfmpegRecorder) -> Self {
        Self {
            config,
            resolver,
            recorder,
        }
    }

    /// Output directory for a session started now.
    fn session_dir(&self, stamp: &str) -> PathBuf {
        self.config
            .output_root
            .join(format!("room{}", self.config.room_id))
            .join(stamp)
    }

    /// Consume monitor events until the channel closes; emit recorder
    /// events. Handles start → stop → export.
    pub async fn run(
        mut self,
        mut events: mpsc::Receiver<MonitorEvent>,
        tx: mpsc::Sender<RecorderEvent>,
    ) {
        let mut active: Option<ActiveRecording> = None;
        while let Some(ev) = events.recv().await {
            match ev {
                MonitorEvent::WentLive { room_id } => {
                    if active.is_some() {
                        continue;
                    }
                    match self.start_recording(room_id).await {
                        Ok(rec) => {
                            let _ = tx
                                .send(RecorderEvent::RecordingStarted {
                                    room_id,
                                    output_dir: rec.dir.clone(),
                                })
                                .await;
                            active = Some(rec);
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
                        match Self::stop_recording(rec).await {
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
        // Channel closed: stop any active recording gracefully.
        if let Some(rec) = active.take() {
            let _ = Self::stop_recording(rec).await;
        }
    }

    async fn start_recording(&mut self, room_id: u64) -> Result<ActiveRecording> {
        let stream = self.resolver.resolve(room_id).await?;
        let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
        let dir = self.session_dir(&stamp);
        let stem = format!("room{room_id}-{stamp}");
        let ext = self
            .config
            .extension_override
            .clone()
            .unwrap_or(stream.extension);

        let mut opts = RecordOptions::new(stream.url, &dir, &stem);
        opts.extension = ext.clone();
        opts.segment = self.config.segment.clone();
        opts.headers = stream.headers;
        opts.user_agent = stream.user_agent;

        let child = self.recorder.spawn(&opts)?;
        let session = RecordingSession::start(room_id, None, &dir);
        Ok(ActiveRecording {
            child,
            session,
            dir,
            stem,
            ext,
        })
    }

    async fn stop_recording(mut rec: ActiveRecording) -> Result<SessionMetadata> {
        // Graceful stop: SIGKILL corrupts containers with trailers; FLV/TS
        // tolerate it but we still prefer a clean shutdown. `start_kill`
        // sends SIGKILL, so try a polite kill via the child's stdin-less
        // process: on unix send SIGINT.
        #[cfg(unix)]
        {
            if let Some(pid) = rec.child.id() {
                unsafe {
                    libc::kill(pid as i32, libc::SIGINT);
                }
                // Give ffmpeg a moment to finalize.
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    rec.child.wait(),
                )
                .await;
            }
        }
        // Fallback / non-unix: hard kill.
        let _ = rec.child.start_kill();
        let _ = rec.child.wait().await;

        let metadata = rec.session.finalize(&rec.stem, &rec.ext)?;
        rec.session.export_manifest(&metadata, &rec.stem)?;
        Ok(metadata)
    }
}

/// Production resolver: uses the Bilibili playurl API, preferring FLV at
/// the highest available quality.
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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    /// Resolver that "streams" from a local file:// (we actually just use
    /// a command that sleeps — ffmpeg isn't needed for orchestration
    /// tests; we use `sleep` via a fake ffmpeg binary).
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

    /// A "recorder" whose binary is `/bin/sleep`-like: it ignores args and
    /// sleeps, standing in for a long-running ffmpeg.
    fn fake_recorder() -> FfmpegRecorder {
        // `tail -f /dev/null` isn't spawnable via one binary path; use
        // `sleep` with args appended after — sleep ignores extra args?
        // It doesn't, so use /usr/bin/yes redirected? Simplest portable
        // stand-in: /bin/cat with no args blocks on stdin (which we set
        // to null → EOF → exits immediately). Use `sleep` wrapper script.
        FfmpegRecorder::new(test_sleep_script())
    }

    /// Write a tiny script that sleeps regardless of arguments.
    fn test_sleep_script() -> PathBuf {
        let dir = std::env::temp_dir().join("vtb-recorder-test-bin");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fake-ffmpeg.sh");
        if !path.exists() {
            std::fs::write(&path, "#!/bin/sh\nsleep 30\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                    .unwrap();
            }
        }
        path
    }

    #[tokio::test]
    async fn live_then_offline_produces_start_stop_events() {
        let dir = tempfile::tempdir().unwrap();
        let config = AutoRecorderConfig {
            room_id: 42,
            output_root: dir.path().to_path_buf(),
            segment: SegmentPolicy::Single,
            extension_override: None,
        };
        let calls = Arc::new(AtomicU32::new(0));
        let auto = AutoRecorder::new(
            config,
            FakeResolver {
                calls: calls.clone(),
            },
            fake_recorder(),
        );

        let (mtx, mrx) = mpsc::channel(8);
        let (etx, mut erx) = mpsc::channel(8);
        let handle = tokio::spawn(auto.run(mrx, etx));

        mtx.send(MonitorEvent::WentLive { room_id: 42 }).await.unwrap();
        let started = erx.recv().await.unwrap();
        assert!(matches!(started, RecorderEvent::RecordingStarted { room_id: 42, .. }));
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        mtx.send(MonitorEvent::WentOffline { room_id: 42 }).await.unwrap();
        let stopped = erx.recv().await.unwrap();
        match stopped {
            RecorderEvent::RecordingStopped { room_id, metadata } => {
                assert_eq!(room_id, 42);
                assert_eq!(metadata.room_id, 42);
                assert!(metadata.ended_at.is_some());
            }
            other => panic!("expected stop, got {other:?}"),
        }

        drop(mtx);
        handle.await.unwrap();

        // Manifest exported.
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
        let config = AutoRecorderConfig {
            room_id: 1,
            output_root: dir.path().to_path_buf(),
            segment: SegmentPolicy::Single,
            extension_override: None,
        };
        let calls = Arc::new(AtomicU32::new(0));
        let auto = AutoRecorder::new(
            config,
            FakeResolver {
                calls: calls.clone(),
            },
            fake_recorder(),
        );
        let (mtx, mrx) = mpsc::channel(8);
        let (etx, mut erx) = mpsc::channel(8);
        let handle = tokio::spawn(auto.run(mrx, etx));

        mtx.send(MonitorEvent::WentLive { room_id: 1 }).await.unwrap();
        let _ = erx.recv().await.unwrap();
        mtx.send(MonitorEvent::WentLive { room_id: 1 }).await.unwrap();
        drop(mtx);
        handle.await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1, "second live must be ignored");
    }

    #[tokio::test]
    async fn resolver_failure_reports_error_event() {
        let dir = tempfile::tempdir().unwrap();
        let config = AutoRecorderConfig {
            room_id: 9,
            output_root: dir.path().to_path_buf(),
            segment: SegmentPolicy::Single,
            extension_override: None,
        };
        let auto = AutoRecorder::new(config, FailingResolver, fake_recorder());
        let (mtx, mrx) = mpsc::channel(8);
        let (etx, mut erx) = mpsc::channel(8);
        let handle = tokio::spawn(auto.run(mrx, etx));

        mtx.send(MonitorEvent::WentLive { room_id: 9 }).await.unwrap();
        let ev = erx.recv().await.unwrap();
        assert!(matches!(ev, RecorderEvent::RecordingError { room_id: 9, .. }));
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
