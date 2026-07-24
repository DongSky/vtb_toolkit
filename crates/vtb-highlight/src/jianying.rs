//! 剪映专业版 draft export (experimental).
//!
//! Generates a draft folder (`draft_content.json` + `draft_meta_info.json`)
//! with every 高能片段 pre-cut on the video track, referencing the source
//! recording in place. Copy the folder into 剪映's draft root (macOS:
//! `~/Movies/JianyingPro/User Data/Projects/com.lveditor.draft/`) and it
//! appears in the project list ready for fine cutting.
//!
//! The draft format is unofficial and drifts between versions; the JSON
//! written here follows the structure documented by the pyJianYingDraft
//! project for 剪映专业版 5.x/6.x (CN). All timeline values are in
//! MICROSECONDS, which is the single most common integration mistake.

use crate::error::{HighlightError, Result};
use serde_json::json;
use std::path::{Path, PathBuf};
use vtb_common::Highlight;

/// Unique uppercase UUID-shaped id. Uniqueness (per process) is what 剪映
/// needs; cryptographic randomness is not, so a hash + counter avoids a
/// dependency.
fn draft_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(n);
    h.write_u128(std::time::UNIX_EPOCH.elapsed().map(|d| d.as_nanos()).unwrap_or(0));
    let a = h.finish();
    h.write_u64(a ^ 0x9E37_79B9_7F4A_7C15);
    let b = h.finish();
    format!(
        "{:08X}-{:04X}-{:04X}-{:04X}-{:012X}",
        (a >> 32) as u32,
        (a >> 16) as u16,
        a as u16,
        (b >> 48) as u16,
        b & 0xFFFF_FFFF_FFFF
    )
}

const US_PER_MS: u64 = 1000;

/// One video-track segment: `[source_start, source_start+len)` of the
/// recording placed at `target_start` on the timeline.
fn segment(material_id: &str, src_start_us: u64, len_us: u64, target_start_us: u64) -> serde_json::Value {
    json!({
        "cartoon": false,
        "clip": {
            "alpha": 1.0,
            "flip": { "horizontal": false, "vertical": false },
            "rotation": 0.0,
            "scale": { "x": 1.0, "y": 1.0 },
            "transform": { "x": 0.0, "y": 0.0 }
        },
        "common_keyframes": [],
        "enable_adjust": true,
        "enable_color_curves": true,
        "enable_color_wheels": true,
        "enable_lut": true,
        "extra_material_refs": [],
        "group_id": "",
        "id": draft_id(),
        "intensifies_audio": false,
        "is_placeholder": false,
        "is_tone_modify": false,
        "keyframe_refs": [],
        "last_nonzero_volume": 1.0,
        "material_id": material_id,
        "render_index": 0,
        "reverse": false,
        "source_timerange": { "duration": len_us, "start": src_start_us },
        "speed": 1.0,
        "target_timerange": { "duration": len_us, "start": target_start_us },
        "template_id": "",
        "template_scene": "default",
        "track_attribute": 0,
        "track_render_index": 0,
        "visible": true,
        "volume": 1.0
    })
}

/// Source-video material entry.
fn video_material(id: &str, path: &Path, duration_us: u64, width: u32, height: u32) -> serde_json::Value {
    json!({
        "audio_fade": null,
        "category_id": "",
        "category_name": "local",
        "check_flag": 63487,
        "crop": {
            "lower_left_x": 0.0, "lower_left_y": 1.0,
            "lower_right_x": 1.0, "lower_right_y": 1.0,
            "upper_left_x": 0.0, "upper_left_y": 0.0,
            "upper_right_x": 1.0, "upper_right_y": 0.0
        },
        "crop_ratio": "free",
        "crop_scale": 1.0,
        "duration": duration_us,
        "extra_type_option": 0,
        "gameplay": { "algorithm": "", "path": "" },
        "has_audio": true,
        "height": height,
        "id": id,
        "intensifies_audio_path": "",
        "intensifies_path": "",
        "is_unified_beauty_mode": false,
        "local_id": "",
        "local_material_id": "",
        "material_id": "",
        "material_name": path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        "material_url": "",
        "matting": { "flag": 0, "has_use_quick_brush": false, "has_use_quick_eraser": false, "interactiveTime": [], "path": "", "strokes": [] },
        "media_path": "",
        "path": path.to_string_lossy(),
        "reverse_intensifies_path": "",
        "reverse_path": "",
        "source_platform": 0,
        "stable": null,
        "team_id": "",
        "type": "video",
        "video_algorithm": { "algorithms": [], "deflicker": null, "motion_blur_config": null, "noise_reduction": null, "path": "", "time_range": null },
        "width": width
    })
}

