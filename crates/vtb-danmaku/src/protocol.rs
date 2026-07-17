//! Bilibili danmaku WebSocket wire protocol.
//!
//! Every message is framed with a 16-byte big-endian header:
//!
//! | offset | size | field            | notes                              |
//! |--------|------|------------------|------------------------------------|
//! | 0      | 4    | packet length    | total length incl. header          |
//! | 4      | 2    | header length    | always 16                          |
//! | 6      | 2    | protocol version | 0 json, 1 heartbeat/popularity, 2 zlib, 3 brotli |
//! | 8      | 4    | operation        | see [`Operation`]                  |
//! | 12     | 4    | sequence         | usually 1                          |
//!
//! A single frame may, after decompression (proto 2/3), contain multiple
//! concatenated frames — [`decode_packet`] returns all inner payloads.

use crate::error::{DanmakuError, Result};
use byteorder::{BigEndian, ByteOrder};
use std::io::Read;

pub const HEADER_LEN: usize = 16;

/// Protocol version stored in the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtoVer {
    /// Body is uncompressed JSON.
    Json = 0,
    /// Body is an int32 popularity value / heartbeat.
    HeartbeatReply = 1,
    /// Body is zlib-compressed and contains nested frames.
    Zlib = 2,
    /// Body is brotli-compressed and contains nested frames.
    Brotli = 3,
}

impl ProtoVer {
    pub fn from_u16(v: u16) -> Result<Self> {
        Ok(match v {
            0 => ProtoVer::Json,
            1 => ProtoVer::HeartbeatReply,
            2 => ProtoVer::Zlib,
            3 => ProtoVer::Brotli,
            other => return Err(DanmakuError::UnsupportedProtocol(other)),
        })
    }
}

/// Operation code stored in the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Operation {
    Heartbeat = 2,
    HeartbeatReply = 3,
    /// A command / danmaku message (JSON body).
    Message = 5,
    /// Authentication (first packet after connect).
    Auth = 7,
    /// Server ack for auth.
    AuthReply = 8,
    /// Unknown / other operation code.
    Unknown = u32::MAX,
}

impl Operation {
    pub fn from_u32(v: u32) -> Self {
        match v {
            2 => Operation::Heartbeat,
            3 => Operation::HeartbeatReply,
            5 => Operation::Message,
            7 => Operation::Auth,
            8 => Operation::AuthReply,
            _ => Operation::Unknown,
        }
    }
}

/// A decoded frame: its operation, protocol version, and raw body bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub operation: Operation,
    pub proto: ProtoVer,
    pub body: Vec<u8>,
}

/// Encode a single frame (header + body) for sending.
pub fn encode(operation: Operation, proto: ProtoVer, body: &[u8]) -> Vec<u8> {
    let total = HEADER_LEN + body.len();
    let mut buf = vec![0u8; total];
    BigEndian::write_u32(&mut buf[0..4], total as u32);
    BigEndian::write_u16(&mut buf[4..6], HEADER_LEN as u16);
    BigEndian::write_u16(&mut buf[6..8], proto as u16);
    BigEndian::write_u32(&mut buf[8..12], operation as u32);
    BigEndian::write_u32(&mut buf[12..16], 1); // sequence
    buf[HEADER_LEN..].copy_from_slice(body);
    buf
}

/// Build the auth packet body (JSON) sent as the first packet.
pub fn auth_body(uid: u64, room_id: u64, token: &str, buvid: Option<&str>) -> Vec<u8> {
    // protover 3 requests brotli-compressed downstream frames.
    let mut obj = serde_json::json!({
        "uid": uid,
        "roomid": room_id,
        "protover": 3,
        "platform": "web",
        "type": 2,
        "key": token,
    });
    if let Some(b) = buvid {
        obj["buvid"] = serde_json::Value::String(b.to_string());
    }
    serde_json::to_vec(&obj).expect("auth json serializes")
}

/// The heartbeat packet (empty body, sent every ~30s).
pub fn heartbeat() -> Vec<u8> {
    encode(Operation::Heartbeat, ProtoVer::Json, b"[object Object]")
}

/// Parse the 16-byte header of a frame, returning
/// `(packet_len, header_len, proto, operation)`.
pub fn parse_header(buf: &[u8]) -> Result<(usize, usize, ProtoVer, Operation)> {
    if buf.len() < HEADER_LEN {
        return Err(DanmakuError::ShortPacket {
            need: HEADER_LEN,
            got: buf.len(),
        });
    }
    let packet_len = BigEndian::read_u32(&buf[0..4]) as usize;
    let header_len = BigEndian::read_u16(&buf[4..6]) as usize;
    let proto = ProtoVer::from_u16(BigEndian::read_u16(&buf[6..8]))?;
    let operation = Operation::from_u32(BigEndian::read_u32(&buf[8..12]));
    Ok((packet_len, header_len, proto, operation))
}

/// Fully decode a raw WebSocket binary message into flattened frames.
///
/// Handles concatenated frames and recursively decompresses zlib/brotli
/// payloads (which themselves contain concatenated frames).
pub fn decode_packet(buf: &[u8]) -> Result<Vec<Frame>> {
    let mut out = Vec::new();
    decode_into(buf, &mut out)?;
    Ok(out)
}

