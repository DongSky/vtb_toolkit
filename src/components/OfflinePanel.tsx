import { t, tm, useLocale } from "../i18n";
import { useEffect, useState } from "react";
import { usePersisted } from "../hooks/usePersisted";
import ModelPicker from "./ModelPicker";
import LlmSettings from "./LlmSettings";
import { HotwordSelect } from "./HotwordsPanel";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { JobDonePayload, JobProgressPayload } from "../types";
import SpeechLanguageFields from "./SpeechLanguageFields";

export default function OfflinePanel() {
  useLocale();
  const [input, setInput] = useState("");
  const [outputDir, setOutputDir] = usePersisted("off.outputDir", "");
  const [modelPath, setModelPath] = usePersisted("off.modelPath", "");
  const [danmakuLog, setDanmakuLog] = useState("");
  const [translate, setTranslate] = usePersisted("off.translate", false);
  const [asrLang, setAsrLang] = usePersisted("off.asrLang", "auto");
  const [targetLang, setTargetLang] = usePersisted("off.targetLang", "zh");
  const [burnSubs, setBurnSubs] = usePersisted("off.burnSubs", false);
  const [semanticHighlights, setSemanticHighlights] = usePersisted(
    "off.semanticHighlights",
    false,
  );
  const [visualHighlights, setVisualHighlights] = usePersisted(
    "off.visualHighlights",
    false,
  );
  const [preRollSecs, setPreRollSecs] = usePersisted("off.aiPreRollSecs", 8);
  const [postRollSecs, setPostRollSecs] = usePersisted("off.aiPostRollSecs", 12);
  const [visualSampleSecs, setVisualSampleSecs] = usePersisted(
    "off.visualSampleSecs",
    20,
  );
  const [aiMaxClips, setAiMaxClips] = usePersisted("off.aiMaxClips", 24);
  const [songClips, setSongClips] = usePersisted("off.songClips", false);
  const [hotwords, setHotwords] = usePersisted<string[]>("off.hotwords", []);
  const [progress, setProgress] = useState<JobProgressPayload | null>(null);
  const [done, setDone] = useState<JobDonePayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);


  useEffect(() => {
    const unProgress = listen<JobProgressPayload>("job://progress", (e) =>
      setProgress(e.payload),
    );
    const unDone = listen<JobDonePayload>("job://done", (e) => {
      setDone(e.payload);
      setRunning(false);
    });
    return () => {
      unProgress.then((f) => f());
      unDone.then((f) => f());
    };
  }, []);

  const start = async () => {
    setError(null);
    setDone(null);
    setProgress(null);
    try {
      setRunning(true);
      await invoke("offline_process", {
        options: {
          job_id: `job-${Date.now()}`,
          input,
          output_dir: outputDir,
          model_path: modelPath,
          asr_lang: asrLang,
          target_lang: targetLang,
          danmaku_log: danmakuLog || undefined,
          translate,
          burn_subtitles: burnSubs,
          semantic_highlights: semanticHighlights,
          visual_highlights: visualHighlights,
          ai_pre_roll_secs: preRollSecs,
          ai_post_roll_secs: postRollSecs,
          visual_sample_secs: visualSampleSecs,
          ai_max_clips: aiMaxClips,
          song_clips: songClips,
          hotword_tables: hotwords,
        },
      });
    } catch (e) {
      setRunning(false);
      setError(String(e));
    }
  };

  return (
    <div className="panel" data-testid="offline-panel">
      <h2>{t("离线处理（字幕 / 翻译 / 切片）")}</h2>
      <div className="form-col">
        <input
          data-testid="off-input"
          placeholder={t("录播文件路径")}
          value={input}
          onChange={(e) => setInput(e.target.value)}
        />
        <input
          data-testid="off-output"
          placeholder={t("输出目录")}
          value={outputDir}
          onChange={(e) => setOutputDir(e.target.value)}
        />
        <input
          data-testid="off-model"
          placeholder={t("whisper 模型路径（ggml-*.bin）")}
          value={modelPath}
          onChange={(e) => setModelPath(e.target.value)}
        />
        <ModelPicker value={modelPath} onChange={setModelPath} />
        <HotwordSelect value={hotwords} onChange={setHotwords} />
        <input
          data-testid="off-danmaku"
          placeholder={t("弹幕日志 JSONL（可选，用于高能检测）")}
          value={danmakuLog}
          onChange={(e) => setDanmakuLog(e.target.value)}
        />
        <label>
          <input
            type="checkbox"
            data-testid="off-translate"
            checked={translate}
            onChange={(e) => setTranslate(e.target.checked)}
          />
          {t("启用翻译")}{" "}</label>
        <label>
          <input
            type="checkbox"
            data-testid="off-burnsubs"
            checked={burnSubs}
            onChange={(e) => setBurnSubs(e.target.checked)}
          />
          {t("切片烧录双语字幕（需翻译，需带 libass 的 ffmpeg）")}{" "}</label>
        <label>
          <input
            type="checkbox"
            data-testid="off-semantic-highlights"
            checked={semanticHighlights}
            onChange={(e) => setSemanticHighlights(e.target.checked)}
          />
          {t("AI 语义粗剪（按转录内容识别梗、情绪转折和精彩操作）")}{" "}</label>
        <label>
          <input
            type="checkbox"
            data-testid="off-visual-highlights"
            checked={visualHighlights}
            onChange={(e) => setVisualHighlights(e.target.checked)}
          />
          {t("AI 画面扫描（录制后抽帧识别精彩操作、反应和视觉梗）")}{" "}</label>
        {(semanticHighlights || visualHighlights) && (
          <div className="form-row" data-testid="off-ai-clip-options">
            <label>
              {t("前置冗余（秒）")}{" "}<input
                type="number"
                min={0}
                max={120}
                value={preRollSecs}
                onChange={(e) => setPreRollSecs(Number(e.target.value))}
                style={{ width: 72 }}
              />
            </label>
            <label>
              {t("后置冗余（秒）")}{" "}<input
                type="number"
                min={0}
                max={120}
                value={postRollSecs}
                onChange={(e) => setPostRollSecs(Number(e.target.value))}
                style={{ width: 72 }}
              />
            </label>
            <label>
              {t("抽帧间隔（秒）")}{" "}<input
                type="number"
                min={5}
                max={300}
                value={visualSampleSecs}
                disabled={!visualHighlights}
                onChange={(e) => setVisualSampleSecs(Number(e.target.value))}
                style={{ width: 72 }}
              />
            </label>
            <label>
              {t("最多切片")}{" "}<input
                type="number"
                min={1}
                max={100}
                value={aiMaxClips}
                onChange={(e) => setAiMaxClips(Number(e.target.value))}
                style={{ width: 72 }}
              />
            </label>
          </div>
        )}
        <label>
          <input
            type="checkbox"
            data-testid="off-songclips"
            checked={songClips}
            onChange={(e) => setSongClips(e.target.checked)}
          />
          {t("歌切模式（检测歌回中的完整歌曲并单独切出）")}{" "}</label>
        {(translate || semanticHighlights || visualHighlights) && <LlmSettings />}
        <SpeechLanguageFields source={asrLang} target={targetLang} translating={translate} onSource={setAsrLang} onTarget={setTargetLang} />
        <button
          data-testid="off-start"
          disabled={running || !input || !outputDir || !modelPath}
          onClick={start}
        >
          {running ? t("处理中…") : t("开始处理")}
        </button>
      </div>

      {error && (
        <div className="error" data-testid="off-error">
          {tm(error)}
        </div>
      )}
      {progress && (
        <div className="progress" data-testid="off-progress">
          [{tm(progress.stage)}] {tm(progress.message)}
          {progress.fraction != null &&
            ` ${(progress.fraction * 100).toFixed(0)}%`}
        </div>
      )}
      {done && (
        <div
          className={done.ok ? "done" : "error"}
          data-testid="off-done"
        >
          {done.ok
            ? t("完成：{0} 段字幕 / {1} 段翻译 / {2} 个高能 / {3} 个切片", done.transcript_segments, done.translations, done.highlights, done.clips)
            : t("失败：{0}", tm(done.message))}
        </div>
      )}
    </div>
  );
}
