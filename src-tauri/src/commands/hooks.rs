//! Script hooks (plugin MVP): run user scripts on lifecycle events with
//! the event payload as JSON on stdin — the same integration surface
//! BililiveRecorder exposes via webhooks, but local-first.

use serde::Serialize;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct HookEvent {
    /// e.g. "recording_started" / "recording_stopped" / "highlight".
    pub event: String,
    pub room_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Read the hooks map from settings: `{"hooks": {"recording_started":
/// "/path/to/script.sh", ...}}`.
pub fn hook_for(settings: &serde_json::Value, event: &str) -> Option<String> {
    settings
        .get("hooks")
        .and_then(|h| h.get(event))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Run one hook script: JSON payload on stdin, 30 s timeout, detached from
/// the caller (never blocks pipelines).
pub fn run_hook(script: String, payload: HookEvent) {
    tokio::spawn(async move {
        if !Path::new(&script).exists() {
            tracing::warn!("hook script missing: {script}");
            return;
        }
        let json = match serde_json::to_string(&payload) {
            Ok(j) => j,
            Err(_) => return,
        };
        let child = tokio::process::Command::new(&script)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("hook spawn failed ({script}): {e}");
                return;
            }
        };
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(json.as_bytes()).await;
            drop(stdin);
        }
        match tokio::time::timeout(Duration::from_secs(30), child.wait()).await {
            Ok(Ok(status)) if !status.success() => {
                tracing::warn!("hook {script} exited {status}");
            }
            Ok(Ok(_)) => {}
            Ok(Err(e)) => tracing::warn!("hook wait failed: {e}"),
            Err(_) => {
                tracing::warn!("hook {script} timed out (30s), killing");
                let _ = child.start_kill();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_lookup_from_settings() {
        let settings = serde_json::json!({
            "hooks": {
                "recording_started": "/hooks/start.sh",
                "recording_stopped": "",
            }
        });
        assert_eq!(
            hook_for(&settings, "recording_started").as_deref(),
            Some("/hooks/start.sh")
        );
        // Empty string = unset.
        assert!(hook_for(&settings, "recording_stopped").is_none());
        assert!(hook_for(&settings, "highlight").is_none());
        assert!(hook_for(&serde_json::json!({}), "recording_started").is_none());
    }

    #[tokio::test]
    async fn hook_receives_json_on_stdin() {
        let dir = tempfile::tempdir().unwrap();
        let out_file = dir.path().join("received.json");
        let script = dir.path().join("hook.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\ncat > {}\n", out_file.display()),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }

        run_hook(
            script.to_string_lossy().into_owned(),
            HookEvent {
                event: "recording_stopped".into(),
                room_id: 24158116,
                path: Some("/rec/session".into()),
                message: None,
            },
        );

        // Poll for the file (hook runs detached).
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if out_file.exists() {
                let text = std::fs::read_to_string(&out_file).unwrap();
                if !text.is_empty() {
                    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
                    assert_eq!(v["event"], "recording_stopped");
                    assert_eq!(v["room_id"], 24158116);
                    assert_eq!(v["path"], "/rec/session");
                    break;
                }
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "hook never wrote output"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    #[tokio::test]
    async fn missing_script_is_harmless() {
        run_hook(
            "/definitely/not/a/script.sh".into(),
            HookEvent {
                event: "x".into(),
                room_id: 1,
                path: None,
                message: None,
            },
        );
        tokio::time::sleep(Duration::from_millis(100)).await; // no panic
    }
}
