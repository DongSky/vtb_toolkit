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
}
