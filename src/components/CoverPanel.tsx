import { useEffect, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { usePersisted } from "../hooks/usePersisted";
import { useLlmSettings } from "./LlmSettings";

interface CoverIdea {
  title: string;
  subtitle: string;
  composition: string;
  palette: string;
  platform: string;
  image_prompt: string;
}

/**
 * 封面辅助: candidate frames from 高能片段, LLM cover-design ideas, and
 * gpt-image-2 concept-image generation. Operates on the same analysis
 * directory the review page loads.
 */
export default function CoverPanel({ dir }: { dir: string }) {
  const { llm } = useLlmSettings();
  const [frames, setFrames] = useState<string[]>([]);
  const [ideas, setIdeas] = useState<CoverIdea[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [extraText, setExtraText] = usePersisted("cover.extra", "");
  const [selectedRefs, setSelectedRefs] = useState<Record<string, boolean>>({});

  // gpt-image-2 endpoint (kept separate from the text LLM settings).
  const [imgBase, setImgBase] = usePersisted("cover.img_base", "");
  const [imgModel, setImgModel] = usePersisted("cover.img_model", "gpt-image-2");
  const [imgSize, setImgSize] = usePersisted("cover.img_size", "1536x1024");
  const [imgKey, setImgKey] = useState("");
  const [genPrompt, setGenPrompt] = useState("");

  const refresh = async () => {
    try {
      const list = await invoke<string[]>("cover_list", { dir });
      setFrames(Array.isArray(list) ? list : []);
    } catch {
      /* dir may not have a cover/ yet */
    }
  };

  useEffect(() => {
    if (dir) refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dir]);

  const extractFrames = async () => {
    setError(null);
    setBusy("抽取候选帧…");
    try {
      await invoke<string[]>("cover_extract_frames", { dir });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const genIdeas = async () => {
    setError(null);
    setBusy("生成封面创意…");
    try {
      const result = await invoke<CoverIdea[]>("cover_ideas", {
        dir,
        extraText: extraText.trim() || null,
        llmProvider: llm.provider ?? null,
        llmModel: llm.model ?? null,
        llmBaseUrl: llm.base_url ?? null,
      });
      setIdeas(Array.isArray(result) ? result : []);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const generateImage = async () => {
    setError(null);
    setBusy("生成封面图（gpt-image-2）…");
    try {
      const referenceImages = Object.entries(selectedRefs)
        .filter(([, v]) => v)
        .map(([k]) => k);
      const out = await invoke<string[]>("cover_generate", {
        options: {
          dir,
          prompt: genPrompt,
          base_url: imgBase,
          api_key: imgKey,
          model: imgModel || null,
          size: imgSize || null,
          extra_text: extraText.trim() || null,
          reference_images: referenceImages,
          n: 1,
        },
      });
      if (out.length > 0) await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const useIdeaPrompt = (idea: CoverIdea) => {
    setGenPrompt(idea.image_prompt || `${idea.title} ${idea.composition}`.trim());
  };

  const toggleRef = (path: string) =>
    setSelectedRefs((prev) => ({ ...prev, [path]: !prev[path] }));

  return (
    <div className="cover-panel" data-testid="cover-panel">
      <h3>封面辅助</h3>
      <div className="form-row">
        <button data-testid="cover-extract" disabled={!!busy} onClick={extractFrames}>
          抽取候选帧
        </button>
        <button data-testid="cover-ideas" disabled={!!busy} onClick={genIdeas}>
          生成封面创意 (LLM)
        </button>
        {busy && <span className="hint">{busy}</span>}
      </div>
      <input
        data-testid="cover-extra"
        placeholder="补充信息（梗/人设/活动，供创意与生成参考）"
        value={extraText}
        onChange={(e) => setExtraText(e.target.value)}
        style={{ width: "100%" }}
      />
      {error && (
        <div className="error" data-testid="cover-error">
          {error}
        </div>
      )}

      {ideas.length > 0 && (
        <div data-testid="cover-ideas-list">
          <h4>创意方案</h4>
          {ideas.map((idea, i) => (
            <div key={i} className="cover-idea">
              <strong>{idea.title}</strong>
              {idea.subtitle && <span className="hint"> — {idea.subtitle}</span>}
              <div className="hint">
                {idea.composition} / {idea.palette} / {idea.platform}
              </div>
              {idea.image_prompt && (
                <div className="cover-prompt">
                  <code>{idea.image_prompt}</code>
                  <button
                    data-testid={`cover-use-prompt-${i}`}
                    onClick={() => useIdeaPrompt(idea)}
                  >
                    用作生成 prompt
                  </button>
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {frames.length > 0 && (
        <>
          <h4>候选帧与生成图（{frames.length}，勾选作参考图）</h4>
          <div className="cover-grid" data-testid="cover-grid">
            {frames.map((f) => (
              <div key={f} className="cover-thumb">
                <img src={convertFileSrc(f)} alt="" loading="lazy" />
                <label>
                  <input
                    type="checkbox"
                    checked={!!selectedRefs[f]}
                    onChange={() => toggleRef(f)}
                  />
                  参考
                </label>
              </div>
            ))}
          </div>
        </>
      )}

      <details className="cover-gen" data-testid="cover-gen">
        <summary>gpt-image-2 生成封面{imgBase ? `（${imgBase}）` : "（需配置端点）"}</summary>
        <div className="form-col" style={{ marginTop: 8 }}>
          <input
            data-testid="cover-img-base"
            placeholder="图像 API Base URL（如 https://api.openai.com/v1 或中转）"
            value={imgBase}
            onChange={(e) => setImgBase(e.target.value)}
          />
          <div className="form-row">
            <input
              data-testid="cover-img-model"
              placeholder="模型（默认 gpt-image-2）"
              value={imgModel}
              onChange={(e) => setImgModel(e.target.value)}
            />
            <input
              data-testid="cover-img-size"
              placeholder="尺寸 1536x1024"
              value={imgSize}
              onChange={(e) => setImgSize(e.target.value)}
              style={{ width: 120 }}
            />
          </div>
          <input
            data-testid="cover-img-key"
            type="password"
            placeholder="图像 API Key（仅本次会话使用，不落盘）"
            value={imgKey}
            onChange={(e) => setImgKey(e.target.value)}
          />
          <textarea
            data-testid="cover-gen-prompt"
            placeholder="生成 prompt（可用上方创意的 prompt，或自行填写）"
            value={genPrompt}
            rows={3}
            onChange={(e) => setGenPrompt(e.target.value)}
          />
          <button
            data-testid="cover-generate"
            disabled={!!busy || !imgBase || !imgKey || !genPrompt.trim()}
            onClick={generateImage}
          >
            生成封面图
          </button>
          <p className="login-note">
            勾选上方候选帧作为参考图时走 images/edits 接口（模型基于参考图创作）；
            未勾选则纯文生图。补充信息会自动附加到 prompt。
          </p>
        </div>
      </details>
    </div>
  );
}
