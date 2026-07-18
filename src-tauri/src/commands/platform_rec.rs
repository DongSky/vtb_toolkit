//! Multi-platform recording (YouTube/Twitch via yt-dlp): monitor a channel
//! URL and run the same supervised ffmpeg recording used for B站.

use crate::state::AppState;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};
use vtb_recorder::auto::{ResolvedStream, StreamResolver};
use vtb_recorder::platform::{platform_for, Platform};
use vtb_recorder::supervisor::{supervise_recording, SupervisorConfig, SupervisorEnd};

pub const EVENT_PLATFORM_REC: &str = "platform_rec://event";

#[derive(serde::Serialize, Clone)]
struct PlatformRecPayload {
    id: String,
    kind: String, // waiting | started | part | stopped | error
    message: String,
}

/// Adapter: a Platform + fixed id looks like a StreamResolver to the
/// supervisor (which is keyed by numeric room ids it never uses here).
struct PlatformResolver {
    platform: Box<dyn Platform>,
    id: String,
}

#[async_trait::async_trait]
impl StreamResolver for PlatformResolver {
    async fn resolve(&self, _room_id: u64) -> vtb_recorder::error::Result<ResolvedStream> {
        self.platform.resolve(&self.id).await
    }
}

/// Filesystem-safe session key for a channel URL.
fn slug(id: &str) -> String {
    let cleaned: String = id
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    cleaned.trim_matches('_').chars().take(80).collect()
}

#[tauri::command]
pub async fn platform_record_start(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    output_dir: String,
) -> Result<(), String> {
    let key = slug(&id);
    if key.is_empty() {
        return Err("无效的直播间 URL".into());
    }
    {
        let map = state.platform_recorders.lock().unwrap();
        if map.get(&key).map(|h| !h.pump.is_finished()).unwrap_or(false) {
            return Err(format!("{id} 已在监听录制"));
        }
    }

    let client = state.api_client()?;
    let platform = platform_for(&id, client, vtb_recorder::stream::qn::ORIGINAL);
    let resolver = PlatformResolver {
        platform,
        id: id.clone(),
    };
    let root = PathBuf::from(&output_dir).join(format!("platform-{key}"));
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let app2 = app.clone();
    let id2 = id.clone();
    let key2 = key.clone();

    let pump = tokio::spawn(async move {
        let emit = |kind: &str, message: String| {
            let _ = app2.emit(
                EVENT_PLATFORM_REC,
                PlatformRecPayload {
                    id: id2.clone(),
                    kind: kind.into(),
                    message,
                },
            );
        };
        // Wait for live, then record until the stream ends or stop.
        let mut stop_rx = stop_rx;
        loop {
            if *stop_rx.borrow() {
                emit("stopped", "已停止".into());
                return;
            }
            match resolver.platform.check_live(&resolver.id).await {
                Ok(vtb_common::LiveStatus::Live) => {}
                Ok(_) => {
                    emit("waiting", "未开播，30s 后重查".into());
                    tokio::select! {
                        _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => continue,
                        _ = stop_rx.changed() => continue,
                    }
                }
                Err(e) => {
                    emit("error", format!("状态检查失败: {e}"));
                    tokio::select! {
                        _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => continue,
                        _ = stop_rx.changed() => continue,
                    }
                }
            }

            let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
            let session_dir = root.join(stamp.to_string());
            if let Err(e) = std::fs::create_dir_all(&session_dir) {
                emit("error", format!("创建目录失败: {e}"));
                return;
            }
            emit("started", session_dir.to_string_lossy().into_owned());

            let cfg = SupervisorConfig::new(0, &session_dir, &key2);
            let end = supervise_recording(
                &resolver,
                &vtb_recorder::ffmpeg::FfmpegRecorder::default(),
                &cfg,
                stop_rx.clone(),
                None,
            )
            .await;
            match end {
                SupervisorEnd::Stopped => {
                    emit("stopped", "录制结束".into());
                }
                SupervisorEnd::GaveUp { last_error, .. } => {
                    emit("error", format!("放弃录制: {last_error}"));
                }
                SupervisorEnd::DiskFull { free_bytes } => {
                    emit("error", format!("磁盘不足: 剩余 {free_bytes} 字节"));
                    return;
                }
            }
            // Back to waiting-for-live (stream likely ended).
        }
    });

    state.platform_recorders.lock().unwrap().insert(
        key,
        crate::state::PlatformRecHandles { pump, stop: stop_tx },
    );
    Ok(())
}

#[tauri::command]
pub async fn platform_record_stop(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    // Accept either the original URL (slugged) or the slug key shown in
    // the active list.
    let key = slug(&id);
    let mut map = state.platform_recorders.lock().unwrap();
    let handle = map.remove(&key).or_else(|| map.remove(id.trim()));
    if let Some(h) = handle {
        // Graceful only: the supervisor stops ffmpeg via signal so the
        // container header is finalized (abort would SIGKILL mid-write).
        let _ = h.stop.send(true);
        Ok(())
    } else {
        Err(format!("{id} 未在录制"))
    }
}

#[tauri::command]
pub async fn platform_record_status(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let map = state.platform_recorders.lock().unwrap();
    Ok(map
        .iter()
        .filter(|(_, h)| !h.pump.is_finished())
        .map(|(k, _)| k.clone())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn slug_sanitizes_urls() {
        assert_eq!(
            slug("https://www.twitch.tv/some_streamer"),
            "twitch_tv_some_streamer"
        );
        assert_eq!(
            slug("https://youtube.com/@VtuberChannel/live"),
            "youtube_com__VtuberChannel_live"
        );
        assert_eq!(slug("   "), "");
    }
}
