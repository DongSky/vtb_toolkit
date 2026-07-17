//! Per-signal scoring over fixed time windows.

use vtb_common::LiveEvent;

/// A per-window score series. Window `i` covers
/// `[i*window_ms, (i+1)*window_ms)` relative to the session start.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowSeries {
    pub window_ms: u64,
    pub values: Vec<f64>,
}

impl WindowSeries {
    pub fn new(window_ms: u64, n: usize) -> Self {
        Self {
            window_ms,
            values: vec![0.0; n],
        }
    }

    /// Normalize to z-scores (mean 0, std 1). All-equal series → all zeros.
    pub fn zscores(&self) -> Vec<f64> {
        let n = self.values.len() as f64;
        if n == 0.0 {
            return vec![];
        }
        let mean = self.values.iter().sum::<f64>() / n;
        let var = self
            .values
            .iter()
            .map(|v| (v - mean).powi(2))
            .sum::<f64>()
            / n;
        let std = var.sqrt();
        if std < 1e-12 {
            return vec![0.0; self.values.len()];
        }
        self.values.iter().map(|v| (v - mean) / std).collect()
    }
}

fn window_index(offset_ms: u64, window_ms: u64, n: usize) -> Option<usize> {
    let idx = (offset_ms / window_ms) as usize;
    (idx < n).then_some(idx)
}

/// Count danmaku messages per window. `events` carry offsets in ms from
/// stream start (pre-computed by the caller from timestamps).
pub fn danmaku_density(
    events: &[(u64, &LiveEvent)],
    window_ms: u64,
    total_ms: u64,
) -> WindowSeries {
    let n = n_windows(total_ms, window_ms);
    let mut series = WindowSeries::new(window_ms, n);
    for (offset, ev) in events {
        if matches!(ev, LiveEvent::Danmaku(_)) {
            if let Some(i) = window_index(*offset, window_ms, n) {
                series.values[i] += 1.0;
            }
        }
    }
    series
}

/// Emotion keywords that mark funny / hype / shock moments in VTuber chat.
pub const EMOTION_KEYWORDS: &[&str] = &[
    "草", "哈哈", "hhh", "www", "ｗｗ", "笑死", "太强", "太强了", "神",
    "牛", "！！", "？？", "!!", "??", "泪目", "呜呜", "可爱", "kawaii",
    "きた", "やば", "すご", "888", "666", "名场面", "切了", "剪了",
];

/// Score emotion keyword occurrences per window (each hit = 1).
pub fn keyword_score(
    events: &[(u64, &LiveEvent)],
    window_ms: u64,
    total_ms: u64,
) -> WindowSeries {
    let n = n_windows(total_ms, window_ms);
    let mut series = WindowSeries::new(window_ms, n);
    for (offset, ev) in events {
        if let LiveEvent::Danmaku(d) = ev {
            let hits = EMOTION_KEYWORDS
                .iter()
                .filter(|k| d.text.contains(**k))
                .count();
            if hits > 0 {
                if let Some(i) = window_index(*offset, window_ms, n) {
                    series.values[i] += hits as f64;
                }
            }
        }
    }
    series
}

/// Sum CNY value of gifts + SuperChats + guard purchases per window.
pub fn gift_value(
    events: &[(u64, &LiveEvent)],
    window_ms: u64,
    total_ms: u64,
) -> WindowSeries {
    let n = n_windows(total_ms, window_ms);
    let mut series = WindowSeries::new(window_ms, n);
    for (offset, ev) in events {
        let value = match ev {
            LiveEvent::Gift(g) => g.total_price,
            LiveEvent::SuperChat(sc) => sc.price,
            LiveEvent::GuardBuy(g) => g.price * g.count as f64,
            _ => 0.0,
        };
        if value > 0.0 {
            if let Some(i) = window_index(*offset, window_ms, n) {
                series.values[i] += value;
            }
        }
    }
    series
}

