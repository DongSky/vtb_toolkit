//! Bilibili account management: QR-code login, credential storage in the
//! OS keychain, and cookie injection for API clients.

pub mod error;
pub mod credentials;
pub mod qr;
pub mod store;

pub use credentials::Credentials;
pub use error::AccountError;
pub use qr::{QrLogin, QrPollState};
pub use store::{CredentialStore, KeyringStore, MemoryStore};

/// Browser-like UA shared by all bilibili requests.
pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36";

/// Build a reqwest client that sends the credentials' cookies on every
/// request (plus the standard UA). Pass `None` for an anonymous client.
pub fn build_client(creds: Option<&Credentials>) -> reqwest::Result<reqwest::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(c) = creds {
        if let Ok(v) = reqwest::header::HeaderValue::from_str(&c.cookie_header()) {
            headers.insert(reqwest::header::COOKIE, v);
        }
    }
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .build()
}
