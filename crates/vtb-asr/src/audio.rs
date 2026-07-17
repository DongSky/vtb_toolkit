//! PCM audio utilities: WAV IO, resampling, ffmpeg extraction.

use crate::error::{AsrError, Result};
use crate::SAMPLE_RATE;
use std::path::Path;

/// Read a WAV file into 16 kHz mono f32 samples (resampling/downmixing
/// as needed).
pub fn read_wav_16k_mono(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.samples::<f32>().collect::<std::result::Result<_, _>>()?
        }
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<std::result::Result<_, _>>()?
        }
    };
    let mono = downmix(&raw, spec.channels as usize);
    Ok(resample_linear(&mono, spec.sample_rate, SAMPLE_RATE))
}

/// Average interleaved channels into mono.
pub fn downmix(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

/// Linear-interpolation resampler (adequate for speech features).
pub fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = pos.floor() as usize;
        let frac = (pos - idx as f64) as f32;
        let a = input[idx.min(input.len() - 1)];
        let b = input[(idx + 1).min(input.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

/// Write mono f32 samples to a 16-bit WAV (for debugging / round-trips).
pub fn write_wav_16k_mono(path: &Path, samples: &[f32]) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for s in samples {
        writer.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    Ok(())
}

/// Extract 16 kHz mono PCM from any media file via ffmpeg (s16le piped).
pub async fn extract_audio_ffmpeg(ffmpeg: &Path, input: &Path) -> Result<Vec<f32>> {
    let out = tokio::process::Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(input)
        .args([
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-f",
            "s16le",
            "-acodec",
            "pcm_s16le",
            "-",
        ])
        .output()
        .await
        .map_err(|e| AsrError::Ffmpeg(format!("spawn: {e}")))?;
    if !out.status.success() {
        return Err(AsrError::Ffmpeg(format!(
            "exit {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(out
        .stdout
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / i16::MAX as f32)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_stereo_averages() {
        let stereo = vec![1.0, 0.0, 0.5, 0.5, -1.0, 1.0];
        assert_eq!(downmix(&stereo, 2), vec![0.5, 0.5, 0.0]);
    }

    #[test]
    fn downmix_mono_is_identity() {
        let mono = vec![0.1, 0.2];
        assert_eq!(downmix(&mono, 1), mono);
    }

    #[test]
    fn resample_identity_when_same_rate() {
        let x = vec![0.0, 1.0, 0.5];
        assert_eq!(resample_linear(&x, 16000, 16000), x);
    }

    #[test]
    fn resample_halves_length() {
        let x: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
        let y = resample_linear(&x, 32000, 16000);
        assert_eq!(y.len(), 50);
        // Should still be monotonically increasing.
        assert!(y.windows(2).all(|w| w[1] >= w[0]));
    }

    #[test]
    fn resample_upsamples() {
        let x = vec![0.0, 1.0];
        let y = resample_linear(&x, 8000, 16000);
        assert_eq!(y.len(), 4);
        assert!((y[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn wav_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.wav");
        let samples: Vec<f32> = (0..16000)
            .map(|i| (i as f32 * 0.01).sin() * 0.5)
            .collect();
        write_wav_16k_mono(&path, &samples).unwrap();
        let loaded = read_wav_16k_mono(&path).unwrap();
        assert_eq!(loaded.len(), samples.len());
        // 16-bit quantization error bound.
        for (a, b) in samples.iter().zip(&loaded) {
            assert!((a - b).abs() < 1e-3);
        }
    }

    #[tokio::test]
    async fn ffmpeg_extraction_from_generated_media() {
        if !ffmpeg_on_path() {
            eprintln!("ffmpeg not found; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.mp4");
        let st = tokio::process::Command::new("ffmpeg")
            .args([
                "-y", "-hide_banner", "-loglevel", "error",
                "-f", "lavfi", "-i", "sine=frequency=440:duration=2",
                "-c:a", "aac",
            ])
            .arg(&src)
            .status()
            .await
            .unwrap();
        assert!(st.success());
        let pcm = extract_audio_ffmpeg(Path::new("ffmpeg"), &src).await.unwrap();
        // ~2 seconds at 16k.
        assert!((pcm.len() as i64 - 32000).abs() < 4000);
        // Sine wave should have significant energy (lavfi sine defaults to
        // ~1/8 full-scale amplitude → RMS ≈ 0.09).
        let rms = (pcm.iter().map(|s| s * s).sum::<f32>() / pcm.len() as f32).sqrt();
        assert!(rms > 0.05, "rms was {rms}");
    }

    fn ffmpeg_on_path() -> bool {
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).any(|d| d.join("ffmpeg").exists()))
            .unwrap_or(false)
    }
}
