import { t, tm, useLocale } from "../i18n";
import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface AuthStatus {
  logged_in: boolean;
  uname: string | null;
  mid: number | null;
}

type QrPoll =
  | { state: "waiting_scan" }
  | { state: "waiting_confirm" }
  | { state: "confirmed"; uname: string; mid: number }
  | { state: "expired" };

export default function LoginPanel() {
  useLocale();
  const [status, setStatus] = useState<AuthStatus | null>(null);
  const [qrSvg, setQrSvg] = useState<string | null>(null);
  const [hint, setHint] = useState("");
  const [error, setError] = useState<string | null>(null);
  const pollTimer = useRef<ReturnType<typeof setInterval> | null>(null);

  const stopPolling = useCallback(() => {
    if (pollTimer.current) {
      clearInterval(pollTimer.current);
      pollTimer.current = null;
    }
  }, []);

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await invoke<AuthStatus>("auth_status"));
    } catch {
      /* backend absent in browser dev */
    }
  }, []);

  useEffect(() => {
    refreshStatus();
    return stopPolling;
  }, [refreshStatus, stopPolling]);

  const startLogin = async () => {
    setError(null);
    setHint("请用B站手机客户端扫码");
    try {
      const resp = await invoke<{ svg: string }>("auth_qr_start");
      setQrSvg(resp.svg);
      stopPolling();
      pollTimer.current = setInterval(async () => {
        try {
          const poll = await invoke<QrPoll>("auth_qr_poll");
          if (poll.state === "waiting_confirm") {
            setHint("已扫码，请在手机上确认登录");
          } else if (poll.state === "confirmed") {
            stopPolling();
            setQrSvg(null);
            setHint("");
            await refreshStatus();
          } else if (poll.state === "expired") {
            stopPolling();
            setQrSvg(null);
            setHint("二维码已过期，请重新获取");
          }
        } catch (e) {
          stopPolling();
          setError(String(e));
        }
      }, 2000);
    } catch (e) {
      setError(String(e));
    }
  };

  const logout = async () => {
    try {
      await invoke("auth_logout");
      await refreshStatus();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="panel" data-testid="login-panel">
      <h2>{t("账号")}</h2>
      {status?.logged_in ? (
        <div data-testid="login-info">
          <p>
            {t("已登录：")}<b>{status.uname}</b>（UID {status.mid}{t("）")}{" "}</p>
          <p className="login-note">
            {t("登录后弹幕显示完整用户名，录制可用原画画质。凭据保存在系统钥匙串。")}{" "}</p>
          <button data-testid="logout-btn" onClick={logout}>
            {t("退出登录")}{" "}</button>
        </div>
      ) : (
        <div data-testid="login-form">
          <p className="login-note">
            {t("未登录。匿名模式下弹幕用户名会被打码，录制画质受限。")}{" "}</p>
          <button data-testid="qr-start" onClick={startLogin}>
            {t("获取登录二维码")}{" "}</button>
          {qrSvg && (
            <div
              className="qr-box"
              data-testid="qr-box"
              dangerouslySetInnerHTML={{ __html: qrSvg }}
            />
          )}
          {hint && <p data-testid="qr-hint">{tm(hint)}</p>}
        </div>
      )}
      {error && (
        <div className="error" data-testid="login-error">
          {tm(error)}
        </div>
      )}
    </div>
  );
}
