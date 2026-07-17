//! Energy-based voice activity detection with hangover smoothing.
//!
//! Frames of 30 ms are classified speech/silence against an adaptive RMS
//! threshold; hangover keeps short pauses inside an utterance. Emits
//! speech spans suitable for feeding an ASR engine.
//!
//! (Deliberately dependency-free; a silero-onnx VAD can drop in behind the
//! same interface later.)

use crate::SAMPLE_RATE;

pub const FRAME_MS: u64 = 30;
pub const FRAME_LEN: usize = (SAMPLE_RATE as usize * FRAME_MS as usize) / 1000;

#[derive(Debug, Clone)]
pub struct VadConfig {
    /// RMS threshold above which a frame counts as speech.
    pub threshold: f32,
    /// Keep an utterance open across up to this many silent frames.
    pub hangover_frames: usize,
    /// Discard speech spans shorter than this many frames.
    pub min_speech_frames: usize,
    /// Force-close an utterance after this many frames (whisper's sweet
    /// spot is ≤ 30 s; default 20 s).
    pub max_speech_frames: usize,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            threshold: 0.01,
            hangover_frames: 10,        // 300 ms
            min_speech_frames: 8,       // 240 ms
            max_speech_frames: 667,     // ~20 s
        }
    }
}

/// A detected speech span, in samples relative to the start of the feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechSpan {
    pub start_sample: usize,
    pub end_sample: usize,
}

impl SpeechSpan {
    pub fn start_ms(&self) -> u64 {
        (self.start_sample as u64 * 1000) / SAMPLE_RATE as u64
    }
    pub fn end_ms(&self) -> u64 {
        (self.end_sample as u64 * 1000) / SAMPLE_RATE as u64
    }
}

pub fn frame_rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt()
}

/// Streaming VAD: push samples, poll finished utterances.
pub struct Vad {
    config: VadConfig,
    buffer: Vec<f32>,
    /// Absolute sample index of buffer[0].
    buffer_offset: usize,
    /// Currently-open speech span start (absolute samples).
    open_start: Option<usize>,
    silent_run: usize,
    speech_frames: usize,
    finished: Vec<SpeechSpan>,
}

impl Vad {
    pub fn new(config: VadConfig) -> Self {
        Self {
            config,
            buffer: Vec::new(),
            buffer_offset: 0,
            open_start: None,
            silent_run: 0,
            speech_frames: 0,
            finished: Vec::new(),
        }
    }

    /// Feed PCM samples; complete utterances accumulate internally.
    pub fn push(&mut self, samples: &[f32]) {
        self.buffer.extend_from_slice(samples);
        let mut consumed = 0;
        while self.buffer.len() - consumed >= FRAME_LEN {
            let abs_start = self.buffer_offset + consumed;
            let frame = &self.buffer[consumed..consumed + FRAME_LEN];
            let is_speech = frame_rms(frame) >= self.config.threshold;
            self.process_frame(abs_start, is_speech);
            consumed += FRAME_LEN;
        }
        self.buffer.drain(..consumed);
        self.buffer_offset += consumed;
    }

    fn process_frame(&mut self, abs_start: usize, is_speech: bool) {
        match (self.open_start, is_speech) {
            (None, true) => {
                self.open_start = Some(abs_start);
                self.silent_run = 0;
                self.speech_frames = 1;
            }
            (None, false) => {}
            (Some(start), speech) => {
                if speech {
                    self.silent_run = 0;
                } else {
                    self.silent_run += 1;
                }
                self.speech_frames += 1;

                let over_max = self.speech_frames >= self.config.max_speech_frames;
                let closed_by_silence = self.silent_run > self.config.hangover_frames;
                if closed_by_silence || over_max {
                    let end = if closed_by_silence {
                        // Trim the trailing hangover silence.
                        abs_start + FRAME_LEN
                            - self.silent_run.saturating_sub(0) * FRAME_LEN
                    } else {
                        abs_start + FRAME_LEN
                    };
                    let speech_len_frames =
                        self.speech_frames - self.silent_run.min(self.speech_frames);
                    if speech_len_frames >= self.config.min_speech_frames {
                        self.finished.push(SpeechSpan {
                            start_sample: start,
                            end_sample: end.max(start + FRAME_LEN),
                        });
                    }
                    self.open_start = None;
                    self.silent_run = 0;
                    self.speech_frames = 0;
                }
            }
        }
    }

