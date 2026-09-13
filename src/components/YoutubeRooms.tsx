import { t, tm, useLocale } from "../i18n";
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { usePersisted } from "../hooks/usePersisted";
import { youtubeInput } from "../youtube/normalize";
import type { YoutubeRoom } from "../youtube/service";

interface ConnectionStatus { video_id: string; title: string; state: string; message: string; consumers: string[] }

export default function YoutubeRooms({ compact = false }: { compact?: boolean }) {
  useLocale();
  const [rooms, setRooms] = usePersisted<string[]>("youtube.rooms", []);
  const [input, setInput] = useState("");
  const [cards, setCards] = useState<Record<string, YoutubeRoom>>({});
  const [connections, setConnections] = useState<ConnectionStatus[]>([]);
  const [recording, setRecording] = useState<string[]>([]);
  const [outputDir] = usePersisted("rec.outputDir", "");
  const [segmentMode] = usePersisted("rec.segmentMode", "duration");
  const [segmentValue] = usePersisted("rec.segmentValue", "3600");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const refreshStatus = useCallback(async () => {
    const [chat, rec] = await Promise.all([
      invoke<ConnectionStatus[]>("youtube_status"), invoke<string[]>("platform_record_status"),
    ]);
    setConnections(Array.isArray(chat) ? chat : []);
    setRecording(Array.isArray(rec) ? rec : []);
  }, []);
  useEffect(() => {
    let cancelled = false;
    let refreshing = false;
    const refresh = async () => {
      if (refreshing) return;
      refreshing = true;
      try {
        await refreshStatus();
        let refreshError: string | null = null;
        for (const input of rooms) {
          if (cancelled) break;
          try {
            const card = await invoke<YoutubeRoom>("youtube_call", { method: "info", args: { input } });
            if (!cancelled && card) setCards((prev) => ({ ...prev, [input]: card }));
          } catch (e) { refreshError = String(e); }
        }
        if (!cancelled) setError(refreshError);
      } catch (e) { if (!cancelled) setError(String(e)); }
      finally { refreshing = false; }
    };
    void refresh();
    const timer = setInterval(refresh, 30_000);
    const off = listen("youtube://status", () => void refreshStatus().catch((e) => setError(String(e))));
    const offRec = listen("platform_rec://event", () => void refreshStatus().catch((e) => setError(String(e))));
    return () => { cancelled = true; clearInterval(timer); void off.then((f) => f()); void offRec.then((f) => f()); };
  }, [rooms, refreshStatus]);

  const action = async (command: string, args: Record<string, unknown>) => {
    setBusy(true); setError(null);
    try { await invoke(command, args); await refreshStatus(); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const add = () => {
    try {
      const url = youtubeInput(input);
      if (!rooms.includes(url)) setRooms([...rooms, url]);
      setInput(""); setError(null);
      if (compact) void action("youtube_call", { method: "connect", args: { input: url } });
    } catch (e) { setError(String(e)); }
  };
  return <section data-testid="youtube-rooms">
    <h3>YouTube</h3>
    <div className="form-row">
      <input aria-label={t("YouTube 直播或频道")} placeholder={t("直播链接、频道链接、@频道或视频 ID")} value={input} onChange={(e) => setInput(e.target.value)} style={{ flex: 1 }} />
      <button disabled={busy || !input.trim()} onClick={add}>{compact ? t("连接 YouTube") : t("添加频道 / 直播")}</button>
    </div>
    <p className="hint">{t("匿名读取公开直播评论；录制需要 yt-dlp 和 FFmpeg。频道链接会跟踪后续直播。")}</p>
    {!compact && <div className="room-grid">{rooms.map((url) => {
      const card = cards[url];
      const connection = connections.find((c) => c.video_id === card?.video_id);
      const uiConnected = connection?.consumers?.includes("ui");
      return <div className="room-card" key={url}>
        {card?.cover && <img className="room-cover" src={card.cover} alt="" />}
        <div className="room-body">
          <div className="room-title">{card?.title || url}</div>
          <div className="room-meta">{card?.author} · {card ? card.live ? t("直播中") : card.upcoming ? t("等待开播") : t("未开播") : t("查询中")}</div>
          <small>{url}</small>
          <div className="form-row">
            <button disabled={busy || (!uiConnected && !card?.chat_available)} onClick={() => action("youtube_call", { method: uiConnected ? "disconnect" : "connect", args: uiConnected ? { video_id: card?.video_id } : { input: url } })}>{uiConnected ? t("断开评论") : t("连接评论")}</button>
            <button disabled={busy || (!outputDir && !recording.includes(url))} onClick={() => action(recording.includes(url) ? "platform_record_stop" : "platform_record_start", { id: url, outputDir, segmentMode, segmentValue: Number(segmentValue) })}>{recording.includes(url) ? t("停止监听录制") : t("监听录制")}</button>
            <button disabled={busy} onClick={() => setRooms(rooms.filter((r) => r !== url))}>{t("移除收藏")}</button>
          </div>
        </div>
      </div>;
    })}</div>}
    <ul>{connections.map((c) => <li key={c.video_id}>
      {c.title || c.video_id} · {tm(c.message)}
      {c.consumers?.some((v) => v.startsWith("record:")) && t(" · 录制评论日志中")}
      {c.consumers?.includes("ui") && <button disabled={busy} onClick={() => action("youtube_call", { method: "disconnect", args: { video_id: c.video_id } })}>{t("断开评论预览")}</button>}
    </li>)}</ul>
    {error && <p role="alert" className="error">{tm(error)}</p>}
  </section>;
}
