//! Signal fusion: combine per-window scores into merged highlights.

use crate::signals::WindowSeries;
use vtb_common::{Highlight, HighlightSignals};

#[derive(Debug, Clone)]
pub struct FusionConfig {
    /// Weight of danmaku density z-score.
    pub w_density: f64,
    /// Weight of emotion keyword z-score.
    pub w_keyword: f64,
    /// Weight of gift value z-score.
    pub w_gift: f64,
    /// Weight of audio energy z-score.
    pub w_audio: f64,
    /// Combined z-score threshold for a window to be "hot".
    pub threshold: f64,
    /// Merge hot windows separated by gaps ≤ this many windows.
    pub merge_gap: usize,
    /// Pad each highlight by this many ms on both sides (context lead-in).
    pub pad_ms: u64,
    /// Discard merged highlights shorter than this.
    pub min_len_ms: u64,
}

impl Default for FusionConfig {
    fn default() -> Self {
        Self {
            w_density: 1.0,
            w_keyword: 0.8,
            w_gift: 0.6,
            w_audio: 0.5,
            threshold: 1.5,
            merge_gap: 1,
            pad_ms: 5_000,
            min_len_ms: 10_000,
        }
    }
}

/// Inputs to fusion; any series may be `None` (signal unavailable).
#[derive(Debug, Default)]
pub struct SignalSet {
    pub density: Option<WindowSeries>,
    pub keyword: Option<WindowSeries>,
    pub gift: Option<WindowSeries>,
    pub audio: Option<WindowSeries>,
}

