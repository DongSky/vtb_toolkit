//! 打点 commands: manual markers during a live session + marker/event
//! timeline data for the review page.
//!
//! Markers are wall-clock entries in `<session_dir>/markers.jsonl` — the
//! same convention as `danmaku.jsonl` — so their offsets align with the
//! recording via the manifest's `started_at`, no matter which subsystem
//! wrote them.

use crate::state::AppState;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, State};
use vtb_pipeline::markers::{self, Marker};

/// Emitted whenever a marker is appended (frontend toast + list refresh).
pub const EVENT_MARKER: &str = "marker://added";

#[derive(serde::Serialize, Clone)]
pub struct MarkerPayload {
    pub room_id: u64,
    pub kind: String,
    pub source: String,
    pub note: Option<String>,
    pub created_at: String,
}

fn payload(room_id: u64, m: &Marker) -> MarkerPayload {
    MarkerPayload {
        room_id,
        kind: match m.kind {
            vtb_pipeline::markers::MarkerKind::Auto => "auto".into(),
            vtb_pipeline::markers::MarkerKind::Manual => "manual".into(),
        },
        source: m.source.clone(),
        note: m.note.clone(),
        created_at: m.created_at.to_rfc3339(),
    }
}

/// Append a marker for a room's active session and notify the UI.
/// Shared by the command below, the hotkey handler, and the danmaku
/// command trigger. No active recording session → error (there is no
/// timeline to anchor the point to).
pub fn add_marker(
    app: &AppHandle,
    marker_dirs: &std::sync::Mutex<std::collections::HashMap<u64, PathBuf>>,
    room_id: u64,
    marker: Marker,
) -> Result<(), String> {
    let dir = marker_dirs
        .lock()
        .unwrap()
        .get(&room_id)
        .cloned()
        .ok_or_else(|| format!("房间 {room_id} 没有进行中的录制会话，无法打点"))?;
    markers::append_marker(&dir, &marker).map_err(|e| e.to_string())?;
    let _ = app.emit(EVENT_MARKER, payload(room_id, &marker));
    Ok(())
}

/// Room of the single active session, when unambiguous (hotkey has no
/// room context; with several simultaneous recordings we refuse rather
/// than guess).
pub fn sole_active_room(
    marker_dirs: &std::sync::Mutex<std::collections::HashMap<u64, PathBuf>>,
) -> Result<u64, String> {
    let map = marker_dirs.lock().unwrap();
    match map.len() {
        0 => Err("没有进行中的录制会话".into()),
        1 => Ok(*map.keys().next().unwrap()),
        n => Err(format!("有 {n} 个录制会话，请在弹幕面板指定房间打点")),
    }
}

/// 弹幕口令: "打点" or "打点 <备注>" from the streamer or a room admin
/// drops a manual marker. Returns the (possibly empty) note when the text
/// is the command.
pub fn danmaku_marker_note(text: &str) -> Option<Option<String>> {
    let t = text.trim();
    let rest = t.strip_prefix("打点")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None; // e.g. "打点歌吧" is chatter, not a command
    }
    let note = rest.trim();
    Some((!note.is_empty()).then(|| note.to_string()))
}

#[tauri::command]
pub async fn marker_add(
    app: AppHandle,
    state: State<'_, AppState>,
    room_id: u64,
    note: Option<String>,
) -> Result<(), String> {
    add_marker(
        &app,
        &state.marker_dirs,
        room_id,
        Marker::manual("button", note.filter(|n| !n.trim().is_empty())),
    )
}

/// Rooms with an active recording session (marker targets).
#[tauri::command]
pub async fn marker_rooms(state: State<'_, AppState>) -> Result<Vec<u64>, String> {
    Ok(state.marker_dirs.lock().unwrap().keys().copied().collect())
}

pub fn add_source_marker(
    app: &AppHandle,
    state: &AppState,
    key: &str,
    marker: Marker,
) -> Result<(), String> {
    if let Some(id) = key.strip_prefix("bilibili:").and_then(|id| id.parse().ok()) {
        return add_marker(app, &state.marker_dirs, id, marker);
    }
    let dirs = state.source_marker_dirs.lock().unwrap();
    let dir = dirs.get(key).ok_or("该直播没有进行中的录制会话")?;
    markers::append_marker(dir, &marker).map_err(|e| e.to_string())?;
    let mut p = serde_json::to_value(payload(0, &marker)).map_err(|e| e.to_string())?;
    p["room_key"] = serde_json::json!(key);
    let _ = app.emit(EVENT_MARKER, p);
    Ok(())
}

