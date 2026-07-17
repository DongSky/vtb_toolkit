//! Subtitle export: SRT and ASS, mono- or bilingual.

use vtb_common::{TranscriptSegment, TranslatedSegment};

/// Format milliseconds as an SRT timestamp `HH:MM:SS,mmm`.
pub fn srt_timestamp(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1000;
    let milli = ms % 1000;
    format!("{h:02}:{m:02}:{s:02},{milli:03}")
}

/// Format milliseconds as an ASS timestamp `H:MM:SS.cc`.
pub fn ass_timestamp(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1000;
    let cs = (ms % 1000) / 10;
    format!("{h}:{m:02}:{s:02}.{cs:02}")
}

/// Build an SRT file from transcript segments.
pub fn to_srt(segments: &[TranscriptSegment]) -> String {
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            srt_timestamp(seg.start_ms),
            srt_timestamp(seg.end_ms),
            seg.text.trim()
        ));
    }
    out
}

/// Build a bilingual SRT: translation on the first line, source below.
pub fn to_srt_bilingual(segments: &[TranslatedSegment]) -> String {
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n{}\n\n",
            i + 1,
            srt_timestamp(seg.start_ms),
            srt_timestamp(seg.end_ms),
            seg.translated_text.trim(),
            seg.source_text.trim()
        ));
    }
    out
}

const ASS_HEADER: &str = "\
[Script Info]\n\
Title: vtb-toolkit subtitles\n\
ScriptType: v4.00+\n\
WrapStyle: 0\n\
ScaledBorderAndShadow: yes\n\
PlayResX: 1920\n\
PlayResY: 1080\n\
\n\
[V4+ Styles]\n\
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
Style: CN,Noto Sans CJK SC,64,&H00FFFFFF,&H000000FF,&H00000000,&H7F000000,-1,0,0,0,100,100,0,0,1,3,1,2,30,30,40,1\n\
Style: JP,Noto Sans CJK JP,44,&H00B4E1FF,&H000000FF,&H00000000,&H7F000000,0,0,0,0,100,100,0,0,1,3,1,2,30,30,110,1\n\
\n\
[Events]\n\
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";

/// Escape braces/newlines for ASS dialogue text.
fn ass_escape(text: &str) -> String {
    text.replace('{', "(").replace('}', ")").replace('\n', "\\N")
}

/// Build a bilingual ASS: big translated line (style CN) + smaller source
/// line (style JP).
pub fn to_ass_bilingual(segments: &[TranslatedSegment]) -> String {
    let mut out = String::from(ASS_HEADER);
    for seg in segments {
        let start = ass_timestamp(seg.start_ms);
        let end = ass_timestamp(seg.end_ms);
        out.push_str(&format!(
            "Dialogue: 0,{start},{end},CN,,0,0,0,,{}\n",
            ass_escape(seg.translated_text.trim())
        ));
        out.push_str(&format!(
            "Dialogue: 0,{start},{end},JP,,0,0,0,,{}\n",
            ass_escape(seg.source_text.trim())
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tseg(start: u64, end: u64, text: &str) -> TranscriptSegment {
        TranscriptSegment {
            start_ms: start,
            end_ms: end,
            text: text.into(),
            lang: Some("ja".into()),
            is_final: true,
        }
    }

    fn xseg(start: u64, end: u64, src: &str, dst: &str) -> TranslatedSegment {
        TranslatedSegment {
            start_ms: start,
            end_ms: end,
            source_text: src.into(),
            translated_text: dst.into(),
            source_lang: Some("ja".into()),
            target_lang: "zh".into(),
        }
    }

    #[test]
    fn srt_timestamps() {
        assert_eq!(srt_timestamp(0), "00:00:00,000");
        assert_eq!(srt_timestamp(3_723_456), "01:02:03,456");
    }

    #[test]
    fn ass_timestamps() {
        assert_eq!(ass_timestamp(0), "0:00:00.00");
        assert_eq!(ass_timestamp(3_723_456), "1:02:03.45");
    }

    #[test]
    fn srt_format() {
        let srt = to_srt(&[tseg(0, 2000, "こんにちは"), tseg(2500, 5000, "元気？")]);
        let expected = "1\n00:00:00,000 --> 00:00:02,000\nこんにちは\n\n2\n00:00:02,500 --> 00:00:05,000\n元気？\n\n";
        assert_eq!(srt, expected);
    }

    #[test]
    fn bilingual_srt_puts_translation_first() {
        let srt = to_srt_bilingual(&[xseg(0, 1000, "原文", "译文")]);
        let block: Vec<&str> = srt.lines().collect();
        assert_eq!(block[2], "译文");
        assert_eq!(block[3], "原文");
    }

    #[test]
    fn ass_contains_styles_and_dialogue() {
        let ass = to_ass_bilingual(&[xseg(1000, 3000, "日本語", "中文")]);
        assert!(ass.contains("[V4+ Styles]"));
        assert!(ass.contains("Style: CN,"));
        assert!(ass.contains("Dialogue: 0,0:00:01.00,0:00:03.00,CN,,0,0,0,,中文"));
        assert!(ass.contains("Dialogue: 0,0:00:01.00,0:00:03.00,JP,,0,0,0,,日本語"));
    }

    #[test]
    fn ass_escaping() {
        let ass = to_ass_bilingual(&[xseg(0, 1000, "a{b}c", "多\n行")]);
        assert!(ass.contains("a(b)c"));
        assert!(ass.contains("多\\N行"));
    }

    #[test]
    fn empty_input_yields_header_only() {
        assert_eq!(to_srt(&[]), "");
        let ass = to_ass_bilingual(&[]);
        assert!(ass.ends_with("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"));
    }
}
