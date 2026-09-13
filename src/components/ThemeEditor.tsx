import { t, tm, useLocale } from "../i18n";
import { useState } from "react";
import type { DanmakuTheme } from "../themes";
import { deriveTheme, exportTheme, importTheme } from "../themes";

export interface ThemeEditorProps {
  theme: DanmakuTheme;
  onSave: (theme: DanmakuTheme) => void;
}

/** Labels for the editable CSS variables. */
const VAR_LABELS: Record<string, string> = {
  "--dm-bg": "背景色",
  "--dm-text": "弹幕文字",
  "--dm-username": "用户名",
  "--dm-medal-bg": "粉丝牌",
  "--dm-sc-bg": "SC 背景",
  "--dm-gift": "礼物",
  "--dm-guard": "舰长",
  "--dm-font-size": "字号",
  "--dm-line-gap": "行距",
  "--dm-radius": "圆角",
  "--dm-font-family": "字体",
};

const COLOR_VARS = new Set([
  "--dm-bg",
  "--dm-text",
  "--dm-username",
  "--dm-medal-bg",
  "--dm-sc-bg",
  "--dm-gift",
  "--dm-guard",
]);

export default function ThemeEditor({ theme, onSave }: ThemeEditorProps) {
  useLocale();
  const [draft, setDraft] = useState<DanmakuTheme>(() =>
    theme.id.startsWith("builtin:")
      ? deriveTheme(theme, t("{0} 副本", t(theme.name)))
      : { ...theme, vars: { ...theme.vars } },
  );
  const [importError, setImportError] = useState<string | null>(null);

  const setVar = (key: string, value: string) =>
    setDraft((d) => ({ ...d, vars: { ...d.vars, [key]: value } }));

  const handleImport = (json: string) => {
    try {
      setDraft(importTheme(json));
      setImportError(null);
    } catch (e) {
      setImportError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="theme-editor" data-testid="theme-editor">
      <label>
        {t("主题名")}{" "}<input
          data-testid="theme-name"
          value={draft.name}
          onChange={(e) => setDraft({ ...draft, name: e.target.value })}
        />
      </label>

      {Object.entries(draft.vars).map(([key, value]) => (
        <label key={key}>
          {t(VAR_LABELS[key] ?? key)}
          <input
            data-testid={`var-${key}`}
            type={COLOR_VARS.has(key) && value.startsWith("#") ? "color" : "text"}
            value={value}
            onChange={(e) => setVar(key, e.target.value)}
          />
        </label>
      ))}

      <label>
        {t("入场动画")}{" "}<select
          data-testid="theme-animation"
          value={draft.animation}
          onChange={(e) =>
            setDraft({
              ...draft,
              animation: e.target.value as DanmakuTheme["animation"],
            })
          }
        >
          <option value="none">{t("无")}</option>
          <option value="fade">{t("淡入")}</option>
          <option value="slide">{t("滑入")}</option>
        </select>
      </label>
      <label>
        <input
          type="checkbox"
          data-testid="theme-show-medal"
          checked={draft.showMedal}
          onChange={(e) => setDraft({ ...draft, showMedal: e.target.checked })}
        />
        {t("显示粉丝牌")}{" "}</label>
      <label>
        <input
          type="checkbox"
          data-testid="theme-show-time"
          checked={draft.showTime}
          onChange={(e) => setDraft({ ...draft, showTime: e.target.checked })}
        />
        {t("显示时间")}{" "}</label>

      <div className="theme-editor-actions">
        <button data-testid="theme-save" onClick={() => onSave(draft)}>
          {t("保存主题")}{" "}</button>
        <button
          data-testid="theme-export"
          onClick={() => navigator.clipboard?.writeText(exportTheme(draft))}
        >
          {t("导出到剪贴板")}{" "}</button>
        <textarea
          data-testid="theme-import-input"
          placeholder={t("粘贴主题 JSON 后点击导入")}
          onChange={() => setImportError(null)}
        />
        <button
          data-testid="theme-import"
          onClick={() => {
            const el = document.querySelector<HTMLTextAreaElement>(
              '[data-testid="theme-import-input"]',
            );
            if (el?.value) handleImport(el.value);
          }}
        >
          {t("导入")}{" "}</button>
        {importError && (
          <div className="theme-error" data-testid="theme-import-error">
            {tm(importError)}
          </div>
        )}
      </div>
    </div>
  );
}
