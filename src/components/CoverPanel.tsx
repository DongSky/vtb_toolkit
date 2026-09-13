import { t, tm, useLocale } from "../i18n";
import { useEffect, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { usePersisted } from "../hooks/usePersisted";
import LlmSettings from "./LlmSettings";

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
 * configurable image generation. Operates on the same analysis
 * directory the review page loads.
 */
export default function CoverPanel({ dir }: { dir: string }) {
  useLocale();
  const [frames, setFrames] = useState<string[]>([]);
  const [ideas, setIdeas] = useState<CoverIdea[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [extraText, setExtraText] = usePersisted("cover.extra", "");
  const [selectedRefs, setSelectedRefs] = useState<Record<string, boolean>>({});

  // Per-task image options; service credentials live in AI Settings.
  const [imgSize, setImgSize] = usePersisted("cover.img_size", "1536x1024");
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
    setBusy("正在生成封面图…");
    try {
      const referenceImages = Object.entries(selectedRefs)
        .filter(([, v]) => v)
        .map(([k]) => k);
      const out = await invoke<string[]>("cover_generate", {
        options: {
          dir,
          prompt: genPrompt,
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
      <h3>{t("封面辅助")}</h3>
      <div className="form-row">
        <button data-testid="cover-extract" disabled={!!busy} onClick={extractFrames}>
          {t("抽取候选帧")}{" "}</button>
        <button data-testid="cover-ideas" disabled={!!busy} onClick={genIdeas}>
          {t("生成封面创意 (LLM)")}{" "}</button>
        {busy && <span className="hint">{tm(busy)}</span>}
      </div>
      <input
        data-testid="cover-extra"
        placeholder={t("补充信息（梗/人设/活动，供创意与生成参考）")}
        value={extraText}
        onChange={(e) => setExtraText(e.target.value)}
        style={{ width: "100%" }}
      />
      {error && (
        <div className="error" data-testid="cover-error">
          {tm(error)}
        </div>
      )}

      {ideas.length > 0 && (
        <div data-testid="cover-ideas-list">
          <h4>{t("创意方案")}</h4>
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
                    {t("用作生成 prompt")}{" "}</button>
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {frames.length > 0 && (
        <>
          <h4>{t("候选帧与生成图（{0}，勾选作参考图）", frames.length)}</h4>
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
                  {t("参考")}{" "}</label>
              </div>
            ))}
          </div>
        </>
      )}

      <details className="cover-gen" data-testid="cover-gen">
        <summary>{t("AI 生成封面")}</summary>
        <div className="form-col" style={{ marginTop: 8 }}>
          <LlmSettings kind="image" />
          <label>{t("图片尺寸")} <input data-testid="cover-img-size" value={imgSize} onChange={(e) => setImgSize(e.target.value)} /></label>
          <textarea
            data-testid="cover-gen-prompt"
            placeholder={t("生成 prompt（可用上方创意的 prompt，或自行填写）")}
            value={genPrompt}
            rows={3}
            onChange={(e) => setGenPrompt(e.target.value)}
          />
          <button
            data-testid="cover-generate"
            disabled={!!busy || !genPrompt.trim()}
            onClick={generateImage}
          >
            {t("生成封面图")}{" "}</button>
          <p className="login-note">
            {t("勾选上方候选帧作为参考图时走 images/edits 接口（模型基于参考图创作）； 未勾选则纯文生图。补充信息会自动附加到 prompt。")}{" "}</p>
        </div>
      </details>
    </div>
  );
}
