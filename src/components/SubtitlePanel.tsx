import { t, tm, useLocale } from "../i18n";
import { useEffect, useState } from "react";
import { usePersisted } from "../hooks/usePersisted";
import ModelPicker from "./ModelPicker";
import LlmSettings from "./LlmSettings";
import { HotwordSelect } from "./HotwordsPanel";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { youtubeInput } from "../youtube/normalize";
import SpeechLanguageFields from "./SpeechLanguageFields";

interface SubtitleSegment {
  room_key?: string;
  room_id: number;
  start_ms: number;
  end_ms: number;
  text: string;
  lang: string | null;
  translated: string | null;
}

export default function SubtitlePanel() {
  useLocale();
  const [roomId, setRoomId] = usePersisted("sub.room", "");
  const [modelPath, setModelPath] = usePersisted("sub.modelPath", "");
  const [translate, setTranslate] = usePersisted("sub.translate", false);
  const [asrLang, setAsrLang] = usePersisted("sub.asrLang", "auto");
  const [targetLang, setTargetLang] = usePersisted("sub.targetLang", "zh");
  const [hotwords, setHotwords] = usePersisted<string[]>("sub.hotwords", []);
  const [active, setActive] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [segments, setSegments] = useState<SubtitleSegment[]>([]);
  const [error, setError] = useState<string | null>(null);


  useEffect(() => {
    const un = listen<SubtitleSegment>("subtitle://segment", (e) =>
      setSegments((prev) => [...prev.slice(-199), e.payload]),
    );
    const refresh = () => invoke<string[]>("live_subtitle_status").then((v) => setActive(Array.isArray(v) ? v : [])).catch(() => {});
    void refresh();
    const off = listen("subtitle://ended", refresh);
    const timer = setInterval(refresh, 3000);
    return () => {
      clearInterval(timer);
      void off.then((f) => f());
      un.then((f) => f());
    };
  }, []);

  const start = async () => {
    setError(null);
    setBusy(true);
    try {
      const isBili = /^\d+$/.test(roomId.trim());
      const key = await invoke<string>("live_subtitle_start", {
        options: {
          room_id: isBili ? Number(roomId) : 0,
          ...(!isBili ? { source_url: youtubeInput(roomId) } : {}),
          model_path: modelPath,
          asr_lang: asrLang,
          target_lang: targetLang,
          translate,
          hotword_tables: hotwords,
        },
      });
      setActive((prev) => [...prev, key || `bilibili:${roomId}`]);
    } catch (e) {
      setError(String(e));
    } finally { setBusy(false); }
  };

  const stop = async (source: string) => {
    try {
      await invoke("live_subtitle_stop", { source });
      setActive((prev) => prev.filter((key) => key !== source));
    } catch (e) { setError(String(e)); }
  };

  return (
    <div className="panel" data-testid="subtitle-panel">
      <h2>{t("实时字幕 / 同传")}</h2>
      <div className="form-col">
        <input
          data-testid="sub-room"
          placeholder={t("Bilibili 房间号 / YouTube 直播或频道链接")}
          value={roomId}
          onChange={(e) => setRoomId(e.target.value)}
        />
        <input
          data-testid="sub-model"
          placeholder={t("whisper 模型路径（ggml-*.bin）")}
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
          {t("启用同传翻译")}{" "}</label>
        {translate && <LlmSettings />}
        <SpeechLanguageFields source={asrLang} target={targetLang} translating={translate} onSource={setAsrLang} onTarget={setTargetLang} />
        <button data-testid="sub-start" disabled={busy || !roomId || !modelPath} onClick={start}>{busy ? t("连接中…") : t("开始")}</button>
        {active.map((key) => <div key={key}>{key} <button data-testid="sub-stop" onClick={() => stop(key)}>{t("停止")}</button></div>)}
      </div>
      {error && (
        <div className="error" data-testid="sub-error">
          {tm(error)}
        </div>
      )}
      <div className="log" data-testid="sub-list">
        {segments.map((s, i) => (
          <div key={i} className="sub-row">
            <small>{s.room_key || `bilibili:${s.room_id}`} </small>
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
