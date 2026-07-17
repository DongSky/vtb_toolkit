import { useState } from "react";
import "./App.css";
import DanmakuPanel from "./components/DanmakuPanel";
import RecorderPanel from "./components/RecorderPanel";
import OfflinePanel from "./components/OfflinePanel";
import SubtitlePanel from "./components/SubtitlePanel";
import LoginPanel from "./components/LoginPanel";
import ObsPanel from "./components/ObsPanel";

type Tab = "danmaku" | "recorder" | "subtitle" | "offline" | "obs" | "account";

const TABS: { id: Tab; label: string }[] = [
  { id: "danmaku", label: "弹幕" },
  { id: "recorder", label: "录制" },
  { id: "subtitle", label: "实时字幕" },
  { id: "offline", label: "离线处理" },
  { id: "obs", label: "OBS输出" },
  { id: "account", label: "账号" },
];

function App() {
  const [tab, setTab] = useState<Tab>("danmaku");

  return (
    <main className="app">
      <nav className="tabs" data-testid="tabs">
        {TABS.map((t) => (
          <button
            key={t.id}
            data-testid={`tab-${t.id}`}
            className={tab === t.id ? "tab active" : "tab"}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </nav>
      {tab === "danmaku" && <DanmakuPanel />}
      {tab === "recorder" && <RecorderPanel />}
      {tab === "subtitle" && <SubtitlePanel />}
      {tab === "offline" && <OfflinePanel />}
      {tab === "obs" && <ObsPanel />}
      {tab === "account" && <LoginPanel />}
    </main>
  );
}

export default App;
