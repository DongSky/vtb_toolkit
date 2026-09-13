import { t, useLocale } from "../i18n";
import type { ServiceKind } from "../hooks/useAiSettings";

export default function LlmSettings({ kind = "text" }: { kind?: ServiceKind }) {
  useLocale();
  return <p className="hint" data-testid="llm-settings">
    {t("AI 服务在设置页统一配置，保存后用于新任务。")}{" "}
    <button onClick={() => window.dispatchEvent(new CustomEvent("open-ai-settings", { detail: kind }))}>{t("打开 AI 服务设置")}</button>
  </p>;
}