/// Build `draft_content.json` for a cuts-only project of `highlights`.
pub fn draft_content(
    source: &Path,
    source_duration_ms: u64,
    width: u32,
    height: u32,
    fps: f64,
    highlights: &[Highlight],
) -> serde_json::Value {
    let material_id = draft_id();
    let src_dur_us = source_duration_ms * US_PER_MS;

    let mut segments = Vec::new();
    let mut cursor_us = 0u64;
    for h in highlights {
        let start_us = h.start_ms * US_PER_MS;
        let len_us = h.end_ms.saturating_sub(h.start_ms) * US_PER_MS;
        if len_us == 0 {
            continue;
        }
        segments.push(segment(&material_id, start_us, len_us, cursor_us));
        cursor_us += len_us;
    }

    json!({
        "canvas_config": { "height": height, "ratio": "original", "width": width },
        "color_space": 0,
        "config": {
            "adjust_max_index": 1,
            "attachment_info": [],
            "combination_max_index": 1,
            "export_range": null,
            "extract_audio_last_index": 1,
            "lyrics_recognition_id": "",
            "lyrics_sync": true,
            "lyrics_taskinfo": [],
            "maintrack_adsorb": true,
            "material_save_mode": 0,
            "original_sound_last_index": 1,
            "record_audio_last_index": 1,
            "sticker_max_index": 1,
            "subtitle_recognition_id": "",
            "subtitle_sync": true,
            "subtitle_taskinfo": [],
            "system_font_list": [],
            "video_mute": false,
            "zoom_info_params": null
        },
        "cover": null,
        "create_time": 0,
        "duration": cursor_us,
        "extra_info": null,
        "fps": fps,
        "free_render_index_mode_on": false,
        "group_container": null,
        "id": draft_id(),
        "keyframe_graph_list": [],
        "keyframes": {
            "adjusts": [], "audios": [], "effects": [], "filters": [],
            "handwrites": [], "stickers": [], "texts": [], "videos": []
        },
        "last_modified_platform": {
            "app_id": 3704, "app_source": "lv", "app_version": "5.9.0",
            "device_id": "", "hard_disk_id": "", "mac_address": "",
            "os": "mac", "os_version": ""
        },
        "materials": {
            "ai_translates": [], "audio_balances": [], "audio_effects": [],
            "audio_fades": [], "audio_track_indexes": [], "audios": [],
            "beats": [], "canvases": [], "chromas": [], "color_curves": [],
            "digital_humans": [], "drafts": [], "effects": [], "flowers": [],
            "green_screens": [], "handwrites": [], "hsl": [], "images": [],
            "log_color_wheels": [], "loudnesses": [], "manual_deformations": [],
            "masks": [], "material_animations": [], "material_colors": [],
            "multi_language_refs": [], "placeholders": [], "plugin_effects": [],
            "primary_color_wheels": [], "realtime_denoises": [], "shapes": [],
            "smart_crops": [], "smart_relights": [], "sound_channel_mappings": [],
            "speeds": [], "stickers": [], "tail_leaders": [], "text_templates": [],
            "texts": [], "time_marks": [],
            "transitions": [], "video_effects": [], "video_trackings": [],
            "videos": [ video_material(&material_id, source, src_dur_us, width, height) ],
            "vocal_beautifys": [], "vocal_separations": []
        },
        "mutable_config": null,
        "name": "",
        "new_version": "110.0.0",
        "platform": {
            "app_id": 3704, "app_source": "lv", "app_version": "5.9.0",
            "device_id": "", "hard_disk_id": "", "mac_address": "",
            "os": "mac", "os_version": ""
        },
        "relationships": [],
        "render_index_track_mode_on": false,
        "retouch_cover": null,
        "source": "default",
        "static_cover_image_path": "",
        "time_marks": null,
        "tracks": [{
            "attribute": 0,
            "flag": 0,
            "id": draft_id(),
            "is_default_name": true,
            "name": "",
            "segments": segments,
            "type": "video"
        }],
        "update_time": 0,
        "version": 360000
    })
}

