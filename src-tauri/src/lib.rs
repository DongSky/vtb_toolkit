//! Tauri application shell: commands + event bridge over the domain crates.

mod state;
mod commands;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage({
            let state = AppState::default();
            // Restore login from the OS keychain at startup.
            use vtb_account::CredentialStore;
            if let Ok(Some(creds)) = vtb_account::KeyringStore::default().load() {
                tracing::info!("restored bilibili credentials for uid {}", creds.dede_user_id);
                *state.credentials.lock().unwrap() = Some(creds);
            }
            state
        })
        .invoke_handler(tauri::generate_handler![
            commands::auth::auth_qr_start,
            commands::auth::auth_qr_poll,
            commands::auth::auth_status,
            commands::auth::auth_logout,
            commands::config::config_load,
            commands::config::config_set,
            commands::config::secret_set,
            commands::config::secret_exists,
            commands::overlay::overlay_start,
            commands::overlay::overlay_stop,
            commands::overlay::overlay_status,
            commands::rooms::room_info,
            commands::rooms::platform_probe,
            commands::review::review_load,
            commands::review::clip_export,
            commands::stats::stats_report,
            commands::stats::stats_export,
            commands::asr_models::asr_models,
            commands::asr_models::asr_model_download,
            commands::tts::tts_set,
            commands::tts::tts_status,
            commands::upload::biliup_upload,
            commands::upload::biliup_status,
            commands::danmaku::danmaku_connect,
            commands::danmaku::danmaku_disconnect,
            commands::danmaku::danmaku_status,
            commands::recorder::recorder_start,
            commands::recorder::recorder_stop,
            commands::recorder::recorder_status,
            commands::pipeline::offline_process,
            commands::pipeline::offline_cancel,
            commands::subtitle::live_subtitle_start,
            commands::subtitle::live_subtitle_stop,
            commands::profile::profile_save,
            commands::profile::profile_load,
            commands::profile::profile_list,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
