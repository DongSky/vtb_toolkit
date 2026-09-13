import { t, tm, useLocale } from "../i18n";
import { Component, type ErrorInfo, type ReactNode } from "react";
import LanguageSwitcher from "./LanguageSwitcher";

function ErrorFallback({ error }: { error: string }) {
  useLocale();
  return <main className="app" role="alert">
    <LanguageSwitcher />
    <h2>{t("界面加载失败")}</h2>
    <p>{t("后台任务可能仍在运行。请记下错误信息后重新加载界面。")}</p>
    <pre style={{ whiteSpace: "pre-wrap" }}>{tm(error)}</pre>
    <button onClick={() => window.location.reload()}>{t("重新加载界面")}</button>
  </main>;
}

export default class AppErrorBoundary extends Component<{ children: ReactNode }, { error: string | null }> {
  state: { error: string | null } = { error: null };
  static getDerivedStateFromError(error: Error) {
    return { error: error.message || "未知界面错误" };
  }
  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("VTB Toolkit UI error", error, info.componentStack);
  }
  render() {
    if (!this.state.error) return this.props.children;
    return <ErrorFallback error={this.state.error} />;
  }
}
