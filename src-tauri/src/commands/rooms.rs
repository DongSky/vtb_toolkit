//! Room info queries for the multi-room dashboard.

use crate::state::AppState;
use serde::Serialize;
use tauri::State;

#[derive(Serialize, Clone)]
pub struct RoomCard {
    pub room_id: u64,
    pub real_room_id: u64,
    pub title: String,
    pub uname: String,
    pub cover: String,
    pub area: String,
    /// 0 offline, 1 live, 2 round.
    pub live_status: u8,
    pub live_start_time: Option<String>,
    /// App-side state for this room.
    pub danmaku_connected: bool,
    pub recording: bool,
}

/// Fetch card info for one room (short or real id).
#[tauri::command]
pub async fn room_info(
    state: State<'_, AppState>,
    room_id: u64,
) -> Result<RoomCard, String> {
    let client = state.api_client()?;

    // Resolve short id → real id + coarse status.
    let init: serde_json::Value = client
        .get("https://api.live.bilibili.com/room/v1/Room/room_init")
        .query(&[("id", room_id)])
        .header("Referer", "https://live.bilibili.com/")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if init["code"].as_i64() != Some(0) {
        return Err(format!(
            "room_init: {}",
            init["message"].as_str().unwrap_or("unknown error")
        ));
    }
    let real = init["data"]["room_id"].as_u64().unwrap_or(room_id);

    // Rich info (title/cover/area/anchor name).
    let info: serde_json::Value = client
        .get("https://api.live.bilibili.com/xlive/web-room/v1/index/getInfoByRoom")
        .query(&[("room_id", real)])
        .header("Referer", "https://live.bilibili.com/")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let ri = &info["data"]["room_info"];
    let anchor = &info["data"]["anchor_info"]["base_info"];

    Ok(RoomCard {
        room_id,
        real_room_id: real,
        title: ri["title"].as_str().unwrap_or("").to_string(),
        uname: anchor["uname"].as_str().unwrap_or("").to_string(),
        cover: ri["cover"].as_str().unwrap_or("").to_string(),
        area: format!(
            "{}·{}",
            ri["parent_area_name"].as_str().unwrap_or(""),
            ri["area_name"].as_str().unwrap_or("")
        ),
        live_status: ri["live_status"].as_u64().unwrap_or(
            init["data"]["live_status"].as_u64().unwrap_or(0),
        ) as u8,
        live_start_time: ri["live_start_time"].as_i64().and_then(|t| {
            (t > 0).then(|| {
                chrono::DateTime::from_timestamp(t, 0)
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_default()
            })
        }),
        danmaku_connected: state.danmaku_connected(real) || state.danmaku_connected(room_id),
        recording: state.recorder_active(real) || state.recorder_active(room_id),
    })
}

/// Probe any supported platform id/URL (bilibili room, YouTube/Twitch URL
/// via yt-dlp) — returns platform name + live status.
#[tauri::command]
pub async fn platform_probe(
    state: tauri::State<'_, crate::state::AppState>,
    id: String,
) -> Result<serde_json::Value, String> {
    let client = state.api_client()?;
    let platform = vtb_recorder::platform::platform_for(&id, client, vtb_recorder::stream::qn::ORIGINAL);
    let status = platform
        .check_live(&id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "platform": platform.name(),
        "live": matches!(status, vtb_common::LiveStatus::Live),
    }))
}
