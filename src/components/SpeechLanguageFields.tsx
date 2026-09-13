import { t, useLocale } from "../i18n";

const languages = [["zh", "中文"], ["en", "English"], ["ja", "日本語"], ["ko", "한국어"]];

/** Speech settings are explicit and independent of the interface locale. */
export default function SpeechLanguageFields({ source, target, translating, onSource, onTarget }: {
  source: string; target: string; translating: boolean;
  onSource: (value: string) => void; onTarget: (value: string) => void;
}) {
  useLocale();
  return <div className="form-row">
    <label>{t("语音识别语种")} <select aria-label={t("语音识别语种")} value={source} onChange={(e) => onSource(e.target.value)}>
      <option value="auto">{t("自动检测")}</option>
      {languages.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
    </select></label>
    {translating && <label>{t("翻译目标语种")} <select aria-label={t("翻译目标语种")} value={target} onChange={(e) => onTarget(e.target.value)}>
      {languages.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
    </select></label>}
  </div>;
}
