import { useEffect, useState } from "react";
import { usePersisted } from "../hooks/usePersisted";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { JobDonePayload, JobProgressPayload } from "../types";

export default function OfflinePanel() {
  const [input, setInput] = useState("");
  const [outputDir, setOutputDir] = usePersisted("off.outputDir", "");
  const [modelPath, setModelPath] = usePersisted("off.modelPath", "");
  const [danmakuLog, setDanmakuLog] = useState("");
  const [translate, setTranslate] = usePersisted("off.translate", false);
  const [apiKey, setApiKey] = useState("");
  const [progress, setProgress] = useState<JobProgressPayload | null>(null);
  const [done, setDone] = useState<JobDonePayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  const [keySaved, setKeySaved] = useState(false);
  useEffect(() => {
    Promise.resolve(invoke<boolean>("secret_exists", { name: "llm-api-key" }))
      .then((v) => setKeySaved(Boolean(v)))
      .catch(() => {});
  }, []);
  const saveKeyToKeychain = async () => {
    if (!apiKey) return;
    try {
      await invoke("secret_set", { name: "llm-api-key", value: apiKey });
      setKeySaved(true);
    } catch {
      /* keychain unavailable */
    }
  };


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
          llm_api_key: apiKey || undefined,
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
        {translate && (
          <input
            data-testid="off-apikey"
            type="password"
            placeholder={keySaved ? "已保存到钥匙串（留空则使用）" : "LLM API Key（自动存入钥匙串）"}
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            onBlur={saveKeyToKeychain}
          />
        )}
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
