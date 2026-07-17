//! Login credentials and the cookie header they produce.

use crate::error::{AccountError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Credentials {
    pub sessdata: String,
    pub bili_jct: String,
    pub dede_user_id: u64,
    #[serde(default)]
    pub buvid3: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
}

impl Credentials {
    /// Build the Cookie header value sent with API requests.
    pub fn cookie_header(&self) -> String {
        let mut parts = vec![
            format!("SESSDATA={}", self.sessdata),
            format!("bili_jct={}", self.bili_jct),
            format!("DedeUserID={}", self.dede_user_id),
        ];
        if let Some(b) = &self.buvid3 {
            parts.push(format!("buvid3={b}"));
        }
        parts.join("; ")
    }

    /// Parse credentials from the `data.url` returned by a successful QR
    /// poll — a crossDomain URL whose query carries the cookie values:
    /// `...?DedeUserID=123&SESSDATA=xxx&bili_jct=yyy&...`
    pub fn from_crossdomain_url(url_str: &str, refresh_token: Option<String>) -> Result<Self> {
        let url = url::Url::parse(url_str)
            .map_err(|_| AccountError::MissingField("valid crossDomain url"))?;
        let mut sessdata = None;
        let mut bili_jct = None;
        let mut dede_user_id = None;
        for (k, v) in url.query_pairs() {
            match k.as_ref() {
                "SESSDATA" => sessdata = Some(v.into_owned()),
                "bili_jct" => bili_jct = Some(v.into_owned()),
                "DedeUserID" => dede_user_id = v.parse::<u64>().ok(),
                _ => {}
            }
        }
        Ok(Self {
            sessdata: sessdata.ok_or(AccountError::MissingField("SESSDATA"))?,
            bili_jct: bili_jct.ok_or(AccountError::MissingField("bili_jct"))?,
            dede_user_id: dede_user_id.ok_or(AccountError::MissingField("DedeUserID"))?,
            buvid3: None,
            refresh_token,
        })
    }
}

/// Fetch a fresh buvid3 fingerprint (needed by the danmaku auth packet).
pub async fn fetch_buvid3(client: &reqwest::Client) -> Result<String> {
    let v: serde_json::Value = client
        .get("https://api.bilibili.com/x/frontend/finger/spi")
        .send()
        .await?
        .json()
        .await?;
    v.get("data")
        .and_then(|d| d.get("b_3"))
        .and_then(|b| b.as_str())
        .map(str::to_string)
        .ok_or(AccountError::MissingField("data.b_3"))
}

/// Result of a `nav` identity check.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Identity {
    pub mid: u64,
    pub uname: String,
}

/// Verify credentials against the `nav` endpoint; returns the identity if
/// the session is valid.
pub async fn verify(client: &reqwest::Client) -> Result<Identity> {
    let v: serde_json::Value = client
        .get("https://api.bilibili.com/x/web-interface/nav")
        .header("Referer", "https://www.bilibili.com/")
        .send()
        .await?
        .json()
        .await?;
    let data = v.get("data").ok_or(AccountError::MissingField("data"))?;
    if !data.get("isLogin").and_then(|b| b.as_bool()).unwrap_or(false) {
        return Err(AccountError::NotLoggedIn);
    }
    Ok(Identity {
        mid: data.get("mid").and_then(|m| m.as_u64()).unwrap_or(0),
        uname: data
            .get("uname")
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_header_includes_all_parts() {
        let c = Credentials {
            sessdata: "sd".into(),
            bili_jct: "jct".into(),
            dede_user_id: 42,
            buvid3: Some("BV3".into()),
            refresh_token: None,
        };
        assert_eq!(
            c.cookie_header(),
            "SESSDATA=sd; bili_jct=jct; DedeUserID=42; buvid3=BV3"
        );
        let c2 = Credentials { buvid3: None, ..c };
        assert!(!c2.cookie_header().contains("buvid3"));
    }

    #[test]
    fn parses_crossdomain_url() {
        let url = "https://passport.biligame.com/crossDomain?DedeUserID=123\
                   &DedeUserID__ckMd5=abc&Expires=1600000000&SESSDATA=s%2Cd\
                   &bili_jct=tok&gourl=https%3A%2F%2Fwww.bilibili.com";
        let c = Credentials::from_crossdomain_url(url, Some("rt".into())).unwrap();
        assert_eq!(c.dede_user_id, 123);
        assert_eq!(c.sessdata, "s,d"); // percent-decoded
        assert_eq!(c.bili_jct, "tok");
        assert_eq!(c.refresh_token.as_deref(), Some("rt"));
    }

    #[test]
    fn missing_fields_error() {
        assert!(Credentials::from_crossdomain_url("https://x/?SESSDATA=a", None).is_err());
        assert!(Credentials::from_crossdomain_url("not a url", None).is_err());
    }

    #[test]
    fn roundtrip_json() {
        let c = Credentials {
            sessdata: "s".into(),
            bili_jct: "j".into(),
            dede_user_id: 7,
            buvid3: None,
            refresh_token: Some("r".into()),
        };
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<Credentials>(&json).unwrap(), c);
    }
}
