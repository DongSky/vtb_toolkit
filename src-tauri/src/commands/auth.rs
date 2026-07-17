//! Account commands: QR login, status, logout.

use crate::state::AppState;
use tauri::State;
use vtb_account::{
    credentials::{fetch_buvid3, verify},
    CredentialStore, KeyringStore, QrLogin, QrPollState,
};

#[derive(serde::Serialize)]
pub struct QrStartResponse {
    /// SVG markup of the QR code, ready to inline in the UI.
    pub svg: String,
}

#[derive(serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum QrPollResponse {
    WaitingScan,
    WaitingConfirm,
    Confirmed { uname: String, mid: u64 },
    Expired,
}

#[derive(serde::Serialize)]
pub struct AuthStatus {
    pub logged_in: bool,
    pub uname: Option<String>,
    pub mid: Option<u64>,
}

#[tauri::command]
pub async fn auth_qr_start(state: State<'_, AppState>) -> Result<QrStartResponse, String> {
    let client = vtb_account::build_client(None).map_err(|e| e.to_string())?;
    let login = QrLogin::start(client).await.map_err(|e| e.to_string())?;
    let svg = login.qr_svg();
    *state.qr_login.lock().await = Some(login);
    Ok(QrStartResponse { svg })
}

#[tauri::command]
pub async fn auth_qr_poll(state: State<'_, AppState>) -> Result<QrPollResponse, String> {
    let guard = state.qr_login.lock().await;
    let login = guard.as_ref().ok_or("no QR session; call auth_qr_start")?;
    match login.poll().await.map_err(|e| e.to_string())? {
        QrPollState::WaitingScan => Ok(QrPollResponse::WaitingScan),
        QrPollState::WaitingConfirm => Ok(QrPollResponse::WaitingConfirm),
        QrPollState::Expired => Ok(QrPollResponse::Expired),
        QrPollState::Confirmed(mut creds) => {
            drop(guard);
            // Complete the profile: buvid3 + identity check, then persist.
            let anon = vtb_account::build_client(None).map_err(|e| e.to_string())?;
            creds.buvid3 = fetch_buvid3(&anon).await.ok();
            let authed =
                vtb_account::build_client(Some(&creds)).map_err(|e| e.to_string())?;
            let identity = verify(&authed).await.map_err(|e| e.to_string())?;
            KeyringStore::default()
                .save(&creds)
                .map_err(|e| e.to_string())?;
            *state.credentials.lock().unwrap() = Some(creds);
            *state.qr_login.lock().await = None;
            Ok(QrPollResponse::Confirmed {
                uname: identity.uname,
                mid: identity.mid,
            })
        }
    }
}

#[tauri::command]
pub async fn auth_status(state: State<'_, AppState>) -> Result<AuthStatus, String> {
    let Some(creds) = state.creds() else {
        return Ok(AuthStatus {
            logged_in: false,
            uname: None,
            mid: None,
        });
    };
    let client = vtb_account::build_client(Some(&creds)).map_err(|e| e.to_string())?;
    match verify(&client).await {
        Ok(id) => Ok(AuthStatus {
            logged_in: true,
            uname: Some(id.uname),
            mid: Some(id.mid),
        }),
        Err(_) => {
            // Session invalid/expired: clear it.
            let _ = KeyringStore::default().clear();
            *state.credentials.lock().unwrap() = None;
            Ok(AuthStatus {
                logged_in: false,
                uname: None,
                mid: None,
            })
        }
    }
}

#[tauri::command]
pub async fn auth_logout(state: State<'_, AppState>) -> Result<(), String> {
    KeyringStore::default().clear().map_err(|e| e.to_string())?;
    *state.credentials.lock().unwrap() = None;
    Ok(())
}
