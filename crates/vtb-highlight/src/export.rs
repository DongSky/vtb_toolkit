//! NLE project export: turn detected highlights into a CMX3600 EDL that
//! Premiere / DaVinci Resolve / Final Cut import directly — the 切片man
//! opens the recording with every 高能片段 already marked on the timeline.

use vtb_common::Highlight;

/// Frames-per-second used for timecode math. B站 live streams are
/// commonly 30 fps; EDL timecodes only need to be close enough for edit
/// points (frame-exact trimming happens in the NLE anyway).
pub const DEFAULT_FPS: u32 = 30;

/// `HH:MM:SS:FF` timecode at `fps` for a millisecond offset.
pub fn timecode(ms: u64, fps: u32) -> String {
    let total_secs = ms / 1000;
    let frames = (ms % 1000) * u64::from(fps) / 1000;
    format!(
        "{:02}:{:02}:{:02}:{:02}",
        total_secs / 3600,
        (total_secs / 60) % 60,
        total_secs % 60,
        frames
    )
}

/// Build a CMX3600 EDL: one event per highlight, source timecode = record
/// timecode (a cuts-only conform of the original recording), clip-name
/// comments carrying the source file and the highlight title.
pub fn edl_from_highlights(
    title: &str,
    source_name: &str,
    highlights: &[Highlight],
    fps: u32,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("TITLE: {title}\n"));
    out.push_str("FCM: NON-DROP FRAME\n\n");

    let mut record_ms: u64 = 0;
    for (i, h) in highlights.iter().enumerate() {
        let dur = h.end_ms.saturating_sub(h.start_ms);
        let src_in = timecode(h.start_ms, fps);
        let src_out = timecode(h.end_ms, fps);
        let rec_in = timecode(record_ms, fps);
        let rec_out = timecode(record_ms + dur, fps);
        record_ms += dur;

        out.push_str(&format!(
            "{:03}  AX       AA/V  C        {src_in} {src_out} {rec_in} {rec_out}\n",
            i + 1
        ));
        out.push_str(&format!("* FROM CLIP NAME: {source_name}\n"));
        let label = h.title.as_deref().unwrap_or(&h.reason);
        if !label.is_empty() {
            out.push_str(&format!("* COMMENT: {} (score {:.2})\n", label, h.score));
        }
        out.push('\n');
    }
    out
}

/// Escape a string for XML attribute/text content.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Frame-aligned FCPXML rational time (`N*100/fps*100s`) for a
/// millisecond offset. FCP requires times to be multiples of the format's
/// frameDuration; Resolve/Premiere are lenient but accept this too.
fn fcpxml_time(ms: u64, fps: u32) -> String {
    let frames = (ms * u64::from(fps) + 500) / 1000;
    format!("{}/{}s", frames * 100, u64::from(fps) * 100)
}

/// A point to surface as an NLE marker: `(offset_ms, label)`.
pub type MarkerPoint = (u64, String);

