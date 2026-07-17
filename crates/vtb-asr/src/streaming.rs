//! Streaming ASR: PCM in → VAD chunking → engine → TranscriptSegments out.

use crate::engine::AsrEngine;
use crate::vad::{SpeechSpan, Vad, VadConfig};
use crate::SAMPLE_RATE;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::mpsc;
use vtb_common::TranscriptSegment;

#[derive(Debug, Clone, Default)]
pub struct StreamingConfig {
    pub vad: VadConfig,
}

/// Ring buffer of recent PCM so VAD spans can be sliced back out.
struct PcmWindow {
    data: VecDeque<f32>,
    /// Absolute index of data[0].
    offset: usize,
}

impl PcmWindow {
    fn new() -> Self {
        Self {
            data: VecDeque::new(),
            offset: 0,
        }
    }

    fn push(&mut self, samples: &[f32]) {
        self.data.extend(samples.iter().copied());
    }

    fn slice(&self, start: usize, end: usize) -> Option<Vec<f32>> {
        if start < self.offset || end > self.offset + self.data.len() || end <= start {
            return None;
        }
        Some(
            self.data
                .iter()
                .skip(start - self.offset)
                .take(end - start)
                .copied()
                .collect(),
        )
    }

    /// Drop everything before absolute index `keep_from`.
    fn trim(&mut self, keep_from: usize) {
        let drop = keep_from.saturating_sub(self.offset).min(self.data.len());
        self.data.drain(..drop);
        self.offset += drop;
    }
}

pub struct StreamingAsr {
    engine: Arc<dyn AsrEngine>,
    vad: Vad,
    window: PcmWindow,
}

impl StreamingAsr {
    pub fn new(engine: Arc<dyn AsrEngine>, config: StreamingConfig) -> Self {
        Self {
            engine,
            vad: Vad::new(config.vad),
            window: PcmWindow::new(),
        }
    }

    /// Feed PCM; transcribe every utterance the VAD closes; return segments.
    pub async fn feed(&mut self, samples: &[f32]) -> Vec<TranscriptSegment> {
        self.window.push(samples);
        self.vad.push(samples);
        let spans = self.vad.take_finished();
        self.transcribe_spans(spans).await
    }

    /// Flush at end of stream.
    pub async fn finish(&mut self) -> Vec<TranscriptSegment> {
        let spans = self.vad.flush();
        self.transcribe_spans(spans).await
    }

    async fn transcribe_spans(&mut self, spans: Vec<SpeechSpan>) -> Vec<TranscriptSegment> {
        let mut out = Vec::new();
        for span in spans {
            let Some(pcm) = self.window.slice(span.start_sample, span.end_sample)
            else {
                tracing::warn!("span fell out of PCM window");
                continue;
            };
            match self.engine.transcribe(&pcm).await {
                Ok(rec) if !rec.text.is_empty() => {
                    out.push(TranscriptSegment {
                        start_ms: span.start_ms(),
                        end_ms: span.end_ms(),
                        text: rec.text,
                        lang: rec.lang,
                        is_final: true,
                    });
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("transcription failed: {e}"),
            }
            // Trim consumed audio; keep anything after this span.
            self.window.trim(span.end_sample);
        }
        out
    }

    /// Channel-driven task: read PCM chunks, emit final segments.
    pub async fn run(
        mut self,
        mut rx: mpsc::Receiver<Vec<f32>>,
        tx: mpsc::Sender<TranscriptSegment>,
    ) {
        while let Some(chunk) = rx.recv().await {
            for seg in self.feed(&chunk).await {
                if tx.send(seg).await.is_err() {
                    return;
                }
            }
        }
        for seg in self.finish().await {
            if tx.send(seg).await.is_err() {
                return;
            }
        }
    }
}

/// Offline: transcribe a whole PCM buffer via VAD chunking.
pub async fn transcribe_buffer(
    engine: Arc<dyn AsrEngine>,
    pcm: &[f32],
    config: StreamingConfig,
) -> Vec<TranscriptSegment> {
    let mut asr = StreamingAsr::new(engine, config);
    let mut out = asr.feed(pcm).await;
    out.extend(asr.finish().await);
    out
}

const _: () = {
    // Compile-time guard that SAMPLE_RATE stays consistent with vad.
    assert!(SAMPLE_RATE == 16_000);
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::mock::MockEngine;
    use crate::engine::Recognition;
    use crate::vad::{FRAME_LEN, VadConfig};

    fn vad_cfg() -> VadConfig {
        VadConfig {
            threshold: 0.01,
            hangover_frames: 3,
            min_speech_frames: 2,
            max_speech_frames: 1000,
        }
    }

    fn signal(pattern: &[(bool, usize)]) -> Vec<f32> {
        let mut out = Vec::new();
        for &(loud, frames) in pattern {
            let amp = if loud { 0.5 } else { 0.0 };
            for i in 0..frames * FRAME_LEN {
                out.push(amp * ((i as f32) * 0.3).sin());
            }
        }
        out
    }

    #[tokio::test]
    async fn two_utterances_two_segments() {
        let engine = Arc::new(MockEngine::new(vec![
            Recognition {
                text: "第一句".into(),
                lang: Some("zh".into()),
            },
            Recognition {
                text: "第二句".into(),
                lang: Some("zh".into()),
            },
        ]));
        let pcm = signal(&[
            (false, 5),
            (true, 10),
            (false, 10),
            (true, 10),
            (false, 10),
        ]);
        let segs = transcribe_buffer(
            engine,
            &pcm,
            StreamingConfig { vad: vad_cfg() },
        )
        .await;
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "第一句");
        assert_eq!(segs[1].text, "第二句");
        assert!(segs[0].end_ms <= segs[1].start_ms);
        assert!(segs.iter().all(|s| s.is_final));
    }

