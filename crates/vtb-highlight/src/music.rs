//! Music / singing segment detection for song-cut clipping (歌切).
//!
//! Pure signal processing over an RMS energy series (as produced by the
//! pipeline's `rms_series`), no audio decoding here. The discriminating
//! feature between speech and singing/music is energy *sustain*: speech has
//! frequent pauses (energy valleys between phrases), while songs keep energy
//! high with few dips.
//!
//! Steps:
//! 1. Adaptive threshold = median RMS of the whole series (speech pauses
//!    and silence pull the median down, so sustained music sits above it).
//! 2. Per [`MusicConfig::window_secs`] window, compute the fraction of
//!    above-threshold frames; windows at or above
//!    [`MusicConfig::active_ratio`] are "music windows".
//! 3. Runs of music windows become candidate segments with frame-accurate
//!    boundaries; candidates separated by ≤ [`MusicConfig::max_gap_secs`]
//!    (song bridges, breaths, short MC) merge into one segment.
//! 4. Segments shorter than [`MusicConfig::min_song_secs`] are discarded.

use serde::{Deserialize, Serialize};
use vtb_common::{Highlight, HighlightSignals};

use crate::signals::n_windows;

/// Tuning for [`detect_music_segments`].
#[derive(Debug, Clone)]
pub struct MusicConfig {
    /// Duration covered by each RMS frame in the input series (ms).
    pub frame_ms: u64,
    /// Segments shorter than this are discarded (secs).
    pub min_song_secs: u64,
    /// Maximum gap bridged between neighbouring music regions (secs).
    pub max_gap_secs: u64,
    /// Minimum fraction of above-threshold frames for a music window.
    pub active_ratio: f64,
    /// Analysis window length (secs).
    pub window_secs: u64,
}

impl Default for MusicConfig {
    fn default() -> Self {
        Self {
            frame_ms: 500,
            min_song_secs: 60,
            max_gap_secs: 8,
            active_ratio: 0.75,
            window_secs: 10,
        }
    }
}

/// A detected song / music passage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MusicSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    /// Mean RMS over the segment (internal gaps included).
    pub mean_energy: f64,
}

/// Detect sustained music/singing segments in an `(offset_ms, rms)` series.
///
/// The series is typically uniform frames from `rms_series`, but any sample
/// spacing works; boundaries are aligned to actual frame offsets.
pub fn detect_music_segments(rms: &[(u64, f64)], cfg: &MusicConfig) -> Vec<MusicSegment> {
    let window_ms = cfg.window_secs * 1000;
    if rms.is_empty() || window_ms == 0 {
        return vec![];
    }

    // Frame indices sorted by offset (rms_series already emits them sorted;
    // sorting keeps the math correct for any caller).
    let mut order: Vec<usize> = (0..rms.len()).collect();
    order.sort_by_key(|&i| rms[i].0);

    // a) Adaptive threshold: whole-series median RMS.
    let mut sorted_vals: Vec<f64> = rms.iter().map(|&(_, v)| v).collect();
    sorted_vals.sort_by(f64::total_cmp);
    let mid = sorted_vals.len() / 2;
    let threshold = if sorted_vals.len() % 2 == 1 {
        sorted_vals[mid]
    } else {
        (sorted_vals[mid - 1] + sorted_vals[mid]) / 2.0
    };
    let active = |i: usize| rms[i].1 > threshold;

    // b) Per-window active fraction.
    let total_ms = rms[*order.last().unwrap()].0 + cfg.frame_ms;
    let n = n_windows(total_ms, window_ms);
    let mut totals = vec![0u32; n];
    let mut actives = vec![0u32; n];
    for &i in &order {
        let w = (rms[i].0 / window_ms) as usize;
        if w < n {
            totals[w] += 1;
            if active(i) {
                actives[w] += 1;
            }
        }
    }
    let is_music: Vec<bool> = (0..n)
        .map(|w| totals[w] > 0 && actives[w] as f64 / totals[w] as f64 >= cfg.active_ratio)
        .collect();

    // c) Runs of consecutive music windows → frame-aligned candidates.
    //    Boundaries snap to actual frames: first extend outward over
    //    contiguous active frames spilling into neighbour windows (a short
    //    in-song gap can drag a whole window below `active_ratio`; its
    //    active halves still belong to the song), then trim inactive edges.
    let mut candidates: Vec<(u64, u64)> = vec![];
    let mut w = 0;
    while w < n {
        if !is_music[w] {
            w += 1;
            continue;
        }
        let run_start = w;
        while w < n && is_music[w] {
            w += 1;
        }
        let lo_ms = run_start as u64 * window_ms;
        let hi_ms = w as u64 * window_ms;
        let mut p = order.partition_point(|&i| rms[i].0 < lo_ms);
        let mut q = order.partition_point(|&i| rms[i].0 < hi_ms);
        while p > 0 && active(order[p - 1]) {
            p -= 1;
        }
        while q < order.len() && active(order[q]) {
            q += 1;
        }
        while p < q && !active(order[p]) {
            p += 1;
        }
        while q > p && !active(order[q - 1]) {
            q -= 1;
        }
        if p < q {
            candidates.push((rms[order[p]].0, rms[order[q - 1]].0 + cfg.frame_ms));
        }
    }

    // Merge candidates separated by ≤ max_gap_secs.
    let max_gap_ms = cfg.max_gap_secs * 1000;
    let mut merged: Vec<(u64, u64)> = vec![];
    for (s, e) in candidates {
        match merged.last_mut() {
            Some((_, prev_end)) if s <= prev_end.saturating_add(max_gap_ms) => {
                *prev_end = (*prev_end).max(e);
            }
            _ => merged.push((s, e)),
        }
    }

    // d) Minimum song length, e) mean energy over the segment.
    merged
        .into_iter()
        .filter(|(s, e)| e - s >= cfg.min_song_secs * 1000)
        .map(|(s, e)| {
            let p = order.partition_point(|&i| rms[i].0 < s);
            let q = order.partition_point(|&i| rms[i].0 < e);
            let count = (q - p).max(1) as f64;
            let mean_energy = order[p..q].iter().map(|&i| rms[i].1).sum::<f64>() / count;
            MusicSegment {
                start_ms: s,
                end_ms: e,
                mean_energy,
            }
        })
        .collect()
}

