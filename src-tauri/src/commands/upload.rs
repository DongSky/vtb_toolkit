//! biliup 投稿 integration: shells out to the `biliup` CLI when installed.

use std::path::{Path, PathBuf};

/// Whether the biliup CLI is on PATH (or at a custom path).
pub fn biliup_available(binary: &Path) -> bool {
    if binary.components().count() > 1 {
        return binary.exists();
    }
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(binary).exists()))
        .unwrap_or(false)
}

/// Build the `biliup upload` argument list (pure, testable).
pub fn build_upload_args(
    file: &Path,
    title: &str,
    tid: Option<u32>,
    tags: &[String],
    desc: Option<&str>,
) -> Vec<String> {
    let mut args = vec![
        "upload".to_string(),
        file.to_string_lossy().into_owned(),
        "--title".into(),
        title.to_string(),
    ];
    // 171 = 电子竞技? Use VTuber-friendly default 27 (综合/动画) left to
    // caller; only pass what's provided.
    if let Some(t) = tid {
        args.push("--tid".into());
        args.push(t.to_string());
    }
    if !tags.is_empty() {
        args.push("--tag".into());
        args.push(tags.join(","));
    }
    if let Some(d) = desc {
        args.push("--desc".into());
        args.push(d.to_string());
    }
    args
}

#[derive(serde::Serialize)]
pub struct UploadResult {
    pub ok: bool,
    pub output: String,
}

/// Upload a clip via biliup. Requires prior `biliup login` by the user.
#[tauri::command]
pub async fn biliup_upload(
    file: String,
    title: String,
    tid: Option<u32>,
    tags: Option<Vec<String>>,
    desc: Option<String>,
) -> Result<UploadResult, String> {
    let binary = PathBuf::from("biliup");
    if !biliup_available(&binary) {
        return Err(
            "未安装 biliup CLI（cargo install biliup 或从 GitHub 下载），并先执行 biliup login"
                .into(),
        );
    }
    let path = PathBuf::from(&file);
    if !path.exists() {
        return Err(format!("文件不存在: {file}"));
    }
    let args = build_upload_args(
        &path,
        &title,
        tid,
        &tags.unwrap_or_default(),
        desc.as_deref(),
    );
    let out = tokio::process::Command::new(&binary)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("biliup: {e}"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(UploadResult {
        ok: out.status.success(),
        output: text.chars().take(4000).collect(),
    })
}

#[tauri::command]
pub async fn biliup_status() -> Result<bool, String> {
    Ok(biliup_available(Path::new("biliup")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_args_built_correctly() {
        let args = build_upload_args(
            Path::new("/clips/c.mp4"),
            "【切片】超绝高能",
            Some(27),
            &["虚拟主播".to_string(), "切片".to_string()],
            Some("自动生成"),
        );
        assert_eq!(args[0], "upload");
        assert_eq!(args[1], "/clips/c.mp4");
        let title_i = args.iter().position(|a| a == "--title").unwrap();
        assert_eq!(args[title_i + 1], "【切片】超绝高能");
        let tag_i = args.iter().position(|a| a == "--tag").unwrap();
        assert_eq!(args[tag_i + 1], "虚拟主播,切片");
        assert!(args.contains(&"--tid".to_string()));
        assert!(args.contains(&"--desc".to_string()));
    }

    #[test]
    fn minimal_args_omit_optionals() {
        let args = build_upload_args(Path::new("/c.mp4"), "t", None, &[], None);
        assert!(!args.contains(&"--tid".to_string()));
        assert!(!args.contains(&"--tag".to_string()));
        assert!(!args.contains(&"--desc".to_string()));
    }
}