fn decode_into(buf: &[u8], out: &mut Vec<Frame>) -> Result<()> {
    let mut offset = 0;
    while offset < buf.len() {
        let remaining = &buf[offset..];
        let (packet_len, header_len, proto, operation) = parse_header(remaining)?;
        if packet_len < header_len || packet_len > remaining.len() {
            return Err(DanmakuError::ShortPacket {
                need: packet_len,
                got: remaining.len(),
            });
        }
        let body = &remaining[header_len..packet_len];
        match proto {
            ProtoVer::Zlib => {
                let decompressed = inflate_zlib(body)?;
                decode_into(&decompressed, out)?;
            }
            ProtoVer::Brotli => {
                let decompressed = inflate_brotli(body)?;
                decode_into(&decompressed, out)?;
            }
            ProtoVer::Json | ProtoVer::HeartbeatReply => {
                out.push(Frame {
                    operation,
                    proto,
                    body: body.to_vec(),
                });
            }
        }
        offset += packet_len;
    }
    Ok(())
}

fn inflate_zlib(data: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = flate2::read::ZlibDecoder::new(data);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| DanmakuError::Decompress(format!("zlib: {e}")))?;
    Ok(out)
}

fn inflate_brotli(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut reader = brotli::Decompressor::new(data, 4096);
    reader
        .read_to_end(&mut out)
        .map_err(|e| DanmakuError::Decompress(format!("brotli: {e}")))?;
    Ok(out)
}

/// Extract the popularity/watched integer from a heartbeat-reply body.
pub fn parse_popularity(body: &[u8]) -> Option<u32> {
    if body.len() >= 4 {
        Some(BigEndian::read_u32(&body[0..4]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn header_roundtrip() {
        let frame = encode(Operation::Auth, ProtoVer::Json, b"hello");
        assert_eq!(frame.len(), HEADER_LEN + 5);
        let (plen, hlen, proto, op) = parse_header(&frame).unwrap();
        assert_eq!(plen, HEADER_LEN + 5);
        assert_eq!(hlen, HEADER_LEN);
        assert_eq!(proto, ProtoVer::Json);
        assert_eq!(op, Operation::Auth);
        assert_eq!(&frame[HEADER_LEN..], b"hello");
    }

    #[test]
    fn short_header_errors() {
        let err = parse_header(&[0u8; 4]).unwrap_err();
        assert!(matches!(err, DanmakuError::ShortPacket { .. }));
    }

    #[test]
    fn unsupported_proto_errors() {
        assert!(ProtoVer::from_u16(9).is_err());
    }

    #[test]
    fn decode_single_json_frame() {
        let body = br#"{"cmd":"DANMU_MSG"}"#;
        let frame = encode(Operation::Message, ProtoVer::Json, body);
        let frames = decode_packet(&frame).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].operation, Operation::Message);
        assert_eq!(frames[0].body, body);
    }

    #[test]
    fn decode_concatenated_frames() {
        let mut buf = Vec::new();
        buf.extend(encode(Operation::Message, ProtoVer::Json, b"{\"a\":1}"));
        buf.extend(encode(Operation::Message, ProtoVer::Json, b"{\"b\":2}"));
        let frames = decode_packet(&buf).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].body, b"{\"a\":1}");
        assert_eq!(frames[1].body, b"{\"b\":2}");
    }

    #[test]
    fn decode_brotli_nested_frames() {
        // Two inner JSON frames, brotli-compressed as one outer frame.
        let mut inner = Vec::new();
        inner.extend(encode(Operation::Message, ProtoVer::Json, b"{\"x\":1}"));
        inner.extend(encode(Operation::Message, ProtoVer::Json, b"{\"y\":2}"));

        let mut compressed = Vec::new();
        {
            let mut writer =
                brotli::CompressorWriter::new(&mut compressed, 4096, 5, 22);
            writer.write_all(&inner).unwrap();
        }
        let outer = encode(Operation::Message, ProtoVer::Brotli, &compressed);
        let frames = decode_packet(&outer).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].body, b"{\"x\":1}");
        assert_eq!(frames[1].body, b"{\"y\":2}");
    }

    #[test]
    fn decode_zlib_nested_frames() {
        let inner = encode(Operation::Message, ProtoVer::Json, b"{\"z\":3}");
        let mut compressed = Vec::new();
        {
            let mut enc = flate2::write::ZlibEncoder::new(
                &mut compressed,
                flate2::Compression::default(),
            );
            enc.write_all(&inner).unwrap();
            enc.finish().unwrap();
        }
        let outer = encode(Operation::Message, ProtoVer::Zlib, &compressed);
        let frames = decode_packet(&outer).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].body, b"{\"z\":3}");
    }

    #[test]
    fn popularity_parsing() {
        let body = 12345u32.to_be_bytes();
        assert_eq!(parse_popularity(&body), Some(12345));
        assert_eq!(parse_popularity(&[0u8; 2]), None);
    }

    #[test]
    fn auth_body_contains_fields() {
        let body = auth_body(42, 100, "tok", Some("BUVID"));
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["uid"], 42);
        assert_eq!(v["roomid"], 100);
        assert_eq!(v["key"], "tok");
        assert_eq!(v["protover"], 3);
        assert_eq!(v["buvid"], "BUVID");
    }
}
