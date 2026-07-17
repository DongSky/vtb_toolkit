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
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
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