    /// Drain finished utterances.
    pub fn take_finished(&mut self) -> Vec<SpeechSpan> {
        std::mem::take(&mut self.finished)
    }

    /// Flush: close any open utterance (stream ended).
    pub fn flush(&mut self) -> Vec<SpeechSpan> {
        if let Some(start) = self.open_start.take() {
            let end = self.buffer_offset;
            let frames = (end - start) / FRAME_LEN;
            if frames >= self.config.min_speech_frames {
                self.finished.push(SpeechSpan {
                    start_sample: start,
                    end_sample: end,
                });
            }
        }
        self.take_finished()
    }
}

/// Offline convenience: run VAD over a full buffer.
pub fn detect_spans(samples: &[f32], config: VadConfig) -> Vec<SpeechSpan> {
    let mut vad = Vad::new(config);
    vad.push(samples);
    let mut spans = vad.take_finished();
    spans.extend(vad.flush());
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a signal: `pattern` of (is_loud, n_frames).
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

    fn cfg() -> VadConfig {
        VadConfig {
            threshold: 0.01,
            hangover_frames: 3,
            min_speech_frames: 2,
            max_speech_frames: 1000,
        }
    }

    #[test]
    fn silence_only_no_spans() {
        let spans = detect_spans(&signal(&[(false, 50)]), cfg());
        assert!(spans.is_empty());
    }

    #[test]
    fn single_utterance_detected() {
        let spans = detect_spans(&signal(&[(false, 10), (true, 20), (false, 10)]), cfg());
        assert_eq!(spans.len(), 1);
        let s = &spans[0];
        // Started at frame 10.
        assert_eq!(s.start_sample, 10 * FRAME_LEN);
        assert!(s.end_sample >= 30 * FRAME_LEN - FRAME_LEN);
    }

    #[test]
    fn short_blip_filtered() {
        let spans = detect_spans(&signal(&[(false, 10), (true, 1), (false, 10)]), cfg());
        assert!(spans.is_empty());
    }

    #[test]
    fn pause_within_hangover_stays_one_utterance() {
        // 2-frame silence gap < hangover 3 → one span.
        let spans = detect_spans(
            &signal(&[(false, 5), (true, 10), (false, 2), (true, 10), (false, 10)]),
            cfg(),
        );
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn long_pause_splits_utterances() {
        let spans = detect_spans(
            &signal(&[(false, 5), (true, 10), (false, 10), (true, 10), (false, 10)]),
            cfg(),
        );
        assert_eq!(spans.len(), 2);
        assert!(spans[0].end_sample <= spans[1].start_sample);
    }

    #[test]
    fn max_length_forces_split() {
        let mut c = cfg();
        c.max_speech_frames = 10;
        let spans = detect_spans(&signal(&[(true, 25), (false, 10)]), c);
        assert!(spans.len() >= 2, "long speech should split, got {spans:?}");
    }

    #[test]
    fn flush_closes_open_utterance() {
        let mut vad = Vad::new(cfg());
        vad.push(&signal(&[(false, 5), (true, 10)]));
        assert!(vad.take_finished().is_empty());
        let spans = vad.flush();
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn span_ms_conversion() {
        let s = SpeechSpan {
            start_sample: 16000,
            end_sample: 48000,
        };
        assert_eq!(s.start_ms(), 1000);
        assert_eq!(s.end_ms(), 3000);
    }

    #[test]
    fn incremental_push_equals_batch() {
        let sig = signal(&[(false, 5), (true, 15), (false, 8), (true, 12), (false, 8)]);
        let batch = detect_spans(&sig, cfg());

        let mut vad = Vad::new(cfg());
        for chunk in sig.chunks(97) {
            vad.push(chunk);
        }
        let mut inc = vad.take_finished();
        inc.extend(vad.flush());
        assert_eq!(batch, inc);
    }
}
