//! WBI request signing for Bilibili web APIs.
//!
//! Since ~2024 many endpoints (incl. `getDanmuInfo` from 2025-07) reject
//! unsigned requests with code -352. The scheme:
//!
//! 1. Fetch `nav` → `data.wbi_img.img_url` / `sub_url`; the two file names
//!    (sans extension) concatenated form a 64-char raw key.
//! 2. Shuffle the raw key through a fixed index table, keep 32 chars →
//!    the "mixin key". Rotates roughly daily; cache with a TTL.
//! 3. To sign params: add `wts` (unix seconds), sort keys, percent-encode
//!    (RFC3986, with `!'()*` stripped from values), then
//!    `w_rid = md5(query + mixin_key)`.
//!
//! Reference: bilibili-API-collect docs/misc/sign/wbi.md

use crate::error::{DanmakuError, Result};
use md5::{Digest, Md5};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The fixed shuffle table (from bilibili's JS, stable for years).
const MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49,
    33, 9, 42, 19, 29, 28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40,
    61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25, 54, 21, 56, 59, 6, 63, 57, 62, 11,
    36, 20, 34, 44, 52,
];

const NAV_URL: &str = "https://api.bilibili.com/x/web-interface/nav";

/// Extract the key file name (without dirs/extension) from a wbi_img URL.
pub fn key_from_url(url: &str) -> &str {
    let file = url.rsplit('/').next().unwrap_or(url);
    file.split('.').next().unwrap_or(file)
}

/// Derive the 32-char mixin key from img_key + sub_key.
pub fn mixin_key(img_key: &str, sub_key: &str) -> String {
    let raw: Vec<char> = format!("{img_key}{sub_key}").chars().collect();
    MIXIN_KEY_ENC_TAB
        .iter()
        .filter_map(|&i| raw.get(i))
        .take(32)
        .collect()
}

/// Percent-encode one query component per WBI rules (RFC3986 unreserved
/// kept; `!'()*` are removed from the value entirely).
fn encode_component(s: &str) -> String {
    let filtered: String = s.chars().filter(|c| !matches!(c, '!' | '\'' | '(' | ')' | '*')).collect();
    let mut out = String::with_capacity(filtered.len() * 3);
    for byte in filtered.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Sign `params` with the given mixin key and timestamp. Returns the final
/// query string including `wts` and `w_rid`.
pub fn sign_query(params: &[(&str, String)], mixin: &str, wts: u64) -> String {
    let mut all: Vec<(String, String)> = params
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    all.push(("wts".into(), wts.to_string()));
    all.sort_by(|a, b| a.0.cmp(&b.0));

    let query = all
        .iter()
        .map(|(k, v)| format!("{}={}", encode_component(k), encode_component(v)))
        .collect::<Vec<_>>()
        .join("&");

    let mut hasher = Md5::new();
    hasher.update(query.as_bytes());
    hasher.update(mixin.as_bytes());
    let w_rid = hex::encode(hasher.finalize());

    format!("{query}&w_rid={w_rid}")
}

/// Caches the mixin key with a TTL and signs queries.
pub struct WbiSigner {
    http: reqwest::Client,
    cached: Option<(String, Instant)>,
    ttl: Duration,
}

impl WbiSigner {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            cached: None,
            ttl: Duration::from_secs(3600 * 6),
        }
    }

    /// Inject a known mixin key (tests / offline).
    pub fn with_fixed_key(mut self, key: impl Into<String>) -> Self {
        self.cached = Some((key.into(), Instant::now()));
        self
    }

    pub async fn get_mixin_key(&mut self) -> Result<String> {
        if let Some((key, at)) = &self.cached {
            if at.elapsed() < self.ttl {
                return Ok(key.clone());
            }
        }
        let body: serde_json::Value = self
            .http
            .get(NAV_URL)
            .header("Referer", "https://www.bilibili.com/")
            .send()
            .await?
            .json()
            .await?;
        let wbi = body
            .get("data")
            .and_then(|d| d.get("wbi_img"))
            .ok_or(DanmakuError::MissingField("wbi_img"))?;
        let img = wbi
            .get("img_url")
            .and_then(|v| v.as_str())
            .ok_or(DanmakuError::MissingField("img_url"))?;
        let sub = wbi
            .get("sub_url")
            .and_then(|v| v.as_str())
            .ok_or(DanmakuError::MissingField("sub_url"))?;
        let key = mixin_key(key_from_url(img), key_from_url(sub));
        self.cached = Some((key.clone(), Instant::now()));
        Ok(key)
    }

    /// Sign params now (fetching/caching the key as needed).
    pub async fn sign(&mut self, params: &[(&str, String)]) -> Result<String> {
        let key = self.get_mixin_key().await?;
        let wts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Ok(sign_query(params, &key, wts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_extraction_from_url() {
        assert_eq!(
            key_from_url("https://i0.hdslb.com/bfs/wbi/7cd084941338484aae1ad9425b84077c.png"),
            "7cd084941338484aae1ad9425b84077c"
        );
        assert_eq!(key_from_url("abc.png"), "abc");
        assert_eq!(key_from_url("noext"), "noext");
    }

    #[test]
    fn mixin_key_is_32_chars() {
        let img = "7cd084941338484aae1ad9425b84077c";
        let sub = "4932caff0ff746eab6f01bf08b70ac45";
        let key = mixin_key(img, sub);
        assert_eq!(key.len(), 32);
        // Known-good value from the bilibili-API-collect reference docs.
        assert_eq!(key, "ea1db124af3c7062474693fa704f4ff8");
    }

    #[test]
    fn sign_query_reference_vector() {
        // Reference example from bilibili-API-collect wbi.md.
        let mixin = "ea1db124af3c7062474693fa704f4ff8";
        let params = [
            ("foo", "114".to_string()),
            ("bar", "514".to_string()),
            ("zab", "1919810".to_string()),
        ];
        let signed = sign_query(&params, mixin, 1702204169);
        assert_eq!(
            signed,
            "bar=514&foo=114&wts=1702204169&zab=1919810&w_rid=8f6f2b5b3d485fe1886cec6a0be8c5d4"
        );
    }

    #[test]
    fn special_chars_are_filtered_and_encoded() {
        let mixin = "ea1db124af3c7062474693fa704f4ff8";
        let params = [("q", "a!'()*b c".to_string())];
        let signed = sign_query(&params, mixin, 1);
        // !'()* removed, space percent-encoded.
        assert!(signed.contains("q=ab%20c"));
        assert!(signed.contains("w_rid="));
    }

    #[test]
    fn params_sorted_by_key() {
        let mixin = "k";
        let params = [("z", "1".into()), ("a", "2".into()), ("m", "3".into())];
        let signed = sign_query(&params, mixin, 10);
        let a = signed.find("a=2").unwrap();
        let m = signed.find("m=3").unwrap();
        let z = signed.find("z=1").unwrap();
        assert!(a < m && m < z);
    }
}
