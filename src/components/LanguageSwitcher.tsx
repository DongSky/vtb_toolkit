import { useState } from "react";
import { type Locale, setLocale, t, useLocale } from "../i18n";

export default function LanguageSwitcher() {
  const locale = useLocale();
  const [failed, setFailed] = useState(false);
  return <div className="language-control">
    <label htmlFor="interface-language">{t("界面语言")}</label>
    <select id="interface-language" data-testid="language-select" value={locale} onChange={(event) => {
      setFailed(false);
      void setLocale(event.target.value as Locale).catch(() => setFailed(true));
    }}>
      <option value="zh-CN" lang="zh-CN">简体中文</option>
      <option value="en" lang="en">English</option>
      <option value="ja" lang="ja">日本語</option>
    </select>
    {failed && <span role="status">{t("语言设置保存失败，重启后可能恢复原语言。")}</span>}
  </div>;
}
