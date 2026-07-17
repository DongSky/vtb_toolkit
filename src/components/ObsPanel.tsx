import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface OverlayStatus {
  running: boolean;
  port: number | null;
  danmaku_url: string | null;
  subtitle_url: string | null;
  clients: number;
}

export default function ObsPanel() {
  const [status, setStatus] = useState<OverlayStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);

  const refresh = async () => {
    try {
      setStatus(await invoke<OverlayStatus>("overlay_status"));
    } catch {
      /* backend absent in browser dev */
    }
  };

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 3000);
    return () => clearInterval(t);
  }, []);

  const start = async () => {
    setError(null);
    try {
      setStatus(await invoke<OverlayStatus>("overlay_start", { port: null }));
    } catch (e) {
      setError(String(e));
    }
  };

  const stop = async () => {
    try {
      await invoke("overlay_stop");
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const copy = async (url: string, tag: string) => {
    await navigator.clipboard?.writeText(url);
    setCopied(tag);
    setTimeout(() => setCopied(null), 1500);
  };

  return (
    <div className="panel" data-testid="obs-panel">
      <h2>OBS 浏览器源输出</h2>
      <p className="login-note">
        启动本地叠加层服务后，把下面的 URL 添加为 OBS 的「浏览器」源即可。
        弹幕层建议宽 420×高 700；字幕条建议铺满底部（宽 1920×高 200）。
        可加参数调节：弹幕 <code>?size=20&width=480&lines=12</code>，字幕{" "}
        <code>?size=34&hold=8000</code>。
      </p>
      {status?.running ? (
        <div data-testid="obs-running">
          <p>
            运行中（端口 {status.port}，已连接页面 {status.clients}）
            <button data-testid="obs-stop" onClick={stop} style={{ marginLeft: 8 }}>
              停止
            </button>
          </p>
          <div className="form-col">
            <label>
              弹幕叠加层
              <div className="form-row">
                <input readOnly data-testid="obs-danmaku-url" value={status.danmaku_url ?? ""} style={{ flex: 1 }} />
                <button onClick={() => copy(status.danmaku_url!, "dm")}>
                  {copied === "dm" ? "已复制" : "复制"}
                </button>
              </div>
            </label>
            <label>
              同传字幕条
              <div className="form-row">
                <input readOnly data-testid="obs-subtitle-url" value={status.subtitle_url ?? ""} style={{ flex: 1 }} />
                <button onClick={() => copy(status.subtitle_url!, "sub")}>
                  {copied === "sub" ? "已复制" : "复制"}
                </button>
              </div>
            </label>
          </div>
        </div>
      ) : (
        <button data-testid="obs-start" onClick={start}>
          启动叠加层服务
        </button>
      )}
      {error && (
        <div className="error" data-testid="obs-error">
          {error}
        </div>
      )}
    </div>
  );
}
