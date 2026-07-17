import { useState } from "react";
import "./App.css";
import DanmakuPanel from "./components/DanmakuPanel";
import RecorderPanel from "./components/RecorderPanel";
import OfflinePanel from "./components/OfflinePanel";

type Tab = "danmaku" | "recorder" | "offline";

const TABS: { id: Tab; label: string }[] = [
  { id: "danmaku", label: "弹幕" },
  { id: "recorder", label: "录制" },
  { id: "offline", label: "离线处理" },
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
      {tab === "offline" && <OfflinePanel />}
    </main>
  );
}

export default App;
