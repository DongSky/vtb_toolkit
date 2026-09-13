import { t, t as translate, tm, useLocale } from "./i18n";
import { useEffect, useState } from "react";
import "./App.css";
import DanmakuPanel from "./components/DanmakuPanel";
import RecorderPanel from "./components/RecorderPanel";
import OfflinePanel from "./components/OfflinePanel";
import SubtitlePanel from "./components/SubtitlePanel";
import YoutubeUpload from "./components/YoutubeUpload";
import LoginPanel from "./components/LoginPanel";
import ObsPanel from "./components/ObsPanel";
import RoomsPanel from "./components/RoomsPanel";
import ReviewPanel from "./components/ReviewPanel";
import HotwordsPanel from "./components/HotwordsPanel";
import LanguageSwitcher from "./components/LanguageSwitcher";
import AiSettingsPanel from "./components/AiSettingsPanel";
import type { ServiceKind } from "./hooks/useAiSettings";
import { openUrl } from "@tauri-apps/plugin-opener";
import { invoke } from "@tauri-apps/api/core";

type Tab = "rooms" | "danmaku" | "recorder" | "subtitle" | "offline" | "review" | "hotwords" | "obs" | "account" | "settings";

const TABS: { id: Tab; label: string }[] = [
  { id: "rooms", label: "房间" },
  { id: "danmaku", label: "弹幕" },
  { id: "recorder", label: "录制" },
  { id: "subtitle", label: "实时字幕" },
  { id: "offline", label: "离线处理" },
  { id: "review", label: "复盘" },
  { id: "hotwords", label: "热词表" },
  { id: "obs", label: "OBS输出" },
  { id: "account", label: "账号" },
  { id: "settings", label: "设置" },
];

function App() {
  const locale = useLocale();
  const [tab, setTab] = useState<Tab>("danmaku");
  const [guideError, setGuideError] = useState<string | null>(null);
  const [aiKind, setAiKind] = useState<ServiceKind>("text");
  useEffect(() => {
    const open = (event: Event) => { setAiKind((event as CustomEvent<ServiceKind>).detail ?? "text"); setTab("settings"); };
    window.addEventListener("open-ai-settings", open);
    return () => window.removeEventListener("open-ai-settings", open);
  }, []);
  useEffect(() => {
    void import("./youtube/service").then((m) => m.startYoutubeBridge()).catch(console.error);
  }, []);

  return (
    <main className="app">
      <header className="app-toolbar">
        <strong>VTB Toolkit</strong>
        <LanguageSwitcher />
        <button onClick={() => {
          setGuideError(null);
          const url = new URL(`/guide/index.html?lang=${locale}`, window.location.href).href;
          if (!("__TAURI_INTERNALS__" in window)) window.open(url, "_blank", "noopener");
          else void invoke<string>("guide_open").then((base) => openUrl(`${base}?lang=${locale}`)).catch((error) => setGuideError(String(error)));
        }}>{t("使用教程")}</button>
      </header>
      {guideError && <p role="alert" className="error">{t("教程打开失败: {0}", tm(guideError))}</p>}
      <nav className="tabs" data-testid="tabs">
        {TABS.map((t) => (
          <button
            key={t.id}
            data-testid={`tab-${t.id}`}
            className={tab === t.id ? "tab active" : "tab"}
            onClick={() => setTab(t.id)}
          >
            {translate(t.label)}
          </button>
        ))}
      </nav>
      {tab === "rooms" && <RoomsPanel />}
      {tab === "danmaku" && <DanmakuPanel />}
      {tab === "recorder" && <RecorderPanel />}
      {tab === "subtitle" && <SubtitlePanel />}
      {tab === "offline" && <OfflinePanel />}
      {tab === "review" && <ReviewPanel />}
      {tab === "hotwords" && <HotwordsPanel />}
      {tab === "obs" && <ObsPanel />}
      {tab === "account" && <><LoginPanel /><YoutubeUpload /></>}
      {tab === "settings" && <AiSettingsPanel initialKind={aiKind} />}
    </main>
  );
}

export default App;
