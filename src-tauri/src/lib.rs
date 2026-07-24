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
        // 打点全局快捷键: works while the app is in the background (the
        // streamer is in OBS/game, the clipper in their editor).
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts(["CmdOrCtrl+Shift+M"])
                .expect("valid marker shortcut")
                .with_handler(|app, _shortcut, event| {
                    use tauri::Manager;
                    if event.state() != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        return;
                    }
                    let state = app.state::<AppState>();
                    let result = commands::markers::sole_active_room(&state.marker_dirs)
                        .and_then(|room_id| {
                            commands::markers::add_marker(
                                app,
                                &state.marker_dirs,
                                room_id,
                                vtb_pipeline::markers::Marker::manual("hotkey", None),
                            )
                        });
                    if let Err(e) = result {
                        tracing::info!("hotkey marker skipped: {e}");
                    }
                })
                .build(),
        )
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
            commands::review::edl_export,
            commands::markers::marker_add,
            commands::markers::marker_rooms,
            commands::markers::marker_timeline,
            commands::markers::timestamps_export,
            commands::editor::fcpxml_export,
            commands::editor::jianying_export,
            commands::editor::asset_bundle_export,
            commands::editor::proxy_generate,
            commands::cover::cover_extract_frames,
            commands::cover::cover_ideas,
            commands::cover::cover_generate,
            commands::cover::cover_list,
            commands::stats::stats_report,
            commands::stats::stats_export,
            commands::stats::user_note_set,
            commands::stats::user_notes_list,
            commands::stats::user_profile,
            commands::asr_models::asr_models,
            commands::asr_models::asr_model_download,
            commands::hotwords::hotwords_list,
            commands::hotwords::hotwords_get,
            commands::hotwords::hotwords_save_raw,
            commands::hotwords::hotwords_update,
            commands::hotwords::hotwords_delete,
            commands::hotwords::hotwords_merge,
            commands::tts::tts_set,
            commands::tts::tts_status,
            commands::upload::biliup_upload,
            commands::upload::biliup_status,
            commands::danmaku::danmaku_connect,
            commands::danmaku::danmaku_disconnect,
            commands::danmaku::danmaku_status,
            commands::twitch::twitch_connect,
            commands::twitch::twitch_disconnect,
            commands::twitch::twitch_status,
            commands::danmaku_translate::danmaku_translate_start,
            commands::danmaku_translate::danmaku_translate_stop,
            commands::danmaku_translate::danmaku_translate_status,
            commands::danmaku_send::danmaku_send,
            commands::danmaku_send::danmaku_autothank_set,
            commands::danmaku_send::danmaku_autothank_get,
            commands::danmaku_send::danmaku_timer_start,
            commands::danmaku_send::danmaku_timer_stop,
            commands::danmaku_send::danmaku_timer_status,
            commands::recorder::recorder_start,
            commands::recorder::recorder_stop,
            commands::recorder::recorder_status,
            commands::recorder::flv_repair,
            commands::platform_rec::platform_record_start,
            commands::platform_rec::platform_record_stop,
            commands::platform_rec::platform_record_status,
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