/// Build an FCPXML 1.9 document: the full recording as one clip with a
/// marker at every 高能片段 start and every 打点 — the clipper opens the
/// whole VOD in Resolve/FCP/Premiere with the interesting moments already
/// pinned on the timeline (the direct fix for the "时间戳" pain point).
pub fn fcpxml_with_markers(
    title: &str,
    source_path: &std::path::Path,
    total_ms: u64,
    fps: u32,
    highlights: &[Highlight],
    extra_markers: &[MarkerPoint],
) -> String {
    let name = source_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "recording".into());
    let src_url = format!("file://{}", source_path.to_string_lossy());
    let dur = fcpxml_time(total_ms.max(1000), fps);
    let frame = fcpxml_time(1000 / u64::from(fps).max(1), fps);

    let mut markers: Vec<MarkerPoint> = extra_markers.to_vec();
    for h in highlights {
        let label = h.title.clone().unwrap_or_else(|| h.reason.clone());
        markers.push((h.start_ms, format!("{} (score {:.2})", label, h.score)));
    }
    markers.sort_by_key(|(ms, _)| *ms);
    markers.retain(|(ms, _)| *ms < total_ms);

    let mut marker_xml = String::new();
    for (ms, label) in &markers {
        marker_xml.push_str(&format!(
            "            <marker start=\"{}\" duration=\"{frame}\" value=\"{}\"/>\n",
            fcpxml_time(*ms, fps),
            xml_escape(label)
        ));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE fcpxml>
<fcpxml version="1.9">
  <resources>
    <format id="r1" name="FFVideoFormat1080p{fps}" frameDuration="100/{fd}s" width="1920" height="1080"/>
    <asset id="r2" name="{name_esc}" start="0s" duration="{dur}" hasVideo="1" hasAudio="1" format="r1">
      <media-rep kind="original-media" src="{src_esc}"/>
    </asset>
  </resources>
  <library>
    <event name="{title_esc}">
      <project name="{title_esc} 高能标记">
        <sequence format="r1" duration="{dur}" tcStart="0s">
          <spine>
            <asset-clip ref="r2" offset="0s" start="0s" duration="{dur}" name="{name_esc}" format="r1">
{marker_xml}            </asset-clip>
          </spine>
        </sequence>
      </project>
    </event>
  </library>
</fcpxml>
"#,
        fps = fps,
        fd = u64::from(fps) * 100,
        name_esc = xml_escape(&name),
        src_esc = xml_escape(&src_url),
        title_esc = xml_escape(title),
        dur = dur,
        marker_xml = marker_xml,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use vtb_common::HighlightSignals;

    fn hl(start_ms: u64, end_ms: u64, title: Option<&str>) -> Highlight {
        Highlight {
            start_ms,
            end_ms,
            score: 0.9,
            reason: "弹幕密度峰值".into(),
            signals: HighlightSignals::default(),
            title: title.map(String::from),
        }
    }

    #[test]
    fn timecode_math() {
        assert_eq!(timecode(0, 30), "00:00:00:00");
        assert_eq!(timecode(1_000, 30), "00:00:01:00");
        assert_eq!(timecode(1_500, 30), "00:00:01:15");
        assert_eq!(timecode(3_661_000, 30), "01:01:01:00");
        assert_eq!(timecode(999, 30), "00:00:00:29");
    }

    #[test]
    fn edl_has_header_events_and_contiguous_record_track() {
        let hs = vec![
            hl(10_000, 20_000, Some("名场面")),
            hl(60_000, 65_000, None),
        ];
        let edl = edl_from_highlights("session", "rec.flv", &hs, 30);

        assert!(edl.starts_with("TITLE: session\nFCM: NON-DROP FRAME\n"));
        // Event 1: source 10s→20s lands at record 0s→10s.
        assert!(edl.contains(
            "001  AX       AA/V  C        00:00:10:00 00:00:20:00 00:00:00:00 00:00:10:00"
        ));
        // Event 2: source 60s→65s appended at record 10s→15s.
        assert!(edl.contains(
            "002  AX       AA/V  C        00:01:00:00 00:01:05:00 00:00:10:00 00:00:15:00"
        ));
        assert!(edl.contains("* FROM CLIP NAME: rec.flv"));
        assert!(edl.contains("* COMMENT: 名场面 (score 0.90)"));
        assert!(edl.contains("* COMMENT: 弹幕密度峰值 (score 0.90)"));
    }

    #[test]
    fn empty_highlights_yield_header_only() {
        let edl = edl_from_highlights("t", "v.flv", &[], 30);
        assert!(edl.contains("TITLE: t"));
        assert!(!edl.contains("001"));
    }

    #[test]
    fn fcpxml_time_is_frame_aligned() {
        assert_eq!(fcpxml_time(0, 30), "0/3000s");
        assert_eq!(fcpxml_time(1000, 30), "3000/3000s");
        // 500ms at 30fps = 15 frames.
        assert_eq!(fcpxml_time(500, 30), "1500/3000s");
        // Rounds to the nearest frame (33ms → 1 frame).
        assert_eq!(fcpxml_time(33, 30), "100/3000s");
    }

    #[test]
    fn fcpxml_has_asset_and_sorted_markers() {
        let hs = vec![hl(60_000, 70_000, Some("名场面"))];
        let extra = vec![(10_000u64, "打点: 开场".to_string())];
        let xml = fcpxml_with_markers(
            "session",
            std::path::Path::new("/rec/room1/rec.flv"),
            3_600_000,
            30,
            &hs,
            &extra,
        );
        assert!(xml.contains("<fcpxml version=\"1.9\">"));
        assert!(xml.contains("src=\"file:///rec/room1/rec.flv\""));
        // Both markers present, 打点 first (10s < 60s).
        let m1 = xml.find("打点: 开场").unwrap();
        let m2 = xml.find("名场面").unwrap();
        assert!(m1 < m2);
        // Frame-aligned start for the 60s highlight.
        assert!(xml.contains("start=\"180000/3000s\""));
    }

    #[test]
    fn fcpxml_escapes_labels_and_drops_out_of_range() {
        let extra = vec![
            (1_000u64, "A & B <爆笑>".to_string()),
            (99_000_000u64, "越界".to_string()),
        ];
        let xml = fcpxml_with_markers(
            "t",
            std::path::Path::new("/v.flv"),
            60_000,
            30,
            &[],
            &extra,
        );
        assert!(xml.contains("A &amp; B &lt;爆笑&gt;"));
        assert!(!xml.contains("越界"));
    }
}
