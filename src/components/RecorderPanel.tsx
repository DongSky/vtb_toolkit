import { useEffect, useState } from "react";
import { usePersisted } from "../hooks/usePersisted";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { RecorderEventPayload } from "../types";
import NotifySettings from "./NotifySettings";

export default function RecorderPanel() {
  const [roomId, setRoomId] = usePersisted("rec.room", "");
  const [outputDir, setOutputDir] = usePersisted("rec.outputDir", "");
  const [segmentMode, setSegmentMode] = usePersisted("rec.segmentMode", "duration");
  const [segmentValue, setSegmentValue] = usePersisted("rec.segmentValue", "3600");
  const [active, setActive] = useState<number[]>([]);
  const [log, setLog] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refresh = async () => {
    try {
      setActive(await invoke<number[]>("recorder_status"));
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
      setLog((l) => [...l.slice(-99), line]);
      refresh();
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

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
      <h2>自动录制</h2>
      <div className="form-row">
        <input
          data-testid="rec-room"
          placeholder="房间号"
          value={roomId}
          onChange={(e) => setRoomId(e.target.value)}
        />
        <input
          data-testid="rec-dir"
          placeholder="输出目录"
          value={outputDir}
          onChange={(e) => setOutputDir(e.target.value)}
        />
        <select
          data-testid="rec-segment-mode"
          value={segmentMode}
          onChange={(e) => setSegmentMode(e.target.value)}
        >
          <option value="single">不切分</option>
          <option value="duration">按时长(秒)</option>
          <option value="size">按大小(字节)</option>
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
          开始监听
        </button>
      </div>
      {error && (
        <div className="error" data-testid="rec-error">
          {error}
        </div>
      )}
      <ul data-testid="rec-active">
        {active.map((id) => (
          <li key={id}>
            房间 {id} 监听中
            <button onClick={() => stop(id)}>停止</button>
          </li>
        ))}
      </ul>
      <NotifySettings />
      <pre className="log" data-testid="rec-log">
        {log.join("\n")}
      </pre>
    </div>
  );
}
