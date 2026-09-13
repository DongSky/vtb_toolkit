import { t, tm, useLocale } from "../i18n";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";

interface Status {
  configured: boolean;
  authorized: boolean;
  authorizing: boolean;
  uploading: boolean;
  progress?: { state?: string; message?: string; video_id?: string; sent?: number; total?: number };
}
export default function YoutubeUpload({ file }: { file?: string }) {
  useLocale();
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState("");
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [title, setTitle] = useState(() => file?.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "")?.slice(0, 100) ?? "");
  const [description, setDescription] = useState("");
  const [tags, setTags] = useState("虚拟主播,切片");
  const [privacy, setPrivacy] = useState("private");
  const [kids, setKids] = useState(false);
  const [busy, setBusy] = useState(false);
  const refresh = async () => { const s = await invoke<Status>("youtube_upload_status"); setStatus(s ?? null); };
  useEffect(() => {
    void refresh().catch(() => {});
    const off = listen("youtube-upload://status", () => void refresh().catch((e) => setError(String(e))));
    const timer = setInterval(() => void refresh().catch(() => {}), 3000);
    return () => { clearInterval(timer); void off.then((f) => f()); };
  }, []);
  const action = async (command: string, args?: Record<string, unknown>) => {
    setBusy(true); setError("");
    try { await invoke(command, args); await refresh(); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const authorize = async () => {
    setError(""); setBusy(true);
    try {
      const url = await invoke<string>("youtube_upload_authorize");
      await openUrl(url); await refresh();
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  return <section className="panel" data-testid="youtube-upload">
    <h3>{file ? t("投稿到 YouTube") : t("YouTube 投稿账号")}</h3>
    {!file && <>
      <p>{t("读取评论和录制无需此授权。投稿需在 Google Cloud 启用 YouTube Data API v3，创建「桌面应用」OAuth 客户端；应用处于测试模式时，将自己的 Google 账号加入测试用户。")}</p>
      <div className="form-col">
        <input aria-label={t("Google OAuth 客户端 ID")} placeholder={t("客户端 ID（…apps.googleusercontent.com）")} value={clientId} onChange={(e) => setClientId(e.target.value)} />
        <input aria-label={t("Google OAuth 客户端密钥")} type="password" placeholder={t("客户端密钥")} value={clientSecret} onChange={(e) => setClientSecret(e.target.value)} />
        <button disabled={busy || !clientId || status?.uploading} onClick={async () => { await action("youtube_upload_configure", { client: { client_id: clientId.trim(), client_secret: clientSecret.trim() } }); setClientSecret(""); }}>{t("保存投稿客户端")}</button>
      </div>
      <p className="hint">{t("客户端配置和授权凭据保存在系统凭据库。")}</p>
      <button disabled={busy || !status?.configured || status.authorizing} onClick={authorize}>{status?.authorized ? t("重新授权") : t("在浏览器授权 YouTube")}</button>
      {status?.authorized && <button disabled={busy} onClick={() => action("youtube_upload_logout")}>{t("清除本机授权")}</button>}
    </>}
    {file && <>
      <p>{file}</p>
      <div className="form-col">
        <label>{t("标题")}<input aria-label={t("YouTube 投稿标题")} maxLength={100} value={title} onChange={(e) => setTitle(e.target.value)} /></label>
        <label>{t("描述")}<textarea aria-label={t("YouTube 投稿描述")} value={description} onChange={(e) => setDescription(e.target.value)} /></label>
        <label>{t("标签（逗号分隔）")}<input value={tags} onChange={(e) => setTags(e.target.value)} /></label>
        <label>{t("可见性")}<select aria-label={t("YouTube 可见性")} value={privacy} onChange={(e) => setPrivacy(e.target.value)}><option value="private">{t("私享")}</option><option value="unlisted">{t("不公开列出")}</option><option value="public">{t("公开")}</option></select></label>
        <label><input type="checkbox" checked={kids} onChange={(e) => setKids(e.target.checked)} />{t("此视频是面向儿童的内容")}</label>
        <button disabled={busy || !status?.authorized || status.uploading || !title.trim()} onClick={() => action("youtube_upload_start", { options: { file, title, description, tags: tags.split(/[,，]/).map((s) => s.trim()).filter(Boolean), privacy, made_for_kids: kids } })}>{t("上传到 YouTube（{0}）", privacy === "private" ? t("私享") : privacy === "unlisted" ? t("不公开列出") : t("公开"))}</button>
      </div>
      {!status?.authorized && <p>{t("请先到「账号」页配置并授权 YouTube 投稿。")}</p>}
    </>}
    {status?.authorizing && <p>{t("请在浏览器完成授权。")}</p>}
    {(status?.authorizing || status?.uploading) && <button onClick={() => action("youtube_upload_cancel")}>{status.authorizing ? t("取消授权") : t("取消上传")}</button>}
    {!!status?.progress?.total && <progress value={status.progress.sent ?? 0} max={status.progress.total} />}
    {status?.progress?.message && <p role="status">{tm(status.progress.message)}</p>}
    {status?.progress?.video_id && <button onClick={() => void openUrl(`https://www.youtube.com/watch?v=${status.progress!.video_id}`).catch((e) => setError(String(e)))}>{t("查看已上传视频")}</button>}
    {error && <p role="alert" className="error">{tm(error)}</p>}
  </section>;
}
