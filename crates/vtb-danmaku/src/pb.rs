//! Minimal protobuf wire-format reader.
//!
//! Bilibili's `*_V2` danmaku commands embed a base64 protobuf. We don't have
//! the `.proto` schema and don't want a codegen dependency, so we decode the
//! wire format directly and pull out fields by number. Only the two wire
//! types we need are supported: varint (0) and length-delimited (2). Groups
//! (3/4) are obsolete; fixed32/64 (5/1) are skipped safely.

use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Fields {
    varints: HashMap<u64, u64>,
    /// length-delimited payloads (last-wins per field, like our varints)
    bytes: HashMap<u64, Vec<u8>>,
}

impl Fields {
    pub fn varint(&self, field: u64) -> Option<u64> {
        self.varints.get(&field).copied()
    }

    pub fn bytes(&self, field: u64) -> Option<&[u8]> {
        self.bytes.get(&field).map(|v| v.as_slice())
    }

    pub fn string(&self, field: u64) -> Option<String> {
        self.bytes(field)
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }
}

fn read_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut result: u64 = 0;
    let mut shift = 0;
    loop {
        let b = *buf.get(*pos)?;
        *pos += 1;
        result |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Some(result);
        }
        shift += 7;
        if shift >= 64 {
            return None; // malformed
        }
    }
}

/// Parse a flat protobuf message into fields keyed by field number.
/// Nested messages are stored as raw bytes (parse again if needed).
pub fn parse_fields(buf: &[u8]) -> Fields {
    let mut fields = Fields::default();
    let mut pos = 0;
    while pos < buf.len() {
        let Some(tag) = read_varint(buf, &mut pos) else {
            break;
        };
        let field = tag >> 3;
        let wire = tag & 0x7;
        match wire {
            0 => {
                let Some(v) = read_varint(buf, &mut pos) else {
                    break;
                };
                fields.varints.insert(field, v);
            }
            2 => {
                let Some(len) = read_varint(buf, &mut pos) else {
                    break;
                };
                let len = len as usize;
                if pos + len > buf.len() {
                    break;
                }
                fields.bytes.insert(field, buf[pos..pos + len].to_vec());
                pos += len;
            }
            5 => pos += 4, // fixed32
            1 => pos += 8, // fixed64
            _ => break,    // groups / unknown: stop
        }
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    /// Real INTERACT_WORD_V2 pb captured from room 286893 (2026-07).
    const SAMPLE: &str = "EgblvLEqKioiAQEoATCtwRE4vsno0gZAkJH+gPczYgB42LDvtc6TxeEYmgEAsgFcElQKBuW8sSoqKhJKaHR0cHM6Ly9pMS5oZHNsYi5jb20vYmZzL2ZhY2UvNWZiOWJlZGZlMjA4MjJiMDA3MGUzZGI5YzRhYzY4MDEwOTg2OGY0Ni5qcGciAggFMgC6AXoKSmh0dHBzOi8vaTAuaGRzbGIuY29tL2Jmcy9saXZlL2JiODg3MzQ1NThjNjM4M2E0Y2ZiNWZhMTZjOTc0OWQ1MjkwZDk1ZTgucG5nEirmm77nu4/mtLvot4Pov4fvvIzov5HmnJ/kuI7kvaDkupLliqjovoPlsJEYBMIBAA==";

    #[test]
    fn extracts_username_and_meta_from_real_pb() {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(SAMPLE)
            .unwrap();
        let f = parse_fields(&bytes);
        assert_eq!(f.string(2).as_deref(), Some("弱***"));
        assert_eq!(f.varint(5), Some(1)); // msg_type = enter
        assert_eq!(f.varint(6), Some(286893)); // room_id
        assert!(f.varint(7).unwrap() > 1_700_000_000); // plausible unix secs
    }

    #[test]
    fn varint_and_string_roundtrip() {
        // field 1 varint = 300 (0x08, 0xac 0x02), field 2 str "hi"
        let buf = [0x08, 0xac, 0x02, 0x12, 0x02, b'h', b'i'];
        let f = parse_fields(&buf);
        assert_eq!(f.varint(1), Some(300));
        assert_eq!(f.string(2).as_deref(), Some("hi"));
    }

    #[test]
    fn truncated_input_does_not_panic() {
        assert!(parse_fields(&[0x12, 0x05, b'a']).string(2).is_none());
        assert!(parse_fields(&[0x08]).varint(1).is_none());
        parse_fields(&[]);
    }
}