/// Convert detected song segments into [`Highlight`]s so the existing clip
/// cutter (`clip::cut_clip`) can export them directly.
pub fn song_clip_highlights(segments: &[MusicSegment]) -> Vec<Highlight> {
    segments
        .iter()
        .map(|seg| Highlight {
            start_ms: seg.start_ms,
            end_ms: seg.end_ms,
            score: 0.8,
            reason: "歌曲段落".into(),
            signals: HighlightSignals {
                audio_energy: Some(seg.mean_energy),
                ..Default::default()
            },
            title: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Append one 500 ms frame with the given RMS.
    fn frame(series: &mut Vec<(u64, f64)>, rms: f64) {
        let offset = series.len() as u64 * 500;
        series.push((offset, rms));
    }

    /// Append `secs` seconds of constant-RMS frames.
    fn push_const(series: &mut Vec<(u64, f64)>, secs: u64, rms: f64) {
        for _ in 0..secs * 2 {
            frame(series, rms);
        }
    }

    /// Append `secs` seconds of speech: short pulses with frequent energy
    /// valleys (alternating 0.3 / 0.05).
    fn push_speech(series: &mut Vec<(u64, f64)>, secs: u64) {
        for k in 0..secs * 2 {
            frame(series, if k % 2 == 0 { 0.3 } else { 0.05 });
        }
    }

    #[test]
    fn default_config_values() {
        let cfg = MusicConfig::default();
        assert_eq!(cfg.frame_ms, 500);
        assert_eq!(cfg.min_song_secs, 60);
        assert_eq!(cfg.max_gap_secs, 8);
        assert!((cfg.active_ratio - 0.75).abs() < 1e-12);
        assert_eq!(cfg.window_secs, 10);
    }

    #[test]
    fn pure_speech_yields_no_segments() {
        // 300 s of talk: pulses above the median but valleys every other
        // frame keep every window's active ratio at 0.5 < 0.75.
        let mut s = vec![];
        push_speech(&mut s, 300);
        let segs = detect_music_segments(&s, &MusicConfig::default());
        assert!(segs.is_empty(), "speech misdetected as music: {segs:?}");
    }

    #[test]
    fn one_song_with_small_gaps_is_bridged() {
        // 60 s speech | 120 s song at 0.8 with three ≤5 s gaps | 60 s speech.
        let mut s = vec![];
        push_speech(&mut s, 60); // 0–60 s
        push_const(&mut s, 30, 0.8); // 60–90 s
        push_const(&mut s, 4, 0.0); // 90–94 s gap
        push_const(&mut s, 26, 0.8); // 94–120 s
        push_const(&mut s, 5, 0.0); // 120–125 s gap
        push_const(&mut s, 25, 0.8); // 125–150 s
        push_const(&mut s, 3, 0.0); // 150–153 s gap
        push_const(&mut s, 27, 0.8); // 153–180 s
        push_speech(&mut s, 60); // 180–240 s

        let cfg = MusicConfig::default();
        let segs = detect_music_segments(&s, &cfg);
        assert_eq!(segs.len(), 1, "expected one bridged song: {segs:?}");
        let win_ms = cfg.window_secs * 1000;
        assert!(segs[0].start_ms.abs_diff(60_000) <= win_ms);
        assert!(segs[0].end_ms.abs_diff(180_000) <= win_ms);
        // All three gaps bridged into one continuous segment.
        assert!(segs[0].end_ms - segs[0].start_ms >= 110_000);
        // 108 s at 0.8 + 12 s at 0.0 over 120 s → 0.72.
        assert!((segs[0].mean_energy - 0.72).abs() < 1e-6);
    }

    #[test]
    fn two_songs_separated_by_talk() {
        // Enough surrounding speech keeps the median at talk level.
        let mut s = vec![];
        push_speech(&mut s, 120); // 0–120 s
        push_const(&mut s, 90, 0.8); // 120–210 s: song A
        push_speech(&mut s, 60); // 210–270 s: 30+ s talk gap (> max_gap)
        push_const(&mut s, 90, 0.8); // 270–360 s: song B
        push_speech(&mut s, 120); // 360–480 s

        let cfg = MusicConfig::default();
        let segs = detect_music_segments(&s, &cfg);
        assert_eq!(segs.len(), 2, "expected two songs: {segs:?}");
        let win_ms = cfg.window_secs * 1000;
        assert!(segs[0].start_ms.abs_diff(120_000) <= win_ms);
        assert!(segs[0].end_ms.abs_diff(210_000) <= win_ms);
        assert!(segs[1].start_ms.abs_diff(270_000) <= win_ms);
        assert!(segs[1].end_ms.abs_diff(360_000) <= win_ms);
        assert!((segs[0].mean_energy - 0.8).abs() < 1e-6);
        assert!((segs[1].mean_energy - 0.8).abs() < 1e-6);
    }

    #[test]
    fn short_high_energy_burst_is_dropped() {
        // 55 s of sustained energy < min_song_secs (60) → no segment.
        let mut s = vec![];
        push_speech(&mut s, 120); // 0–120 s
        push_const(&mut s, 55, 0.8); // 120–175 s
        push_speech(&mut s, 120); // 175–295 s
        let segs = detect_music_segments(&s, &MusicConfig::default());
        assert!(segs.is_empty(), "55 s burst should be dropped: {segs:?}");
    }

    #[test]
    fn silence_and_empty_produce_nothing() {
        let cfg = MusicConfig::default();
        assert!(detect_music_segments(&[], &cfg).is_empty());
        let mut s = vec![];
        push_const(&mut s, 120, 0.0);
        assert!(detect_music_segments(&s, &cfg).is_empty());
    }

    #[test]
    fn song_clip_highlights_maps_fields() {
        let seg = MusicSegment {
            start_ms: 5_000,
            end_ms: 95_000,
            mean_energy: 0.42,
        };
        let hs = song_clip_highlights(&[seg]);
        assert_eq!(hs.len(), 1);
        let h = &hs[0];
        assert_eq!(h.start_ms, 5_000);
        assert_eq!(h.end_ms, 95_000);
        assert!((h.score - 0.8).abs() < 1e-12);
        assert_eq!(h.reason, "歌曲段落");
        assert_eq!(h.title, None);
        assert_eq!(h.signals.audio_energy, Some(0.42));
        assert_eq!(h.signals.danmaku_density, None);
        assert_eq!(h.signals.danmaku_sentiment, None);
        assert_eq!(h.signals.gift_value, None);
        assert_eq!(h.signals.multimodal, None);
    }
}
