//! 记录簿 / 场次报告 commands.

use tauri::AppHandle;

/// Fetch the session report for a recording session directory. Accepts
/// either the session dir itself or its `out/` analysis subdirectory.
#[tauri::command]
pub async fn stats_report(
    app: AppHandle,
    session_dir: String,
) -> Result<vtb_stats::SessionReport, String> {
    let db_path = super::recorder::stats_db_path(&app).ok_or("no data dir")?;
    let db = vtb_stats::StatsDb::open(&db_path).map_err(|e| e.to_string())?;

    let mut id = session_dir.trim_end_matches('/').to_string();
    let report = db.session_report(&id).map_err(|e| e.to_string())?;
    if report.danmaku_count == 0 && id.ends_with("/out") {
        id = id.trim_end_matches("/out").to_string();
        return db.session_report(&id).map_err(|e| e.to_string());
    }
    Ok(report)
}

/// Export the markdown report + SC CSV into the session directory.
#[tauri::command]
pub async fn stats_export(
    app: AppHandle,
    session_dir: String,
) -> Result<Vec<String>, String> {
    let report = stats_report(app, session_dir.clone()).await?;
    let dir = std::path::PathBuf::from(
        session_dir
            .trim_end_matches('/')
            .trim_end_matches("/out")
            .to_string(),
    );
    let md = dir.join("report.md");
    std::fs::write(&md, vtb_stats::export_markdown(&report)).map_err(|e| e.to_string())?;
    let csv = dir.join("superchats.csv");
    std::fs::write(&csv, vtb_stats::export_superchats_csv(&report))
        .map_err(|e| e.to_string())?;
    Ok(vec![
        md.to_string_lossy().into_owned(),
        csv.to_string_lossy().into_owned(),
    ])
}
