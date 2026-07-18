//! FLV tag-level timestamp repair (对标录播姬/blrec 的时间戳修复).
//!
//! Bilibili live FLV streams frequently carry timestamp discontinuities —
//! the server splices segments, so timestamps jump forward by seconds or
//! reset to zero mid-file. Stream-copied recordings inherit the jumps,
//! which desyncs subtitles and breaks cutting. This module rewrites tag
//! timestamps onto a continuous timeline while copying everything else
//! byte-for-byte.
//!
//! Algorithm (per BililiveRecorder's approach, simplified): keep a running
//! `offset` applied to every tag. When an audio/video tag's adjusted
//! timestamp jumps forward beyond `jump_threshold_ms` or backward beyond
//! `backward_tolerance_ms` relative to the last output timestamp, declare a
//! discontinuity and rebase `offset` so the tag lands `default_delta_ms`
//! after the previous one.

use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct RepairConfig {
    /// Forward jump larger than this is a discontinuity.
    pub jump_threshold_ms: i64,
    /// Backward step larger than this is a discontinuity (small backward
    /// steps are normal for interleaved A/V and pass through).
    pub backward_tolerance_ms: i64,
    /// Synthetic gap inserted at a discontinuity.
    pub default_delta_ms: i64,
}

impl Default for RepairConfig {
    fn default() -> Self {
        Self {
            jump_threshold_ms: 5_000,
            backward_tolerance_ms: 500,
            default_delta_ms: 10,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RepairStats {
    pub tags: u64,
    pub discontinuities: u64,
    /// Last output timestamp (≈ duration in ms).
    pub last_timestamp_ms: i64,
    /// A partial tag at EOF was dropped (normal for live recordings).
    pub truncated: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum FlvError {
    #[error("not an FLV file (bad signature)")]
    BadSignature,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

const TAG_AUDIO: u8 = 8;
const TAG_VIDEO: u8 = 9;

/// Read as much of `buf` as possible; returns the byte count filled
/// (== buf.len() when complete, less at EOF).
fn read_up_to<R: Read>(r: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = r.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

/// Repair `reader` into `writer`. The output is a valid FLV with identical
/// tag payloads and monotonic-enough timestamps.
pub fn repair_stream<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    cfg: &RepairConfig,
) -> Result<RepairStats, FlvError> {
    // Header: "FLV" ver flags dataoffset(u32 BE).
    let mut header = [0u8; 9];
    reader.read_exact(&mut header)?;
    if &header[0..3] != b"FLV" {
        return Err(FlvError::BadSignature);
    }
    writer.write_all(&header)?;
    // Copy any extra header bytes (dataoffset > 9 is legal, rare).
    let data_offset = u32::from_be_bytes([header[5], header[6], header[7], header[8]]);
    if data_offset > 9 {
        let mut extra = vec![0u8; (data_offset - 9) as usize];
        reader.read_exact(&mut extra)?;
        writer.write_all(&extra)?;
    }

    let mut stats = RepairStats::default();
    let mut offset: i64 = 0;
    let mut last_out: Option<i64> = None;
    let mut prev_tag_size: u32 = 0;
    let mut head = [0u8; 15]; // prevTagSize(4) + tag header(11)

    loop {
        let filled = read_up_to(reader, &mut head)?;
        if filled == 0 {
            break; // clean EOF after the last full tag
        }
        if filled < head.len() {
            stats.truncated = true;
            break;
        }
        let tag_type = head[4];
        let data_size =
            u32::from_be_bytes([0, head[5], head[6], head[7]]) as usize;
        let ts = i64::from(
            u32::from_be_bytes([head[11], head[8], head[9], head[10]]),
        );

        let mut data = vec![0u8; data_size];
        if read_up_to(reader, &mut data)? < data_size {
            stats.truncated = true;
            break;
        }

        // Timeline adjustment (audio/video drive it; script tags follow).
        let adjusted = if tag_type == TAG_AUDIO || tag_type == TAG_VIDEO {
            let mut adj = ts + offset;
            if let Some(last) = last_out {
                if adj > last + cfg.jump_threshold_ms
                    || adj < last - cfg.backward_tolerance_ms
                {
                    stats.discontinuities += 1;
                    adj = last + cfg.default_delta_ms;
                    offset = adj - ts;
                }
            }
            last_out = Some(adj);
            stats.last_timestamp_ms = adj;
            adj
        } else {
            (ts + offset).max(0)
        };
        let out_ts = adjusted.clamp(0, 0xFFFF_FFFF) as u32;

        // Write: recomputed prevTagSize + patched tag header + payload.
        writer.write_all(&prev_tag_size.to_be_bytes())?;
        let mut out_head = [0u8; 11];
        out_head.copy_from_slice(&head[4..15]);
        out_head[4] = ((out_ts >> 16) & 0xFF) as u8;
        out_head[5] = ((out_ts >> 8) & 0xFF) as u8;
        out_head[6] = (out_ts & 0xFF) as u8;
        out_head[7] = ((out_ts >> 24) & 0xFF) as u8;
        writer.write_all(&out_head)?;
        writer.write_all(&data)?;

        prev_tag_size = 11 + data_size as u32;
        stats.tags += 1;
    }

    writer.flush()?;
    Ok(stats)
}

/// Analyze + repair a file in place. Writes to `<file>.fixing.flv`, then —
/// only when discontinuities were found — atomically replaces the original
/// (the untouched original moves to `<file>.orig`). Returns the stats.
pub fn repair_file(path: &Path, cfg: &RepairConfig) -> Result<RepairStats, FlvError> {
    let tmp = path.with_extension("fixing.flv");
    let stats = {
        let mut input = std::io::BufReader::new(std::fs::File::open(path)?);
        let mut output = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        repair_stream(&mut input, &mut output, cfg)?
    };
    if stats.discontinuities > 0 {
        let backup = path.with_extension("orig");
        std::fs::rename(path, &backup)?;
        std::fs::rename(&tmp, path)?;
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(tag_type: u8, ts: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![tag_type];
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes()[1..4]);
        out.push(((ts >> 16) & 0xFF) as u8);
        out.push(((ts >> 8) & 0xFF) as u8);
        out.push((ts & 0xFF) as u8);
        out.push(((ts >> 24) & 0xFF) as u8);
        out.extend_from_slice(&[0, 0, 0]); // stream id
        out.extend_from_slice(payload);
        out
    }

    fn flv(tags: &[Vec<u8>]) -> Vec<u8> {
        let mut out = b"FLV\x01\x05\x00\x00\x00\x09".to_vec();
        let mut prev = 0u32;
        for t in tags {
            out.extend_from_slice(&prev.to_be_bytes());
            out.extend_from_slice(t);
            prev = t.len() as u32;
        }
        out
    }

    fn timestamps(flv_bytes: &[u8]) -> Vec<(u8, i64)> {
        let mut out = vec![];
        let mut pos = 9usize;
        while pos + 15 <= flv_bytes.len() {
            let h = &flv_bytes[pos + 4..pos + 15];
            let size = u32::from_be_bytes([0, h[1], h[2], h[3]]) as usize;
            let ts = i64::from(u32::from_be_bytes([h[7], h[4], h[5], h[6]]));
            out.push((h[0], ts));
            pos += 15 + size;
        }
        out
    }

    fn cfg() -> RepairConfig {
        RepairConfig::default()
    }

    #[test]
    fn continuous_stream_passes_through_unchanged() {
        let input = flv(&[
            tag(18, 0, b"meta"),
            tag(9, 0, b"v0"),
            tag(8, 12, b"a0"),
            tag(9, 33, b"v1"),
            tag(8, 45, b"a1"),
        ]);
        let mut out = vec![];
        let stats = repair_stream(&mut &input[..], &mut out, &cfg()).unwrap();
        assert_eq!(stats.tags, 5);
        assert_eq!(stats.discontinuities, 0);
        assert!(!stats.truncated);
        assert_eq!(out, input, "no rewrite when timeline is clean");
    }

    #[test]
    fn forward_jump_is_collapsed() {
        let input = flv(&[
            tag(9, 0, b"v0"),
            tag(9, 33, b"v1"),
            tag(9, 60_000, b"v2"), // server splice: +1 min
            tag(9, 60_033, b"v3"),
        ]);
        let mut out = vec![];
        let stats = repair_stream(&mut &input[..], &mut out, &cfg()).unwrap();
        assert_eq!(stats.discontinuities, 1);
        let ts: Vec<i64> = timestamps(&out).iter().map(|(_, t)| *t).collect();
        assert_eq!(ts, vec![0, 33, 43, 76], "jump collapsed to +10ms, later tags shifted");
        assert_eq!(stats.last_timestamp_ms, 76);
    }

    #[test]
    fn backward_reset_is_rebased() {
        let input = flv(&[
            tag(9, 10_000, b"v0"),
            tag(9, 10_033, b"v1"),
            tag(9, 0, b"v2"), // stream restart: ts resets
            tag(9, 33, b"v3"),
        ]);
        let mut out = vec![];
        let stats = repair_stream(&mut &input[..], &mut out, &cfg()).unwrap();
        assert_eq!(stats.discontinuities, 1);
        let ts: Vec<i64> = timestamps(&out).iter().map(|(_, t)| *t).collect();
        assert_eq!(ts, vec![10_000, 10_033, 10_043, 10_076]);
    }

    #[test]
    fn small_backward_av_interleave_tolerated() {
        let input = flv(&[
            tag(8, 100, b"a"),
            tag(9, 66, b"v"), // video slightly behind audio — normal
            tag(8, 123, b"a"),
        ]);
        let mut out = vec![];
        let stats = repair_stream(&mut &input[..], &mut out, &cfg()).unwrap();
        assert_eq!(stats.discontinuities, 0);
        assert_eq!(out, input);
    }

    #[test]
    fn script_tags_follow_offset_but_do_not_drive_timeline() {
        let input = flv(&[
            tag(9, 0, b"v0"),
            tag(9, 60_000, b"v1"),  // discontinuity → offset rebased
            tag(18, 60_001, b"meta"), // script tag rides the same offset
            tag(9, 60_033, b"v2"),
        ]);
        let mut out = vec![];
        let stats = repair_stream(&mut &input[..], &mut out, &cfg()).unwrap();
        assert_eq!(stats.discontinuities, 1);
        let ts = timestamps(&out);
        assert_eq!(ts[2], (18, 11)); // 60_001 + offset(-59_990)
        assert_eq!(ts[3], (9, 43));
    }

    #[test]
    fn truncated_tail_is_dropped_cleanly() {
        let mut input = flv(&[tag(9, 0, b"v0"), tag(9, 33, b"v1")]);
        // Append a partial tag: prevTagSize + half a header.
        input.extend_from_slice(&[0, 0, 0, 13, 9, 0, 0]);
        let mut out = vec![];
        let stats = repair_stream(&mut &input[..], &mut out, &cfg()).unwrap();
        assert_eq!(stats.tags, 2);
        assert!(stats.truncated);
        let ts: Vec<i64> = timestamps(&out).iter().map(|(_, t)| *t).collect();
        assert_eq!(ts, vec![0, 33]);
    }

    #[test]
    fn extended_timestamp_byte_roundtrips() {
        // 0x01234567 ms uses the extension byte.
        let big = 0x0123_4567u32;
        let input = flv(&[tag(9, big, b"v0"), tag(9, big + 33, b"v1")]);
        let mut out = vec![];
        let stats = repair_stream(&mut &input[..], &mut out, &cfg()).unwrap();
        assert_eq!(stats.discontinuities, 0);
        let ts: Vec<i64> = timestamps(&out).iter().map(|(_, t)| *t).collect();
        assert_eq!(ts, vec![i64::from(big), i64::from(big) + 33]);
    }

    #[test]
    fn bad_signature_rejected() {
        let input = b"MP4 not flv....".to_vec();
        let mut out = vec![];
        assert!(matches!(
            repair_stream(&mut &input[..], &mut out, &cfg()),
            Err(FlvError::BadSignature)
        ));
    }

    #[test]
    fn repair_file_replaces_only_when_needed() {
        let dir = tempfile::tempdir().unwrap();
        // Clean file: untouched, no .orig.
        let clean = dir.path().join("clean.flv");
        std::fs::write(&clean, flv(&[tag(9, 0, b"v"), tag(9, 33, b"v")])).unwrap();
        let before = std::fs::read(&clean).unwrap();
        let stats = repair_file(&clean, &cfg()).unwrap();
        assert_eq!(stats.discontinuities, 0);
        assert_eq!(std::fs::read(&clean).unwrap(), before);
        assert!(!clean.with_extension("orig").exists());
        assert!(!clean.with_extension("fixing.flv").exists());

        // Broken file: repaired in place, original kept as .orig.
        let broken = dir.path().join("broken.flv");
        std::fs::write(
            &broken,
            flv(&[tag(9, 0, b"v"), tag(9, 60_000, b"v"), tag(9, 60_033, b"v")]),
        )
        .unwrap();
        let stats = repair_file(&broken, &cfg()).unwrap();
        assert_eq!(stats.discontinuities, 1);
        assert!(broken.with_extension("orig").exists());
        let fixed = std::fs::read(&broken).unwrap();
        let ts: Vec<i64> = timestamps(&fixed).iter().map(|(_, t)| *t).collect();
        assert_eq!(ts, vec![0, 10, 43]);
    }
}
