//! Whisper model catalog + download management.

use crate::error::{AsrError, Result};
use std::path::PathBuf;

/// A known whisper.cpp ggml model.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelInfo {
    /// Short name, e.g. "small".
    pub name: &'static str,
    /// File name under the models dir.
    pub file: &'static str,
    pub url: &'static str,
    /// Approximate download size in MB (display only).
    pub size_mb: u64,
    /// One-line guidance.
    pub note: &'static str,
}

pub const CATALOG: &[ModelInfo] = &[
    ModelInfo {
        name: "tiny",
        file: "ggml-tiny.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
        size_mb: 75,
        note: "最快，质量最低，适合冒烟测试",
    },
    ModelInfo {
        name: "base",
        file: "ggml-base.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        size_mb: 142,
        note: "快，日常低配",
    },
    ModelInfo {
        name: "small",
        file: "ggml-small.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        size_mb: 466,
        note: "速度/质量均衡，推荐实时字幕",
    },
    ModelInfo {
        name: "medium",
        file: "ggml-medium.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
        size_mb: 1500,
        note: "质量好，实时需较强机器",
    },
    ModelInfo {
        name: "large-v3-turbo",
        file: "ggml-large-v3-turbo.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
        size_mb: 1620,
        note: "质量最好且比 large-v3 快，推荐离线处理",
    },
];

/// Models directory: `$VTB_MODELS_DIR` override, else `~/.cache/vtb-toolkit`.
pub fn models_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("VTB_MODELS_DIR") {
        return PathBuf::from(dir);
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".cache/vtb-toolkit"))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Catalog entry + local state.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelStatus {
    pub name: String,
    pub size_mb: u64,
    pub note: String,
    pub downloaded: bool,
    pub path: String,
}

pub fn list_models() -> Vec<ModelStatus> {
    let dir = models_dir();
    CATALOG
        .iter()
        .map(|m| {
            let path = dir.join(m.file);
            ModelStatus {
                name: m.name.to_string(),
                size_mb: m.size_mb,
                note: m.note.to_string(),
                downloaded: path.exists()
                    && std::fs::metadata(&path).map(|md| md.len() > 1_000_000).unwrap_or(false),
                path: path.to_string_lossy().into_owned(),
            }
        })
        .collect()
}

pub fn find(name: &str) -> Option<&'static ModelInfo> {
    CATALOG.iter().find(|m| m.name == name)
}

/// Download a model with progress callbacks `(downloaded_bytes, total)`.
/// Writes to a `.part` file and renames on completion, so an interrupted
/// download never looks like a valid model.
pub async fn download_model(
    name: &str,
    mut progress: impl FnMut(u64, Option<u64>) + Send,
) -> Result<PathBuf> {
    let info = find(name).ok_or_else(|| AsrError::Config(format!("unknown model {name}")))?;
    let dir = models_dir();
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(info.file);
    if dest.exists() {
        return Ok(dest);
    }
    let part = dir.join(format!("{}.part", info.file));

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| AsrError::Config(e.to_string()))?;
    let resp = client
        .get(info.url)
        .send()
        .await
        .map_err(|e| AsrError::Config(format!("download: {e}")))?;
    if !resp.status().is_success() {
        return Err(AsrError::Config(format!("download HTTP {}", resp.status())));
    }
    let total = resp.content_length();

    use std::io::Write;
    let mut file = std::fs::File::create(&part)?;
    let mut stream = resp;
    let mut downloaded: u64 = 0;
    loop {
        match stream
            .chunk()
            .await
            .map_err(|e| AsrError::Config(format!("download: {e}")))?
        {
            Some(chunk) => {
                file.write_all(&chunk)?;
                downloaded += chunk.len() as u64;
                progress(downloaded, total);
            }
            None => break,
        }
    }
    file.flush()?;
    drop(file);
    std::fs::rename(&part, &dest)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_sane() {
        assert!(CATALOG.len() >= 4);
        for m in CATALOG {
            assert!(m.url.starts_with("https://huggingface.co/"));
            assert!(m.url.ends_with(m.file));
            assert!(m.size_mb > 10);
        }
        // Names unique.
        let mut names: Vec<_> = CATALOG.iter().map(|m| m.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), CATALOG.len());
        assert!(find("small").is_some());
        assert!(find("nope").is_none());
    }

    #[test]
    fn list_reflects_local_files() {
        let dir = tempfile::tempdir().unwrap();
        // Env-var override is process-global: guard with a lock-free
        // convention — this is the only test setting it.
        std::env::set_var("VTB_MODELS_DIR", dir.path());
        let before = list_models();
        assert!(before.iter().all(|m| !m.downloaded));

        // A too-small file is not "downloaded" (guards .part corruption).
        std::fs::write(dir.path().join("ggml-tiny.bin"), b"tiny").unwrap();
        let small = list_models();
        assert!(!small.iter().find(|m| m.name == "tiny").unwrap().downloaded);

        // A plausible-size file counts.
        let big = vec![0u8; 2_000_000];
        std::fs::write(dir.path().join("ggml-tiny.bin"), &big).unwrap();
        let after = list_models();
        let tiny = after.iter().find(|m| m.name == "tiny").unwrap();
        assert!(tiny.downloaded);
        assert!(tiny.path.ends_with("ggml-tiny.bin"));
        std::env::remove_var("VTB_MODELS_DIR");
    }
}