    #[tokio::test]
    async fn chunked_feed_matches_batch() {
        let pcm = signal(&[(false, 5), (true, 12), (false, 8), (true, 9), (false, 8)]);

        let batch = transcribe_buffer(
            Arc::new(MockEngine::empty()),
            &pcm,
            StreamingConfig { vad: vad_cfg() },
        )
        .await;

        let engine = Arc::new(MockEngine::empty());
        let mut asr =
            StreamingAsr::new(engine, StreamingConfig { vad: vad_cfg() });
        let mut inc = Vec::new();
        for chunk in pcm.chunks(499) {
            inc.extend(asr.feed(chunk).await);
        }
        inc.extend(asr.finish().await);

        assert_eq!(batch.len(), inc.len());
        for (a, b) in batch.iter().zip(&inc) {
            assert_eq!(a.start_ms, b.start_ms);
            assert_eq!(a.end_ms, b.end_ms);
        }
    }

    #[tokio::test]
    async fn empty_recognitions_skipped() {
        let engine = Arc::new(MockEngine::new(vec![Recognition {
            text: "".into(),
            lang: None,
        }]));
        let pcm = signal(&[(false, 5), (true, 10), (false, 10)]);
        let segs =
            transcribe_buffer(engine, &pcm, StreamingConfig { vad: vad_cfg() }).await;
        assert!(segs.is_empty());
    }

    #[tokio::test]
    async fn channel_run_emits_and_closes() {
        let engine = Arc::new(MockEngine::empty());
        let asr = StreamingAsr::new(engine, StreamingConfig { vad: vad_cfg() });
        let (pcm_tx, pcm_rx) = mpsc::channel(8);
        let (seg_tx, mut seg_rx) = mpsc::channel(8);
        let handle = tokio::spawn(asr.run(pcm_rx, seg_tx));

        pcm_tx
            .send(signal(&[(false, 5), (true, 10), (false, 10)]))
            .await
            .unwrap();
        drop(pcm_tx);

        let seg = seg_rx.recv().await.unwrap();
        assert!(seg.text.starts_with("utt-"));
        assert!(seg_rx.recv().await.is_none());
        handle.await.unwrap();
    }

    #[test]
    fn pcm_window_slicing_and_trim() {
        let mut w = PcmWindow::new();
        w.push(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(w.slice(1, 3), Some(vec![2.0, 3.0]));
        w.trim(2);
        assert_eq!(w.slice(2, 4), Some(vec![3.0, 4.0]));
        assert_eq!(w.slice(0, 2), None); // trimmed away
        assert_eq!(w.slice(4, 9), None); // beyond end
    }
}