#[tauri::command]
pub fn marker_sources(state: State<'_, AppState>) -> Vec<String> {
    let mut keys: Vec<String> = state
        .marker_dirs
        .lock()
        .unwrap()
        .keys()
        .map(|id| format!("bilibili:{id}"))
        .collect();
    keys.extend(state.source_marker_dirs.lock().unwrap().keys().cloned());
    keys.sort();
    keys
}

#[tauri::command]
pub fn marker_add_source(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
    note: Option<String>,
) -> Result<(), String> {
    add_source_marker(&app, &state, &key, Marker::manual("button", note))
}

pub fn hotkey_marker(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let mut keys: Vec<String> = state
        .marker_dirs
        .lock()
        .unwrap()
        .keys()
        .map(|id| format!("bilibili:{id}"))
        .collect();
    keys.extend(state.source_marker_dirs.lock().unwrap().keys().cloned());
    if keys.len() != 1 {
        return Err("请在弹幕页选择要打点的录制会话".into());
    }
    add_source_marker(app, state, &keys[0], Marker::manual("hotkey", None))
}

/// Timeline events for the review page: markers + paid/notable live
/// events (SC / 舰长 / 开播) as offsets against the session start.
#[derive(serde::Serialize)]
pub struct TimelineData {
    pub markers: Vec<serde_json::Value>,
    pub events: Vec<TimelineEvent>,
}

#[derive(serde::Serialize)]
pub struct TimelineEvent {
    pub at_ms: u64,
    /// "super_chat" | "guard_buy" | "live_start" | "live_end"
    pub kind: String,
    pub label: String,
}

/// Locate the recording session directory for an analysis output dir:
/// the layout is `<session>/out/` next to `<session>/danmaku.jsonl`, but
/// accept the session dir itself too.
fn session_dir_of(dir: &Path) -> Option<PathBuf> {
    if dir.join("danmaku.jsonl").exists() || dir.join("markers.jsonl").exists() {
        return Some(dir.to_path_buf());
    }
    let parent = dir.parent()?;
    (parent.join("danmaku.jsonl").exists() || parent.join("markers.jsonl").exists())
        .then(|| parent.to_path_buf())
}

/// Public wrapper for other command modules (editor export).
pub fn session_dir_of_pub(dir: &Path) -> Option<PathBuf> {
    session_dir_of(dir)
}

fn session_start_of(dir: &Path) -> Option<chrono::DateTime<chrono::Utc>> {
    let rd = std::fs::read_dir(dir).ok()?;
    for entry in rd.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(".meta.json") {
            let text = std::fs::read_to_string(entry.path()).ok()?;
            let v: serde_json::Value = serde_json::from_str(&text).ok()?;
            return v["started_at"]
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&chrono::Utc));
        }
    }
    None
}

/// Public wrapper for other command modules (editor export).
pub fn session_start_of_pub(dir: &Path) -> Option<chrono::DateTime<chrono::Utc>> {
    session_start_of(dir)
}

