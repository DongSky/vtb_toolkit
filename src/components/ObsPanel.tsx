import { t, t as translate, tm, useLocale } from "../i18n";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { usePersisted } from "../hooks/usePersisted";
import { allThemes } from "../themes";

interface OverlayStatus {
  running: boolean;
  port: number | null;
  danmaku_url: string | null;
  subtitle_url: string | null;
  clients: number;
}

export default function ObsPanel() {
  const locale = useLocale();
  const [status, setStatus] = useState<OverlayStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [port, setPort] = usePersisted("obs.port", "18990");
  const [platform, setPlatform] = usePersisted("obs.platform", "all");
  const [room, setRoom] = usePersisted("obs.room", "");
  const [size, setSize] = usePersisted("obs.size", "20");
  const [width, setWidth] = usePersisted("obs.width", "420");
  const [lines, setLines] = usePersisted("obs.lines", "14");
  const [hold, setHold] = usePersisted("obs.hold", "0");
  const [themeId, setThemeId] = usePersisted("obs.theme", "builtin:obs-overlay");
  const [avatars, setAvatars] = usePersisted("obs.avatars", true);
  const [badges, setBadges] = usePersisted("obs.badges", true);
  const themes = allThemes();
  const theme = themes.find((t) => t.id === themeId) ?? themes[0];
  const refresh = async () => {
    try { setStatus(await invoke<OverlayStatus>("overlay_status")); }
    catch { /* Browser development has no native backend. */ }
  };
  useEffect(() => { void refresh(); const timer = setInterval(refresh, 3000); return () => clearInterval(timer); }, []);
  const start = async () => {
    setError(null);
    try {
      const n = Number(port);
      if (!Number.isInteger(n) || n < 1024 || n > 65535) throw new Error("端口需为 1024–65535 的整数");
      setStatus(await invoke<OverlayStatus>("overlay_start", { port: n }));
    } catch (e) { setError(String(e)); }
  };
  const stop = async () => {
    try { await invoke("overlay_stop"); await refresh(); }
    catch (e) { setError(String(e)); }
  };
  const copy = async (url: string, tag: string) => {
    try {
      if (!navigator.clipboard) throw new Error("无法访问剪贴板，请选中地址手动复制");
      await navigator.clipboard.writeText(url); setCopied(tag);
      setTimeout(() => setCopied(null), 1500);
    } catch (e) { setError(String(e)); }
  };
  const open = (url: string) => void openUrl(url).catch((e) => setError(String(e)));
  const params = new URLSearchParams({ lang: locale, platform, room: room.trim(), size, width, lines, hold: String(Number(hold) * 1000), theme: JSON.stringify(theme), avatars: avatars ? "1" : "0", badges: badges ? "1" : "0" });
  const chatUrl = status?.danmaku_url ? `${status.danmaku_url}?${params}` : "";
  const subtitleUrl = status?.subtitle_url ? `${status.subtitle_url}?${new URLSearchParams({ lang: locale, room: room.trim(), platform })}` : "";
  return <div className="panel" data-testid="obs-panel">
    <h2>{t("OBS 弹幕 / 评论网页")}</h2>
    <p className="login-note">{t("先在弹幕页连接 Bilibili 或 YouTube，再启动此服务。将地址添加为 OBS「浏览器」源，也可单独打开网页。请保持工具运行。背景透明，预览中的棋盘格不会进入 OBS。")}</p>
    <div className="form-row">
      <label>{t("服务端口")}{" "}<input aria-label={t("服务端口")} type="number" min={1024} max={65535} value={port} disabled={status?.running} onChange={(e) => setPort(e.target.value)} /></label>
      {status?.running ? <span data-testid="obs-running">{t("运行中 ·")}{" "}{status.clients} {t("个页面")}{" "}<button data-testid="obs-stop" onClick={stop}>{t("停止")}</button></span> : <button data-testid="obs-start" onClick={start}>{t("启动叠加层服务")}</button>}
    </div>
    <div className="form-row">
      <label>{t("平台")}{" "}<select aria-label={t("叠加层平台")} value={platform} onChange={(e) => setPlatform(e.target.value)}><option value="all">{t("全部平台")}</option><option value="bilibili">Bilibili</option><option value="youtube">YouTube</option></select></label>
      <label>{t("直播筛选")}{" "}<input aria-label={t("叠加层直播")} placeholder={t("留空显示全部；youtube:视频ID 或 bilibili:房间号")} value={room} onChange={(e) => setRoom(e.target.value)} style={{ width: 360 }} /></label>
      <label>{t("主题")}{" "}<select aria-label={t("叠加层主题")} value={themeId} onChange={(e) => setThemeId(e.target.value)}>{themes.map((t) => <option key={t.id} value={t.id}>{t.id.startsWith("builtin:") ? translate(t.name) : t.name}</option>)}</select></label>
    </div>
    <div className="form-row">
      <label>{t("字号")}{" "}<input aria-label={t("字号")} type="number" min={10} max={72} value={size} onChange={(e) => setSize(e.target.value)} style={{ width: 70 }} /></label>
      <label>{t("宽度")}{" "}<input aria-label={t("宽度")} type="number" min={200} max={3840} value={width} onChange={(e) => setWidth(e.target.value)} style={{ width: 80 }} /></label>
      <label>{t("行数")}{" "}<input aria-label={t("行数")} type="number" min={1} max={200} value={lines} onChange={(e) => setLines(e.target.value)} style={{ width: 70 }} /></label>
      <label>{t("保留秒数（0=常驻）")}{" "}<input aria-label={t("保留秒数")} type="number" min={0} max={600} value={hold} onChange={(e) => setHold(e.target.value)} style={{ width: 70 }} /></label>
      <label><input type="checkbox" checked={avatars} onChange={(e) => setAvatars(e.target.checked)} />{t("头像")}</label>
      <label><input type="checkbox" checked={badges} onChange={(e) => setBadges(e.target.checked)} />{t("平台标识")}</label>
    </div>
    {status?.running && <>
      <p className="hint">{t("修改配置后请重新复制地址到 OBS。弹幕建议 420×700，字幕建议 1920×200。")}</p>
      <div className="form-col">
        <label>{t("弹幕 / 评论地址")}<div className="form-row"><input readOnly data-testid="obs-danmaku-url" value={chatUrl} style={{ flex: 1 }} /><button onClick={() => copy(chatUrl, "dm")}>{copied === "dm" ? t("已复制") : t("复制")}</button><button onClick={() => open(chatUrl)}>{t("单独打开评论网页")}</button></div></label>
        <label>{t("同传字幕地址")}<div className="form-row"><input readOnly data-testid="obs-subtitle-url" value={subtitleUrl} style={{ flex: 1 }} /><button onClick={() => copy(subtitleUrl, "sub")}>{copied === "sub" ? t("已复制") : t("复制")}</button><button onClick={() => open(subtitleUrl)}>{t("单独打开字幕网页")}</button></div></label>
      </div>
      <iframe title={t("评论网页预览")} src={`${chatUrl}&preview=1`} style={{ width: "100%", height: 460, border: "1px solid #8885", borderRadius: 8, marginTop: 16 }} />
    </>}
    {error && <p role="alert" className="error" data-testid="obs-error">{tm(error)}</p>}
  </div>;
}