/// Write the draft folder `<out_root>/<name>/` with content + meta files.
/// Returns the draft folder path.
pub fn write_draft(
    out_root: &Path,
    name: &str,
    source: &Path,
    source_duration_ms: u64,
    highlights: &[Highlight],
) -> Result<PathBuf> {
    if highlights.is_empty() {
        return Err(HighlightError::Config("没有可导出的高能片段".into()));
    }
    let dir = out_root.join(name);
    std::fs::create_dir_all(&dir)?;

    let content = draft_content(source, source_duration_ms, 1920, 1080, 30.0, highlights);
    std::fs::write(
        dir.join("draft_content.json"),
        serde_json::to_string(&content)?,
    )?;

    let now_us = std::time::UNIX_EPOCH
        .elapsed()
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0);
    let meta = json!({
        "cloud_package_completed_time": "",
        "draft_cloud_capcut_purchase_info": "",
        "draft_cloud_last_action_download": false,
        "draft_cloud_purchase_info": "",
        "draft_cloud_template_id": "",
        "draft_cloud_tutorial_info": "",
        "draft_cloud_videocut_purchase_info": "",
        "draft_cover": "draft_cover.jpg",
        "draft_deeplink_url": "",
        "draft_enterprise_info": {
            "draft_enterprise_extra": "", "draft_enterprise_id": "",
            "draft_enterprise_name": "", "enterprise_material": []
        },
        "draft_fold_path": dir.to_string_lossy(),
        "draft_id": draft_id(),
        "draft_is_ai_packaging_used": false,
        "draft_is_ai_shorts": false,
        "draft_is_article_video_draft": false,
        "draft_is_from_deeplink": "false",
        "draft_is_invisible": false,
        "draft_materials": [],
        "draft_materials_copied_info": [],
        "draft_name": name,
        "draft_new_version": "",
        "draft_removable_storage_device": "",
        "draft_root_path": out_root.to_string_lossy(),
        "draft_segment_extra_info": [],
        "draft_timeline_materials_size_": 0,
        "draft_type": "",
        "tm_draft_cloud_completed": "",
        "tm_draft_cloud_modified": 0,
        "tm_draft_create": now_us,
        "tm_draft_modified": now_us,
        "tm_draft_removed": 0,
        "tm_duration": content["duration"].as_u64().unwrap_or(0)
    });
    std::fs::write(dir.join("draft_meta_info.json"), serde_json::to_string(&meta)?)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vtb_common::HighlightSignals;

    fn hl(start_ms: u64, end_ms: u64) -> Highlight {
        Highlight {
            start_ms,
            end_ms,
            score: 0.9,
            reason: "test".into(),
            signals: HighlightSignals::default(),
            title: None,
        }
    }

    #[test]
    fn ids_are_unique_and_uuid_shaped() {
        let a = draft_id();
        let b = draft_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(a.chars().filter(|c| *c == '-').count(), 4);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }

    #[test]
    fn content_uses_microseconds_and_contiguous_timeline() {
        let hs = vec![hl(10_000, 20_000), hl(60_000, 65_000)];
        let v = draft_content(Path::new("/rec/a.flv"), 3_600_000, 1920, 1080, 30.0, &hs);

        // Timeline duration = 10s + 5s in µs.
        assert_eq!(v["duration"], 15_000_000u64);
        let segs = v["tracks"][0]["segments"].as_array().unwrap();
        assert_eq!(segs.len(), 2);
        // Segment 1: source 10s→, target 0.
        assert_eq!(segs[0]["source_timerange"]["start"], 10_000_000u64);
        assert_eq!(segs[0]["target_timerange"]["start"], 0u64);
        // Segment 2 appended right after segment 1 on the timeline.
        assert_eq!(segs[1]["target_timerange"]["start"], 10_000_000u64);
        assert_eq!(segs[1]["source_timerange"]["duration"], 5_000_000u64);
        // Segments reference the single source material.
        let mat_id = v["materials"]["videos"][0]["id"].as_str().unwrap();
        assert_eq!(segs[0]["material_id"], mat_id);
        assert_eq!(segs[1]["material_id"], mat_id);
        // Source duration in µs.
        assert_eq!(v["materials"]["videos"][0]["duration"], 3_600_000_000u64);
        assert_eq!(v["materials"]["videos"][0]["path"], "/rec/a.flv");
    }

    #[test]
    fn write_draft_creates_folder_with_both_files() {
        let dir = tempfile::tempdir().unwrap();
        let out = write_draft(
            dir.path(),
            "测试草稿",
            Path::new("/rec/a.flv"),
            600_000,
            &[hl(0, 10_000)],
        )
        .unwrap();
        assert!(out.join("draft_content.json").exists());
        assert!(out.join("draft_meta_info.json").exists());
        let meta: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(out.join("draft_meta_info.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(meta["draft_name"], "测试草稿");
        assert_eq!(meta["tm_duration"], 10_000_000u64);
    }

    #[test]
    fn empty_highlights_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(write_draft(dir.path(), "x", Path::new("/v.flv"), 1000, &[]).is_err());
    }

    #[test]
    fn zero_length_highlights_skipped() {
        let hs = vec![hl(5_000, 5_000), hl(10_000, 12_000)];
        let v = draft_content(Path::new("/v.flv"), 60_000, 1920, 1080, 30.0, &hs);
        assert_eq!(v["tracks"][0]["segments"].as_array().unwrap().len(), 1);
    }
}
