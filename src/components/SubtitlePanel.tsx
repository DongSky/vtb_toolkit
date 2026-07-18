import { useEffect, useState } from "react";
import { usePersisted } from "../hooks/usePersisted";
import ModelPicker from "./ModelPicker";
import LlmSettings, { useLlmSettings } from "./LlmSettings";
import { HotwordSelect } from "./HotwordsPanel";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface SubtitleSegment {
  room_id: number;
  start_ms: number;
  end_ms: number;
  text: string;
  lang: string | null;
  translated: string | null;
}

export default function SubtitlePanel() {
  const [roomId, setRoomId] = usePersisted("sub.room", "");
  const [modelPath, setModelPath] = usePersisted("sub.modelPath", "");
  const [translate, setTranslate] = usePersisted("sub.translate", false);
  const { llm } = useLlmSettings();
  const [hotwords, setHotwords] = usePersisted<string[]>("sub.hotwords", []);
  const [running, setRunning] = useState(false);
  const [segments, setSegments] = useState<SubtitleSegment[]>([]);
  const [error, setError] = useState<string | null>(null);


  useEffect(() => {
    const un = listen<SubtitleSegment>("subtitle://segment", (e) =>
      setSegments((prev) => [...prev.slice(-199), e.payload]),
    );
    return () => {
      un.then((f) => f());
    };
  }, []);

  const start = async () => {
    setError(null);
    try {
      await invoke("live_subtitle_start", {
        options: {
          room_id: Number(roomId),
          model_path: modelPath,
          translate,
          llm_provider: llm.provider || undefined,
          llm_base_url: llm.base_url || undefined,
          llm_model: llm.model || undefined,
          hotword_tables: hotwords,
        },
      });
      setRunning(true);
    } catch (e) {
      setError(String(e));
    }
  };

  const stop = async () => {
    try {
      await invoke("live_subtitle_stop", { roomId: Number(roomId) });
    } catch {
      /* already stopped */
    }
    setRunning(false);
  };

  return (
    <div className="panel" data-testid="subtitle-panel">
      <h2>实时字幕 / 同传</h2>
      <div className="form-col">
        <input
          data-testid="sub-room"
          placeholder="房间号"
          value={roomId}
          onChange={(e) => setRoomId(e.target.value)}
        />
        <input
          data-testid="sub-model"
          placeholder="whisper 模型路径（ggml-*.bin）"
          value={modelPath}
          onChange={(e) => setModelPath(e.target.value)}
        />
        <ModelPicker value={modelPath} onChange={setModelPath} />
        <HotwordSelect value={hotwords} onChange={setHotwords} />
        <label>
          <input
            type="checkbox"
            data-testid="sub-translate"
            checked={translate}
            onChange={(e) => setTranslate(e.target.checked)}
          />
          启用同传翻译
        </label>
        {translate && <LlmSettings />}
        {running ? (
          <button data-testid="sub-stop" onClick={stop}>
            停止
          </button>
        ) : (
          <button
            data-testid="sub-start"
            disabled={!roomId || !modelPath}
            onClick={start}
          >
            开始
          </button>
        )}
      </div>
      {error && (
        <div className="error" data-testid="sub-error">
          {error}
        </div>
      )}
      <div className="log" data-testid="sub-list">
        {segments.map((s, i) => (
          <div key={i} className="sub-row">
            <span className="sub-time">
              {(s.start_ms / 1000).toFixed(1)}s
            </span>{" "}
            <span className="sub-text">{s.text}</span>
            {s.translated && (
              <div className="sub-translated">{s.translated}</div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
