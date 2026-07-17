//! Realtime highlight detection over a live event stream.
//!
//! Incremental version of the offline fusion: events are bucketed into
//! fixed windows as they arrive; each closed window's combined score is
//! compared against a rolling baseline (z-score). Peaks raise
//! [`HighlightAlert`]s — "现在可能是名场面" pings for the streamer/clipper.

use serde::Serialize;
use std::collections::VecDeque;
use vtb_common::LiveEvent;

#[derive(Debug, Clone)]
pub struct RealtimeConfig {
    /// Bucket size.
    pub window_ms: u64,
    /// Rolling baseline length (windows).
    pub history_windows: usize,
    /// Minimum closed windows before alerts can fire.
    pub min_history: usize,
    /// Combined z-score needed to alert.
    pub threshold: f64,
    /// Suppress further alerts for this long after one fires.
    pub cooldown_ms: u64,
    /// Signal weights (mirror offline fusion defaults).
    pub w_density: f64,
    pub w_keyword: f64,
    pub w_gift: f64,
}

impl Default for RealtimeConfig {
    fn default() -> Self {
        Self {
            window_ms: 10_000,
            history_windows: 30, // 5 min baseline
            min_history: 6,      // 1 min warm-up
            threshold: 2.5,
            cooldown_ms: 60_000,
            w_density: 1.0,
            w_keyword: 0.8,
            w_gift: 0.6,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HighlightAlert {
    pub window_start_ms: u64,
    pub window_end_ms: u64,
    /// Combined z-score vs the rolling baseline.
    pub z: f64,
    pub danmaku_count: u64,
    pub keyword_hits: u64,
    pub gift_value: f64,
    pub reason: String,
}

#[derive(Debug, Default, Clone)]
struct WindowAccum {
    danmaku: u64,
    keywords: u64,
    gift: f64,
}

pub struct RealtimeDetector {
    config: RealtimeConfig,
    window_start: u64,
    accum: WindowAccum,
    /// Combined scores of closed windows (rolling baseline).
    history: VecDeque<f64>,
    last_alert_end: Option<u64>,
}

impl RealtimeDetector {
    pub fn new(config: RealtimeConfig) -> Self {
        Self {
            config,
            window_start: 0,
            accum: WindowAccum::default(),
            history: VecDeque::new(),
            last_alert_end: None,
        }
    }

    /// Feed one event at `offset_ms` from stream start (monotonic).
    /// Returns an alert when a just-closed window is a peak.
    pub fn on_event(&mut self, offset_ms: u64, ev: &LiveEvent) -> Option<HighlightAlert> {
        let mut alert = None;
        // Close every window that ended before this event.
        while offset_ms >= self.window_start + self.config.window_ms {
            if let Some(a) = self.close_window() {
                // Keep the strongest alert if multiple windows closed.
                if alert
                    .as_ref()
                    .map(|prev: &HighlightAlert| a.z > prev.z)
                    .unwrap_or(true)
                {
                    alert = Some(a);
                }
            }
            self.window_start += self.config.window_ms;
        }

        match ev {
            LiveEvent::Danmaku(d) => {
                self.accum.danmaku += 1;
                self.accum.keywords += crate::signals::EMOTION_KEYWORDS
                    .iter()
                    .filter(|k| d.text.contains(**k))
                    .count() as u64;
            }
            LiveEvent::Gift(g) => self.accum.gift += g.total_price,
            LiveEvent::SuperChat(sc) => self.accum.gift += sc.price,
            LiveEvent::GuardBuy(g) => self.accum.gift += g.price * g.count as f64,
            _ => {}
        }
        alert
    }

    fn combined(&self, w: &WindowAccum) -> f64 {
        self.config.w_density * w.danmaku as f64
            + self.config.w_keyword * w.keywords as f64
            + self.config.w_gift * w.gift
    }

    fn close_window(&mut self) -> Option<HighlightAlert> {
        let window = std::mem::take(&mut self.accum);
        let score = self.combined(&window);

        let result = if self.history.len() >= self.config.min_history {
            let n = self.history.len() as f64;
            let mean = self.history.iter().sum::<f64>() / n;
            let var =
                self.history.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
            let std = var.sqrt().max(1e-9);
            let z = (score - mean) / std;
            let window_end = self.window_start + self.config.window_ms;
            let cooled = self
                .last_alert_end
                .map(|t| self.window_start >= t + self.config.cooldown_ms)
                .unwrap_or(true);
            if z >= self.config.threshold && score > 0.0 && cooled {
                self.last_alert_end = Some(window_end);
                let mut reasons = vec![];
                if window.danmaku > 0 {
                    reasons.push("弹幕爆发");
                }
                if window.keywords > 2 {
                    reasons.push("情绪关键词");
                }
                if window.gift > 0.0 {
                    reasons.push("礼物/SC");
                }
                Some(HighlightAlert {
                    window_start_ms: self.window_start,
                    window_end_ms: window_end,
                    z,
                    danmaku_count: window.danmaku,
                    keyword_hits: window.keywords,
                    gift_value: window.gift,
                    reason: if reasons.is_empty() {
                        "综合信号峰值".into()
                    } else {
                        reasons.join(" + ")
                    },
                })
            } else {
                None
            }
        } else {
            None
        };

        self.history.push_back(score);
        while self.history.len() > self.config.history_windows {
            self.history.pop_front();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use vtb_common::{DanmakuMsg, GiftMsg};

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

    fn gift(value: f64) -> LiveEvent {
        LiveEvent::Gift(GiftMsg {
            room_id: 1,
            uid: 1,
            username: "u".into(),
            gift_name: "g".into(),
            gift_id: 1,
            count: 1,
            total_price: value,
            timestamp: Utc::now(),
        })
    }

    fn cfg() -> RealtimeConfig {
        RealtimeConfig {
            window_ms: 1000,
            history_windows: 20,
            min_history: 5,
            threshold: 2.5,
            cooldown_ms: 3000,
            ..Default::default()
        }
    }

    /// Feed `n` danmaku spread inside window starting at `w`*1000ms.
    fn feed_window(det: &mut RealtimeDetector, w: u64, n: u64) -> Vec<HighlightAlert> {
        let mut alerts = vec![];
        for i in 0..n.max(1) {
            let off = w * 1000 + (i * 900 / n.max(1)).min(999);
            let ev = if n == 0 { gift(0.0) } else { danmaku("你好") };
            if let Some(a) = det.on_event(off, &ev) {
                alerts.push(a);
            }
        }
        alerts
    }

    #[test]
    fn quiet_stream_never_alerts() {
        let mut det = RealtimeDetector::new(cfg());
        let mut alerts = vec![];
        for w in 0..30 {
            alerts.extend(feed_window(&mut det, w, 3));
        }
        assert!(alerts.is_empty(), "{alerts:?}");
    }

    #[test]
    fn burst_after_baseline_alerts_once_with_cooldown() {
        let mut det = RealtimeDetector::new(cfg());
        let mut alerts = vec![];
        // Baseline: 10 windows of ~3 msgs.
        for w in 0..10 {
            alerts.extend(feed_window(&mut det, w, 3));
        }
        assert!(alerts.is_empty());
        // Burst: windows 10-12 with 40 msgs each.
        for w in 10..13 {
            alerts.extend(feed_window(&mut det, w, 40));
        }
        // Push one more quiet window so the last burst window closes.
        alerts.extend(feed_window(&mut det, 13, 1));
        assert_eq!(alerts.len(), 1, "cooldown must suppress repeats: {alerts:?}");
        let a = &alerts[0];
        assert!(a.z >= 2.5);
        assert_eq!(a.danmaku_count, 40);
        assert!(a.reason.contains("弹幕爆发"));
    }

    #[test]
    fn no_alert_during_warmup() {
        let mut det = RealtimeDetector::new(cfg());
        let mut alerts = vec![];
        // Big burst immediately in window 1 with tiny history.
        alerts.extend(feed_window(&mut det, 0, 2));
        alerts.extend(feed_window(&mut det, 1, 50));
        alerts.extend(feed_window(&mut det, 2, 1));
        assert!(alerts.is_empty(), "warm-up must gate alerts");
    }

    #[test]
    fn gift_burst_alerts() {
        let mut det = RealtimeDetector::new(cfg());
        let mut alerts = vec![];
        for w in 0..10 {
            alerts.extend(feed_window(&mut det, w, 3));
        }
        // ¥500 gift bomb in window 10.
        if let Some(a) = det.on_event(10_500, &gift(500.0)) {
            alerts.push(a);
        }
        // Close it.
        if let Some(a) = det.on_event(11_500, &danmaku("x")) {
            alerts.push(a);
        }
        assert_eq!(alerts.len(), 1);
        assert!(alerts[0].gift_value >= 500.0);
        assert!(alerts[0].reason.contains("礼物"));
    }

    #[test]
    fn alert_fires_again_after_cooldown() {
        let mut det = RealtimeDetector::new(cfg());
        let mut alerts = vec![];
        for w in 0..10 {
            alerts.extend(feed_window(&mut det, w, 3));
        }
        alerts.extend(feed_window(&mut det, 10, 40));
        alerts.extend(feed_window(&mut det, 11, 3)); // closes burst → alert 1
        // Quiet padding beyond cooldown (3s = 3 windows).
        for w in 12..16 {
            alerts.extend(feed_window(&mut det, w, 3));
        }
        alerts.extend(feed_window(&mut det, 16, 60));
        alerts.extend(feed_window(&mut det, 17, 3)); // closes burst → alert 2
        assert_eq!(alerts.len(), 2, "{alerts:?}");
    }
}
