use crate::state::AppState;
use serde_json::{json, Value};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};
use vtb_account::{
    youtube::{Authorization, ClientConfig, UploadOptions},
    Secrets,
};

#[derive(Default)]
pub struct UploadState {
    auth: Mutex<Option<tokio::task::JoinHandle<()>>>,
    upload: Mutex<Option<tokio::task::JoinHandle<()>>>,
    progress: std::sync::Arc<Mutex<Value>>,
}
fn config() -> Result<ClientConfig, String> {
    let value = Secrets::get("youtube-upload-client")
        .map_err(|e| e.to_string())?
        .ok_or("请先配置 Google 桌面应用 OAuth 客户端")?;
    serde_json::from_str(&value).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn youtube_upload_status(state: State<'_, AppState>) -> Result<Value, String> {
    Ok(
        json!({"configured":Secrets::get("youtube-upload-client").map_err(|e|e.to_string())?.is_some(),
        "authorized":Secrets::get("youtube-upload-refresh").map_err(|e|e.to_string())?.is_some(),
        "authorizing":state.youtube_upload.auth.lock().unwrap().as_ref().is_some_and(|t|!t.is_finished()),
        "uploading":state.youtube_upload.upload.lock().unwrap().as_ref().is_some_and(|t|!t.is_finished()),
        "progress":state.youtube_upload.progress.lock().unwrap().clone()}),
    )
}

#[tauri::command]
pub fn youtube_upload_configure(
    state: State<'_, AppState>,
    client: ClientConfig,
) -> Result<(), String> {
    client.validate()?;
    if state
        .youtube_upload
        .upload
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|t| !t.is_finished())
    {
        return Err("请先取消或完成上传".into());
    }
    if let Some(task) = state.youtube_upload.auth.lock().unwrap().take() {
        task.abort();
    }
    Secrets::delete("youtube-upload-refresh").map_err(|e| e.to_string())?;
    Secrets::set(
        "youtube-upload-client",
        &serde_json::to_string(&client).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn youtube_upload_authorize(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let config = config()?;
    let authorization = Authorization::new(&config).await?;
    let url = authorization.url.clone();
    let mut auth = state.youtube_upload.auth.lock().unwrap();
    if let Some(task) = auth.take() {
        task.abort();
    }
    let progress = state.youtube_upload.progress.clone();
    *auth = Some(tokio::spawn(async move {
        let result = authorization.finish(&config).await.and_then(|refresh| {
            Secrets::set("youtube-upload-refresh", &refresh).map_err(|e| e.to_string())
        });
        let status = match result {
            Ok(()) => json!({"state":"authorized","message":"YouTube 投稿授权成功"}),
            Err(e) => json!({"state":"error","message":e}),
        };
        *progress.lock().unwrap() = status.clone();
        let _ = app.emit("youtube-upload://status", status);
    }));
    Ok(url)
}

#[tauri::command]
pub fn youtube_upload_cancel(app: AppHandle, state: State<'_, AppState>) {
    if let Some(task) = state.youtube_upload.auth.lock().unwrap().take() {
        task.abort();
    }
    if let Some(task) = state.youtube_upload.upload.lock().unwrap().take() {
        task.abort();
    }
    let status = json!({"state":"cancelled","message":"本地传输已取消；如已传完，请在 YouTube Studio 核实视频状态"});
    *state.youtube_upload.progress.lock().unwrap() = status.clone();
    let _ = app.emit("youtube-upload://status", status);
}

#[tauri::command]
pub fn youtube_upload_logout(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    youtube_upload_cancel(app, state);
    Secrets::delete("youtube-upload-refresh").map_err(|e| e.to_string())
}

#[tauri::command]
pub fn youtube_upload_start(
    app: AppHandle,
    state: State<'_, AppState>,
    options: UploadOptions,
) -> Result<(), String> {
    options.validate()?;
    let config = config()?;
    let refresh = Secrets::get("youtube-upload-refresh")
        .map_err(|e| e.to_string())?
        .ok_or("请先在账号页授权 YouTube 投稿")?;
    let mut upload = state.youtube_upload.upload.lock().unwrap();
    if upload.as_ref().is_some_and(|t| !t.is_finished()) {
        return Err("已有视频正在上传".into());
    }
    let progress = state.youtube_upload.progress.clone();
    *progress.lock().unwrap() = json!({"state":"uploading","sent":0,"total":0});
    *upload = Some(tokio::spawn(async move {
        let result = async {
            let token = vtb_account::youtube::access_token(&config, &refresh).await?;
            vtb_account::youtube::upload(&token, &options, |sent, total| {
                let value = json!({"state":"uploading","sent":sent,"total":total});
                *progress.lock().unwrap() = value.clone();
                let _ = app.emit("youtube-upload://status", value);
            })
            .await
        }
        .await;
        let value = match result {
            Ok(id) => json!({"state":"done","video_id":id,"message":"YouTube 上传完成"}),
            Err(e) => json!({"state":"error","message":e}),
        };
        *progress.lock().unwrap() = value.clone();
        let _ = app.emit("youtube-upload://status", value);
    }));
    Ok(())
}
