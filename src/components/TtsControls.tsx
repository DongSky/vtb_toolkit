import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/** Danmaku TTS controls (macOS `say`). Reads chat aloud for solo streamers. */
export default function TtsControls() {
  const [enabled, setEnabled] = useState(false);
  const [paidOnly, setPaidOnly] = useState(false);

  useEffect(() => {
    Promise.resolve(invoke<{ enabled: boolean; paid_only: boolean }>("tts_status"))
      .then((s) => {
        setEnabled(Boolean(s?.enabled));
        setPaidOnly(Boolean(s?.paid_only));
      })
      .catch(() => {});
  }, []);

  const apply = async (e: boolean, p: boolean) => {
    setEnabled(e);
    setPaidOnly(p);
    try {
      await invoke("tts_set", { enabled: e, paidOnly: p });
    } catch {
      /* browser dev */
    }
  };

  return (
    <details className="tts-controls" data-testid="tts-controls">
      <summary>弹幕语音朗读（TTS）</summary>
      <div className="form-col" style={{ marginTop: 8 }}>
        <label>
          <input
            type="checkbox"
            data-testid="tts-enabled"
            checked={enabled}
            onChange={(e) => apply(e.target.checked, paidOnly)}
          />
          开启弹幕朗读（连接弹幕后生效）
        </label>
        <label>
          <input
            type="checkbox"
            data-testid="tts-paid-only"
            checked={paidOnly}
            disabled={!enabled}
            onChange={(e) => apply(enabled, e.target.checked)}
          />
          只朗读 SC / 礼物 / 上舰
        </label>
        <p className="login-note">仅 macOS 支持（使用系统 say 命令）。</p>
      </div>
    </details>
  );
}