/// Detect highlights from available signals.
///
/// All provided series must share `window_ms` and length; the shortest
/// length is used. Returns highlights sorted by start time.
pub fn detect_highlights(
    signals: &SignalSet,
    config: &FusionConfig,
    total_ms: u64,
) -> Vec<Highlight> {
    let series: Vec<(&WindowSeries, f64, &str)> = [
        (signals.density.as_ref(), config.w_density, "弹幕密度"),
        (signals.keyword.as_ref(), config.w_keyword, "弹幕情绪"),
        (signals.gift.as_ref(), config.w_gift, "礼物/SC"),
        (signals.audio.as_ref(), config.w_audio, "音频能量"),
    ]
    .into_iter()
    .filter_map(|(s, w, name)| s.map(|s| (s, w, name)))
    .collect();

    if series.is_empty() {
        return vec![];
    }
    let window_ms = series[0].0.window_ms;
    let n = series.iter().map(|(s, _, _)| s.values.len()).min().unwrap();
    if n == 0 {
        return vec![];
    }

    // Per-series z-scores.
    let zs: Vec<(Vec<f64>, f64, &str)> = series
        .iter()
        .map(|(s, w, name)| (s.zscores(), *w, *name))
        .collect();

    // Combined score per window + which signals crossed 1σ (for reasons).
    let mut combined = vec![0.0f64; n];
    let mut contributors: Vec<Vec<&str>> = vec![vec![]; n];
    let mut raw_signals: Vec<HighlightSignals> = vec![HighlightSignals::default(); n];
    for (z, w, name) in &zs {
        for i in 0..n {
            combined[i] += z[i] * w;
            if z[i] >= 1.0 {
                contributors[i].push(name);
            }
            match *name {
                "弹幕密度" => raw_signals[i].danmaku_density = Some(z[i]),
                "弹幕情绪" => raw_signals[i].danmaku_sentiment = Some(z[i]),
                "礼物/SC" => raw_signals[i].gift_value = Some(z[i]),
                "音频能量" => raw_signals[i].audio_energy = Some(z[i]),
                _ => {}
            }
        }
    }

    // Hot windows above threshold.
    let hot: Vec<usize> = (0..n).filter(|&i| combined[i] >= config.threshold).collect();
    if hot.is_empty() {
        return vec![];
    }

    // Merge runs with small gaps.
    let mut groups: Vec<(usize, usize)> = Vec::new(); // inclusive ranges
    let (mut start, mut end) = (hot[0], hot[0]);
    for &i in &hot[1..] {
        if i - end <= config.merge_gap + 1 {
            end = i;
        } else {
            groups.push((start, end));
            start = i;
            end = i;
        }
    }
    groups.push((start, end));

    // Build highlights.
    let max_combined = combined.iter().cloned().fold(f64::MIN, f64::max);
    groups
        .into_iter()
        .filter_map(|(a, b)| {
            let raw_start = (a as u64) * window_ms;
            let raw_end = ((b + 1) as u64) * window_ms;
            let start_ms = raw_start.saturating_sub(config.pad_ms);
            let end_ms = (raw_end + config.pad_ms).min(total_ms);
            if end_ms.saturating_sub(start_ms) < config.min_len_ms {
                return None;
            }
            // Peak window inside the group.
            let peak = (a..=b)
                .max_by(|&x, &y| combined[x].partial_cmp(&combined[y]).unwrap())
                .unwrap();
            let score = if max_combined > 0.0 {
                (combined[peak] / max_combined).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut reasons = contributors[peak].clone();
            reasons.dedup();
            let reason = if reasons.is_empty() {
                "综合信号峰值".to_string()
            } else {
                format!("{}峰值", reasons.join(" + "))
            };
            Some(Highlight {
                start_ms,
                end_ms,
                score,
                reason,
                signals: raw_signals[peak].clone(),
                title: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(values: Vec<f64>) -> WindowSeries {
        WindowSeries {
            window_ms: 10_000,
            values,
        }
    }

    fn cfg() -> FusionConfig {
        FusionConfig {
            pad_ms: 0,
            min_len_ms: 0,
            ..Default::default()
        }
    }

    #[test]
    fn no_signals_no_highlights() {
        let out = detect_highlights(&SignalSet::default(), &cfg(), 100_000);
        assert!(out.is_empty());
    }

    #[test]
    fn flat_signal_no_highlights() {
        let s = SignalSet {
            density: Some(series(vec![3.0; 10])),
            ..Default::default()
        };
        assert!(detect_highlights(&s, &cfg(), 100_000).is_empty());
    }

    #[test]
    fn single_peak_detected_with_correct_bounds() {
        // Window 5 (50s-60s) is a big spike.
        let mut v = vec![2.0; 10];
        v[5] = 50.0;
        let s = SignalSet {
            density: Some(series(v)),
            ..Default::default()
        };
        let out = detect_highlights(&s, &cfg(), 100_000);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].start_ms, 50_000);
        assert_eq!(out[0].end_ms, 60_000);
        assert!(out[0].score > 0.99);
        assert!(out[0].reason.contains("弹幕密度"));
    }

    #[test]
    fn adjacent_hot_windows_merge() {
        let mut v = vec![1.0; 12];
        v[4] = 30.0;
        v[5] = 28.0;
        v[7] = 29.0; // gap of 1 window (6) — should merge with merge_gap=1
        let s = SignalSet {
            density: Some(series(v)),
            ..Default::default()
        };
        let out = detect_highlights(&s, &cfg(), 120_000);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].start_ms, 40_000);
        assert_eq!(out[0].end_ms, 80_000);
    }

    #[test]
    fn distant_peaks_stay_separate() {
        let mut v = vec![1.0; 20];
        v[2] = 40.0;
        v[15] = 42.0;
        let s = SignalSet {
            density: Some(series(v)),
            ..Default::default()
        };
        let out = detect_highlights(&s, &cfg(), 200_000);
        assert_eq!(out.len(), 2);
        assert!(out[0].start_ms < out[1].start_ms);
    }

    #[test]
    fn padding_and_min_length_apply() {
        let mut v = vec![1.0; 10];
        v[5] = 50.0;
        let s = SignalSet {
            density: Some(series(v)),
            ..Default::default()
        };
        let mut config = cfg();
        config.pad_ms = 5_000;
        let out = detect_highlights(&s, &config, 100_000);
        assert_eq!(out[0].start_ms, 45_000);
        assert_eq!(out[0].end_ms, 65_000);

        config.min_len_ms = 60_000; // longer than the padded window
        assert!(detect_highlights(&s, &config, 100_000).is_empty());
    }

    #[test]
    fn multi_signal_fusion_combines() {
        // Density alone is a mild bump; audio pushes window 3 over threshold.
        let mut d = vec![2.0; 10];
        d[3] = 6.0;
        let mut a = vec![0.1; 10];
        a[3] = 0.9;
        let s = SignalSet {
            density: Some(series(d)),
            audio: Some(series(a)),
            ..Default::default()
        };
        let out = detect_highlights(&s, &cfg(), 100_000);
        assert_eq!(out.len(), 1);
        assert!(out[0].reason.contains("弹幕密度"));
        assert!(out[0].reason.contains("音频能量"));
        assert!(out[0].signals.audio_energy.unwrap() > 1.0);
    }

    #[test]
    fn end_clamped_to_total() {
        let mut v = vec![1.0; 5];
        v[4] = 30.0;
        let s = SignalSet {
            density: Some(series(v)),
            ..Default::default()
        };
        let mut config = cfg();
        config.pad_ms = 20_000;
        let out = detect_highlights(&s, &config, 50_000);
        assert_eq!(out[0].end_ms, 50_000);
    }
}
