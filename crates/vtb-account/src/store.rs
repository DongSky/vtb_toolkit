//! Credential persistence: OS keychain by default, in-memory for tests.

use crate::credentials::Credentials;
use crate::error::{AccountError, Result};

pub trait CredentialStore: Send + Sync {
    fn save(&self, creds: &Credentials) -> Result<()>;
    fn load(&self) -> Result<Option<Credentials>>;
    fn clear(&self) -> Result<()>;
}

/// Stores credentials as JSON in the OS keychain (macOS Keychain /
/// Windows Credential Manager / Secret Service on Linux).
pub struct KeyringStore {
    service: String,
    account: String,
}

impl Default for KeyringStore {
    fn default() -> Self {
        Self {
            service: "vtb-toolkit".into(),
            account: "bilibili".into(),
        }
    }
}

impl KeyringStore {
    fn entry(&self) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, &self.account)
            .map_err(|e| AccountError::Keyring(e.to_string()))
    }
}

impl CredentialStore for KeyringStore {
    fn save(&self, creds: &Credentials) -> Result<()> {
        let json = serde_json::to_string(creds)?;
        self.entry()?
            .set_password(&json)
            .map_err(|e| AccountError::Keyring(e.to_string()))
    }

    fn load(&self) -> Result<Option<Credentials>> {
        match self.entry()?.get_password() {
            Ok(json) => Ok(serde_json::from_str(&json).ok()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AccountError::Keyring(e.to_string())),
        }
    }

    fn clear(&self) -> Result<()> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AccountError::Keyring(e.to_string())),
        }
    }
}

/// Generic named secrets in the OS keychain (API keys etc.), separate from
/// the bilibili credential entry.
pub struct Secrets;

impl Secrets {
    const SERVICE: &'static str = "vtb-toolkit-secrets";

    fn entry(name: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(Self::SERVICE, name)
            .map_err(|e| AccountError::Keyring(e.to_string()))
    }

    pub fn set(name: &str, value: &str) -> Result<()> {
        Self::entry(name)?
            .set_password(value)
            .map_err(|e| AccountError::Keyring(e.to_string()))
    }

    pub fn get(name: &str) -> Result<Option<String>> {
        match Self::entry(name)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AccountError::Keyring(e.to_string())),
        }
    }

    pub fn delete(name: &str) -> Result<()> {
        match Self::entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AccountError::Keyring(e.to_string())),
        }
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct MemoryStore {
    inner: std::sync::Mutex<Option<Credentials>>,
}

impl CredentialStore for MemoryStore {
    fn save(&self, creds: &Credentials) -> Result<()> {
        *self.inner.lock().unwrap() = Some(creds.clone());
        Ok(())
    }

    fn load(&self) -> Result<Option<Credentials>> {
        Ok(self.inner.lock().unwrap().clone())
    }

    fn clear(&self) -> Result<()> {
        *self.inner.lock().unwrap() = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds() -> Credentials {
        Credentials {
            sessdata: "s".into(),
            bili_jct: "j".into(),
            dede_user_id: 1,
            buvid3: None,
            refresh_token: None,
        }
    }

    #[test]
    fn memory_store_roundtrip() {
        let store = MemoryStore::default();
        assert!(store.load().unwrap().is_none());
        store.save(&creds()).unwrap();
        assert_eq!(store.load().unwrap(), Some(creds()));
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
    }
}
