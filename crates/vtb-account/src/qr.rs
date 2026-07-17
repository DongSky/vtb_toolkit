//! QR-code login flow.
//!
//! 1. [`QrLogin::start`] → get a login URL + key; render the URL as a QR
//!    code for the user to scan with the bilibili app.
//! 2. Poll [`QrLogin::poll`] every ~2s until `Confirmed` (or `Expired`).
//!
//! Endpoints (verified live 2026-07-17):
//! - GET passport.bilibili.com/x/passport-login/web/qrcode/generate
//! - GET passport.bilibili.com/x/passport-login/web/qrcode/poll?qrcode_key=
//!   data.code: 0 success / 86101 not scanned / 86090 scanned, waiting /
//!   86038 expired

use crate::credentials::Credentials;
use crate::error::{AccountError, Result};
use serde::Deserialize;

const GENERATE: &str = "https://passport.bilibili.com/x/passport-login/web/qrcode/generate";
const POLL: &str = "https://passport.bilibili.com/x/passport-login/web/qrcode/poll";

#[derive(Debug, Clone, PartialEq)]
pub enum QrPollState {
    /// Waiting for the user to scan.
    WaitingScan,
    /// Scanned; waiting for in-app confirmation.
    WaitingConfirm,
    /// Login complete.
    Confirmed(Credentials),
    /// QR expired; start over.
    Expired,
}

pub struct QrLogin {
    client: reqwest::Client,
    pub qrcode_key: String,
    /// The URL to encode as a QR code.
    pub login_url: String,
}

#[derive(Deserialize)]
struct Envelope<T> {
    code: i64,
    #[serde(default)]
    message: String,
    data: Option<T>,
}

#[derive(Deserialize)]
struct GenerateData {
    url: String,
    qrcode_key: String,
}

#[derive(Deserialize)]
struct PollData {
    #[serde(default)]
    url: String,
    #[serde(default)]
    refresh_token: String,
    code: i64,
}

impl QrLogin {
    /// Request a fresh QR login session.
    pub async fn start(client: reqwest::Client) -> Result<Self> {
        let body = client.get(GENERATE).send().await?.text().await?;
        let env: Envelope<GenerateData> = serde_json::from_str(&body)?;
        if env.code != 0 {
            return Err(AccountError::Api {
                code: env.code,
                message: env.message,
            });
        }
        let data = env.data.ok_or(AccountError::MissingField("data"))?;
        Ok(Self {
            client,
            qrcode_key: data.qrcode_key,
            login_url: data.url,
        })
    }

    /// Render the login URL as an SVG QR code (for direct display in UI).
    pub fn qr_svg(&self) -> String {
        use qrcode::render::svg;
        use qrcode::QrCode;
        let code = QrCode::new(self.login_url.as_bytes()).expect("url encodes");
        code.render::<svg::Color>()
            .min_dimensions(240, 240)
            .quiet_zone(true)
            .build()
    }

    /// Poll once for the current state.
    pub async fn poll(&self) -> Result<QrPollState> {
        let body = self
            .client
            .get(POLL)
            .query(&[("qrcode_key", &self.qrcode_key)])
            .send()
            .await?
            .text()
            .await?;
        parse_poll(&body)
    }
}

/// Parse a poll response body (separated for testability).
pub fn parse_poll(body: &str) -> Result<QrPollState> {
    let env: Envelope<PollData> = serde_json::from_str(body)?;
    if env.code != 0 {
        return Err(AccountError::Api {
            code: env.code,
            message: env.message,
        });
    }
    let data = env.data.ok_or(AccountError::MissingField("data"))?;
    match data.code {
        0 => {
            let refresh = (!data.refresh_token.is_empty()).then_some(data.refresh_token);
            let creds = Credentials::from_crossdomain_url(&data.url, refresh)?;
            Ok(QrPollState::Confirmed(creds))
        }
        86101 => Ok(QrPollState::WaitingScan),
        86090 => Ok(QrPollState::WaitingConfirm),
        86038 => Ok(QrPollState::Expired),
        other => Err(AccountError::Api {
            code: other,
            message: format!("unexpected poll code {other}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_states() {
        let waiting = r#"{"code":0,"data":{"url":"","refresh_token":"","timestamp":0,"code":86101,"message":"未扫码"}}"#;
        assert_eq!(parse_poll(waiting).unwrap(), QrPollState::WaitingScan);

        let scanned = r#"{"code":0,"data":{"url":"","refresh_token":"","timestamp":0,"code":86090,"message":"已扫码"}}"#;
        assert_eq!(parse_poll(scanned).unwrap(), QrPollState::WaitingConfirm);

        let expired = r#"{"code":0,"data":{"url":"","refresh_token":"","timestamp":0,"code":86038,"message":"过期"}}"#;
        assert_eq!(parse_poll(expired).unwrap(), QrPollState::Expired);
    }

    #[test]
    fn poll_success_extracts_credentials() {
        let ok = r#"{"code":0,"data":{
            "url":"https://passport.biligame.com/crossDomain?DedeUserID=99&SESSDATA=abc&bili_jct=tok&gourl=x",
            "refresh_token":"RT","timestamp":1700000000,"code":0,"message":""}}"#;
        match parse_poll(ok).unwrap() {
            QrPollState::Confirmed(c) => {
                assert_eq!(c.dede_user_id, 99);
                assert_eq!(c.sessdata, "abc");
                assert_eq!(c.bili_jct, "tok");
                assert_eq!(c.refresh_token.as_deref(), Some("RT"));
            }
            other => panic!("expected Confirmed, got {other:?}"),
        }
    }

    #[test]
    fn outer_error_code_propagates() {
        let err = r#"{"code":-412,"message":"request blocked","data":null}"#;
        assert!(matches!(
            parse_poll(err),
            Err(AccountError::Api { code: -412, .. })
        ));
    }

    #[tokio::test]
    async fn qr_svg_renders() {
        // Build QrLogin without network by constructing fields directly.
        let login = QrLogin {
            client: reqwest::Client::new(),
            qrcode_key: "k".into(),
            login_url: "https://example.com/login?key=abc".into(),
        };
        let svg = login.qr_svg();
        assert!(svg.starts_with("<?xml"));
        assert!(svg.contains("<svg"));
    }
}
