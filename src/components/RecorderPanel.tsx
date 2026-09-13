import { t, tm, useLocale } from "../i18n";
import { useEffect, useState } from "react";
import { usePersisted } from "../hooks/usePersisted";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { RecorderEventPayload } from "../types";
import NotifySettings from "./NotifySettings";

export default function RecorderPanel() {
  useLocale();
  const [roomId, setRoomId] = usePersisted("rec.room", "");
  const [outputDir, setOutputDir] = usePersisted("rec.outputDir", "");
  const [segmentMode, setSegmentMode] = usePersisted("rec.segmentMode", "duration");
  const [segmentValue, setSegmentValue] = usePersisted("rec.segmentValue", "3600");
  const [retention, setRetention] = usePersisted<{
    max_age_days: number;
    max_sessions_per_room: number;
    target_free_gib: number;
  }>("retention", {
    max_age_days: 0,
    max_sessions_per_room: 0,
    target_free_gib: 0,
  });
  const [active, setActive] = useState<number[]>([]);
  const [log, setLog] = useState<{ prefix?: string; message: string }[]>([]);
  const [error, setError] = useState<string | null>(null);
  // 多平台 (YouTube/Twitch via yt-dlp)
  const [platformUrl, setPlatformUrl] = usePersisted("rec.platformUrl", "");
  const [platformActive, setPlatformActive] = useState<string[]>([]);

  const refresh = async () => {
    try {
      setActive(await invoke<number[]>("recorder_status"));
      const p = await invoke<string[]>("platform_record_status");
      setPlatformActive(Array.isArray(p) ? p : []);
    } catch {
      /* backend absent in browser dev */
    }
  };

  useEffect(() => {
    refresh();
    const un = listen<RecorderEventPayload>("recorder://event", (e) => {
      const p = e.payload;
      const line =
        p.kind === "started"
          ? `房间 ${p.room_id} 开始录制 → ${p.output_dir}`
          : p.kind === "stopped"
            ? `房间 ${p.room_id} 录制结束（${p.segments} 段, ${((p.total_bytes ?? 0) / 1048576).toFixed(1)} MB）`
            : `房间 ${p.room_id} 错误: ${p.message}`;
      setLog((l) => [...l.slice(-99), { message: line }]);
      refresh();
    });
    const unP = listen<{ id: string; kind: string; message: string }>(
      "platform_rec://event",
      (e) => {
        setLog((l) => [
          ...l.slice(-99),
          { prefix: `[${e.payload.kind}] ${e.payload.id}: `, message: e.payload.message },
        ]);
        refresh();
      },
    );
    return () => {
      un.then((f) => f());
      unP.then((f) => f());
    };
  }, []);

  const startPlatform = async () => {
    setError(null);
    try {
      await invoke("platform_record_start", {
        id: platformUrl,
        outputDir: outputDir,
        segmentMode,
        segmentValue: Number(segmentValue) || undefined,
      });
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const start = async () => {
    setError(null);
    try {
      await invoke("recorder_start", {
        options: {
          room_id: Number(roomId),
          output_dir: outputDir,
          segment_mode: segmentMode,
          segment_value: Number(segmentValue) || undefined,
        },
      });
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const stop = async (id: number) => {
    try {
      await invoke("recorder_stop", { roomId: id });
      refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="panel" data-testid="recorder-panel">
      <h2>{t("自动录制")}</h2>
      <div className="form-row">
        <input
          data-testid="rec-room"
          placeholder={t("房间号")}
          value={roomId}
          onChange={(e) => setRoomId(e.target.value)}
        />
        <input
          data-testid="rec-dir"
          placeholder={t("输出目录")}
          value={outputDir}
          onChange={(e) => setOutputDir(e.target.value)}
        />
        <select
          data-testid="rec-segment-mode"
          value={segmentMode}
          onChange={(e) => setSegmentMode(e.target.value)}
        >
          <option value="single">{t("不切分")}</option>
          <option value="duration">{t("按时长(秒)")}</option>
          <option value="size">{t("按大小(字节)")}</option>
        </select>
        {segmentMode !== "single" && (
          <input
            data-testid="rec-segment-value"
            value={segmentValue}
            onChange={(e) => setSegmentValue(e.target.value)}
          />
        )}
        <button
          data-testid="rec-start"
          disabled={!roomId || !outputDir}
          onClick={start}
        >
          {t("开始监听")}{" "}</button>
      </div>
      {error && (
        <div className="error" data-testid="rec-error">
          {tm(error)}
        </div>
      )}
      <ul data-testid="rec-active">
        {active.map((id) => (
          <li key={id}>
            {t("房间 {0} 监听中", id)}{" "}<button onClick={() => stop(id)}>{t("停止")}</button>
          </li>
        ))}
      </ul>
      <div className="form-row" data-testid="rec-platform">
        <input
          data-testid="rec-platform-url"
          placeholder={t("YouTube/Twitch 直播 URL（需安装 yt-dlp）")}
          style={{ flex: 1 }}
          value={platformUrl}
          onChange={(e) => setPlatformUrl(e.target.value)}
        />
        <button
          data-testid="rec-platform-start"
          disabled={!platformUrl.trim() || !outputDir}
          onClick={startPlatform}
        >
          {t("监听录制")}{" "}</button>
        {platformActive.map((k) => (
          <span key={k}>
            {k}
            <button
              onClick={() =>
                invoke("platform_record_stop", { id: k })
                  .then(refresh)
                  .catch((e) => setError(String(e)))
              }
            >
              {t("停止")}{" "}</button>
          </span>
        ))}
      </div>
      <div className="form-row" data-testid="rec-retention">
        <span>{t("滚动清理（0=关闭）:")}</span>
        <label>
          {t("保留天数")}{" "}<input
            data-testid="rec-retention-days"
            type="number"
            min={0}
            style={{ width: 64 }}
            value={retention.max_age_days}
            onChange={(e) =>
              setRetention({ ...retention, max_age_days: Number(e.target.value) || 0 })
            }
          />
        </label>
        <label>
          {t("每房间场次")}{" "}<input
            data-testid="rec-retention-sessions"
            type="number"
            min={0}
            style={{ width: 64 }}
            value={retention.max_sessions_per_room}
            onChange={(e) =>
              setRetention({
                ...retention,
                max_sessions_per_room: Number(e.target.value) || 0,
              })
            }
          />
        </label>
        <label>
          {t("目标剩余(GiB)")}{" "}<input
            data-testid="rec-retention-free"
            type="number"
            min={0}
            style={{ width: 64 }}
            value={retention.target_free_gib}
            onChange={(e) =>
              setRetention({
                ...retention,
                target_free_gib: Number(e.target.value) || 0,
              })
            }
          />
        </label>
      </div>
      <NotifySettings />
      <pre className="log" data-testid="rec-log">
        {log.map((entry) => (entry.prefix ?? "") + tm(entry.message)).join("\n")}
      </pre>
    </div>
  );
}