/// Convert an RMS energy sample series (arbitrary sample interval) into
/// per-window mean energy. `samples` are `(offset_ms, rms)` pairs.
pub fn audio_energy_scores(
    samples: &[(u64, f64)],
    window_ms: u64,
    total_ms: u64,
) -> WindowSeries {
    let n = n_windows(total_ms, window_ms);
    let mut sums = vec![0.0; n];
    let mut counts = vec![0u32; n];
    for (offset, rms) in samples {
        if let Some(i) = window_index(*offset, window_ms, n) {
            sums[i] += rms;
            counts[i] += 1;
        }
    }
    let values = sums
        .iter()
        .zip(&counts)
        .map(|(s, c)| if *c > 0 { s / *c as f64 } else { 0.0 })
        .collect();
    WindowSeries { window_ms, values }
}

pub(crate) fn n_windows(total_ms: u64, window_ms: u64) -> usize {
    if window_ms == 0 {
        return 0;
    }
    total_ms.div_ceil(window_ms) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use vtb_common::DanmakuMsg;

    fn danmaku(text: &str) -> LiveEvent {
        LiveEvent::Danmaku(DanmakuMsg {
            room_id: 1,
            uid: 1,
            username: "u".into(),
            text: text.into(),
            timestamp: Utc::now(),
            medal: None,
            guard_level: 0,
            is_admin: false,
            emoticon: None,
        })
    }

    #[test]
    fn density_counts_per_window() {
        let e1 = danmaku("a");
        let e2 = danmaku("b");
        let e3 = danmaku("c");
        let events = vec![(500u64, &e1), (900u64, &e2), (1500u64, &e3)];
        let s = danmaku_density(&events, 1000, 3000);
        assert_eq!(s.values, vec![2.0, 1.0, 0.0]);
    }

    #[test]
    fn density_ignores_out_of_range() {
        let e = danmaku("late");
        let events = vec![(5000u64, &e)];
        let s = danmaku_density(&events, 1000, 3000);
        assert_eq!(s.values, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn keywords_count_hits() {
        let funny = danmaku("草草草哈哈哈");
        let plain = danmaku("晚上好");
        let events = vec![(100u64, &funny), (1100u64, &plain)];
        let s = keyword_score(&events, 1000, 2000);
        // "草" + "哈哈" = 2 distinct keyword hits in window 0.
        assert_eq!(s.values[0], 2.0);
        assert_eq!(s.values[1], 0.0);
    }

    #[test]
    fn gift_values_accumulate() {
        use vtb_common::{GiftMsg, SuperChatMsg};
        let gift = LiveEvent::Gift(GiftMsg {
            room_id: 1,
            uid: 1,
            username: "u".into(),
            gift_name: "g".into(),
            gift_id: 1,
            count: 1,
            total_price: 5.0,
            timestamp: Utc::now(),
        });
        let sc = LiveEvent::SuperChat(SuperChatMsg {
            room_id: 1,
            uid: 2,
            username: "v".into(),
            text: "hi".into(),
            price: 30.0,
            duration_secs: 60,
            timestamp: Utc::now(),
        });
        let events = vec![(0u64, &gift), (200u64, &sc)];
        let s = gift_value(&events, 1000, 1000);
        assert_eq!(s.values, vec![35.0]);
    }

    #[test]
    fn audio_energy_averages_within_window() {
        let samples = vec![(0u64, 0.2), (500u64, 0.4), (1500u64, 1.0)];
        let s = audio_energy_scores(&samples, 1000, 2000);
        assert!((s.values[0] - 0.3).abs() < 1e-9);
        assert!((s.values[1] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn zscores_normalize() {
        let s = WindowSeries {
            window_ms: 1000,
            values: vec![1.0, 2.0, 3.0],
        };
        let z = s.zscores();
        assert!((z[0] + 1.224744871).abs() < 1e-6);
        assert!(z[1].abs() < 1e-9);
        assert!((z[2] - 1.224744871).abs() < 1e-6);
    }

    #[test]
    fn zscores_flat_series_is_zero() {
        let s = WindowSeries {
            window_ms: 1000,
            values: vec![5.0, 5.0, 5.0],
        };
        assert_eq!(s.zscores(), vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn window_count_rounds_up() {
        assert_eq!(n_windows(2500, 1000), 3);
        assert_eq!(n_windows(2000, 1000), 2);
        assert_eq!(n_windows(0, 1000), 0);
    }
}
