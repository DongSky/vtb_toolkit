import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface ModelStatus {
  name: string;
  size_mb: number;
  note: string;
  downloaded: boolean;
  path: string;
}

interface Progress {
  name: string;
  downloaded: number;
  total: number | null;
}

/** Whisper model picker: choose a downloaded model or fetch one. */
export default function ModelPicker({
  value,
  onChange,
}: {
  value: string;
  onChange: (path: string) => void;
}) {
  const [models, setModels] = useState<ModelStatus[]>([]);
  const [downloading, setDownloading] = useState<string | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = async () => {
    try {
      const list = await invoke<ModelStatus[]>("asr_models");
      setModels(Array.isArray(list) ? list : []);
    } catch {
      /* browser dev */
    }
  };

  useEffect(() => {
    refresh();
    const un = listen<Progress>("model://progress", (e) => setProgress(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, []);

  const download = async (name: string) => {
    setError(null);
    setDownloading(name);
    try {
      const path = await invoke<string>("asr_model_download", { name });
      onChange(path);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setDownloading(null);
      setProgress(null);
    }
  };

  return (
    <div className="model-picker" data-testid="model-picker">
      <div className="form-row">
        <select
          data-testid="model-select"
          value={models.find((m) => m.path === value)?.name ?? ""}
          onChange={(e) => {
            const m = models.find((x) => x.name === e.target.value);
            if (m?.downloaded) onChange(m.path);
          }}
        >
          <option value="">选择 whisper 模型…</option>
          {models.map((m) => (
            <option key={m.name} value={m.name} disabled={!m.downloaded}>
              {m.name} ({m.size_mb}MB{m.downloaded ? "" : "，未下载"}) — {m.note}
            </option>
          ))}
        </select>
      </div>
      <div className="form-row" style={{ flexWrap: "wrap", gap: 6 }}>
        {models
          .filter((m) => !m.downloaded)
          .map((m) => (
            <button
              key={m.name}
              data-testid={`model-dl-${m.name}`}
              disabled={downloading !== null}
              onClick={() => download(m.name)}
            >
              {downloading === m.name
                ? progress?.total
                  ? `下载中 ${Math.round((progress.downloaded / progress.total) * 100)}%`
                  : "下载中…"
                : `下载 ${m.name}`}
            </button>
          ))}
      </div>
      {error && <div className="error">{error}</div>}
    </div>
  );
}
