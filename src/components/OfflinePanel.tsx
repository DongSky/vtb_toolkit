import { useEffect, useState } from "react";
import { usePersisted } from "../hooks/usePersisted";
import ModelPicker from "./ModelPicker";
import LlmSettings, { useLlmSettings } from "./LlmSettings";
import { HotwordSelect } from "./HotwordsPanel";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { JobDonePayload, JobProgressPayload } from "../types";

export default function OfflinePanel() {
  const [input, setInput] = useState("");
  const [outputDir, setOutputDir] = usePersisted("off.outputDir", "");
  const [modelPath, setModelPath] = usePersisted("off.modelPath", "");
  const [danmakuLog, setDanmakuLog] = useState("");
  const [translate, setTranslate] = usePersisted("off.translate", false);
  const [burnSubs, setBurnSubs] = usePersisted("off.burnSubs", false);
  const [multimodal, setMultimodal] = usePersisted("off.multimodal", false);
  const [songClips, setSongClips] = usePersisted("off.songClips", false);
  const { llm } = useLlmSettings();
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
          danmaku_log: danmakuLog || undefined,
          translate,
          burn_subtitles: burnSubs,
          multimodal,
          song_clips: songClips,
          hotword_tables: hotwords,
          llm_provider: llm.provider || undefined,
          llm_base_url: llm.base_url || undefined,
          llm_model: llm.model || undefined,
        },
      });
    } catch (e) {
      setRunning(false);
      setError(String(e));
    }
  };

  return (
    <div className="panel" data-testid="offline-panel">
      <h2>离线处理（字幕 / 翻译 / 切片）</h2>
      <div className="form-col">
        <input
          data-testid="off-input"
          placeholder="录播文件路径"
          value={input}
          onChange={(e) => setInput(e.target.value)}
        />
        <input
          data-testid="off-output"
          placeholder="输出目录"
          value={outputDir}
          onChange={(e) => setOutputDir(e.target.value)}
        />
        <input
          data-testid="off-model"
          placeholder="whisper 模型路径（ggml-*.bin）"
          value={modelPath}
          onChange={(e) => setModelPath(e.target.value)}
        />
        <ModelPicker value={modelPath} onChange={setModelPath} />
        <HotwordSelect value={hotwords} onChange={setHotwords} />
        <input
          data-testid="off-danmaku"
          placeholder="弹幕日志 JSONL（可选，用于高能检测）"
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
          启用翻译
        </label>
        <label>
          <input
            type="checkbox"
            data-testid="off-burnsubs"
            checked={burnSubs}
            onChange={(e) => setBurnSubs(e.target.checked)}
          />
          切片烧录双语字幕（需翻译，需带 libass 的 ffmpeg）
        </label>
        <label>
          <input
            type="checkbox"
            data-testid="off-multimodal"
            checked={multimodal}
            onChange={(e) => setMultimodal(e.target.checked)}
          />
          多模态 AI 复核高能片段（自动打分与起标题）
        </label>
        <label>
          <input
            type="checkbox"
            data-testid="off-songclips"
            checked={songClips}
            onChange={(e) => setSongClips(e.target.checked)}
          />
          歌切模式（检测歌回中的完整歌曲并单独切出）
        </label>
        {(translate || multimodal) && <LlmSettings />}
        <button
          data-testid="off-start"
          disabled={running || !input || !outputDir || !modelPath}
          onClick={start}
        >
          {running ? "处理中…" : "开始处理"}
        </button>
      </div>

      {error && (
        <div className="error" data-testid="off-error">
          {error}
        </div>
      )}
      {progress && (
        <div className="progress" data-testid="off-progress">
          [{progress.stage}] {progress.message}
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
            ? `完成：${done.transcript_segments} 段字幕 / ${done.translations} 段翻译 / ${done.highlights} 个高能 / ${done.clips} 个切片`
            : `失败：${done.message}`}
        </div>
      )}
    </div>
  );
}
