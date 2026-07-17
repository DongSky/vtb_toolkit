//! Audio RMS energy series for highlight detection.

/// Compute `(offset_ms, rms)` samples over `hop_ms` hops of 16 kHz PCM.
pub fn rms_series(pcm: &[f32], hop_ms: u64) -> Vec<(u64, f64)> {
    let hop = ((16_000 * hop_ms) / 1000) as usize;
    if hop == 0 || pcm.is_empty() {
        return vec![];
    }
    pcm.chunks(hop)
        .enumerate()
        .map(|(i, chunk)| {
            let rms = (chunk.iter().map(|s| (*s as f64).powi(2)).sum::<f64>()
                / chunk.len() as f64)
                .sqrt();
            (i as u64 * hop_ms, rms)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_signal_rms() {
        let pcm = vec![0.5f32; 32_000]; // 2 s
        let series = rms_series(&pcm, 1000);
        assert_eq!(series.len(), 2);
        assert_eq!(series[0].0, 0);
        assert_eq!(series[1].0, 1000);
        assert!((series[0].1 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn silence_is_zero() {
        let pcm = vec![0.0f32; 16_000];
        let series = rms_series(&pcm, 500);
        assert_eq!(series.len(), 2);
        assert_eq!(series[0].1, 0.0);
    }

    #[test]
    fn empty_and_zero_hop() {
        assert!(rms_series(&[], 1000).is_empty());
        assert!(rms_series(&[0.1], 0).is_empty());
    }

    #[test]
    fn partial_final_chunk_counted() {
        let pcm = vec![1.0f32; 24_000]; // 1.5 s
        let series = rms_series(&pcm, 1000);
        assert_eq!(series.len(), 2);
        assert!((series[1].1 - 1.0).abs() < 1e-6);
    }
}