/// Load 打点 + SC/舰长/开播 offsets for a review directory (the offline
/// output dir or the session dir itself).
#[tauri::command]
pub async fn marker_timeline(dir: String) -> Result<TimelineData, String> {
    let dir = PathBuf::from(dir);
    let session = session_dir_of(&dir).ok_or("找不到录制会话目录（无 danmaku/markers 日志）")?;
    let start = session_start_of(&session).ok_or("会话缺少 meta.json，无法对齐时间轴")?;

    let ms = markers::read_markers(&session).map_err(|e| e.to_string())?;
    let marker_offsets = markers::to_offsets(&ms, start)
        .into_iter()
        .map(|m| serde_json::to_value(&m).unwrap_or_default())
        .collect();

    // Paid/notable events from the danmaku log; big logs are line-filtered
    // by kind tag before JSON parsing to keep this snappy.
    let mut events = Vec::new();
    let log = session.join("danmaku.jsonl");
    if log.exists() {
        for entry in vtb_pipeline::danmaku_log::read_log(&log).map_err(|e| e.to_string())? {
            let ms = (entry.received_at - start).num_milliseconds();
            if ms < 0 {
                continue;
            }
            let (kind, label) = match &entry.event {
                vtb_common::LiveEvent::SuperChat(sc) => (
                    "super_chat",
                    format!(
                        "SC {} {}: {}",
                        entry
                            .source
                            .as_ref()
                            .and_then(|s| s.money.as_ref())
                            .map(|m| m.display.clone())
                            .unwrap_or_else(|| format!("¥{:.0}", sc.price)),
                        sc.username,
                        sc.text
                    ),
                ),
                vtb_common::LiveEvent::GuardBuy(g) => {
                    let tier = match g.guard_level {
                        1 => "总督",
                        2 => "提督",
                        _ => "舰长",
                    };
                    (
                        "guard_buy",
                        format!(
                            "{}: {}",
                            g.username,
                            entry
                                .source
                                .as_ref()
                                .and_then(|s| s.membership.as_deref())
                                .unwrap_or(tier)
                        ),
                    )
                }
                vtb_common::LiveEvent::LiveStart { .. } => ("live_start", "开播".into()),
                vtb_common::LiveEvent::LiveEnd { .. } => ("live_end", "下播".into()),
                _ => continue,
            };
            events.push(TimelineEvent {
                at_ms: ms as u64,
                kind: kind.into(),
                label,
            });
        }
    }

    Ok(TimelineData {
        markers: marker_offsets,
        events,
    })
}

/// Export `timestamps.txt` (B站评论/简介格式) for a review directory:
/// markers + highlights merged on one timeline.
#[tauri::command]
pub async fn timestamps_export(dir: String) -> Result<String, String> {
    let dir = PathBuf::from(dir);
    let highlights: Vec<vtb_common::Highlight> =
        std::fs::read_to_string(dir.join("highlights.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();

    let mut marker_offsets = vec![];
    if let Some(session) = session_dir_of(&dir) {
        if let Some(start) = session_start_of(&session) {
            let ms = markers::read_markers(&session).map_err(|e| e.to_string())?;
            marker_offsets = markers::to_offsets(&ms, start);
        }
    }
    if highlights.is_empty() && marker_offsets.is_empty() {
        return Err("没有可导出的打点或高能片段".into());
    }
    let text = markers::timestamp_list(&marker_offsets, &highlights);
    let out = dir.join("timestamps.txt");
    std::fs::write(&out, text).map_err(|e| e.to_string())?;
    Ok(out.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn session_dir_resolution() {
        let dir = tempfile::tempdir().unwrap();
        let session = dir.path();
        let out = session.join("out");
        std::fs::create_dir_all(&out).unwrap();

        // Nothing yet → None.
        assert!(session_dir_of(&out).is_none());

        std::fs::write(session.join("danmaku.jsonl"), b"").unwrap();
        // From the analysis output dir, resolves to the parent session.
        assert_eq!(session_dir_of(&out).unwrap(), session);
        // From the session dir itself, identity.
        assert_eq!(session_dir_of(session).unwrap(), session);
    }

    #[test]
    fn session_start_parses_manifest() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("rec.meta.json"),
            r#"{"room_id":1,"started_at":"2026-07-20T12:00:00Z","segments":[]}"#,
        )
        .unwrap();
        let start = session_start_of(dir.path()).unwrap();
        assert_eq!(start, Utc.with_ymd_and_hms(2026, 7, 20, 12, 0, 0).unwrap());
    }

    #[test]
    fn sole_active_room_rules() {
        let dirs: std::sync::Mutex<std::collections::HashMap<u64, PathBuf>> = Default::default();
        assert!(sole_active_room(&dirs).is_err());
        dirs.lock().unwrap().insert(7, PathBuf::from("/a"));
        assert_eq!(sole_active_room(&dirs).unwrap(), 7);
        dirs.lock().unwrap().insert(8, PathBuf::from("/b"));
        assert!(sole_active_room(&dirs).is_err());
    }

    #[test]
    fn danmaku_command_parsing() {
        assert_eq!(danmaku_marker_note("打点"), Some(None));
        assert_eq!(
            danmaku_marker_note("打点 名场面"),
            Some(Some("名场面".into()))
        );
        assert_eq!(danmaku_marker_note("  打点  "), Some(None));
        // Chatter containing the word is not a command.
        assert_eq!(danmaku_marker_note("打点歌吧"), None);
        assert_eq!(danmaku_marker_note("快打点"), None);
    }
}
