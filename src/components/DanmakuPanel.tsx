import { useEffect, useMemo, useState } from "react";
import { usePersisted } from "../hooks/usePersisted";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { DanmakuTranslationPayload, LiveEvent } from "../types";
import type { DanmakuTheme } from "../themes";
import { allThemes, loadCustomThemes, saveCustomThemes } from "../themes";
import DanmakuList from "./DanmakuList";
import ThemeEditor from "./ThemeEditor";

export default function DanmakuPanel() {
  const [roomId, setRoomId] = usePersisted("dm.room", "");
  const [connected, setConnected] = useState<number[]>([]);
  const [events, setEvents] = useState<LiveEvent[]>([]);
  const [themes, setThemes] = useState<DanmakuTheme[]>(() => allThemes());
  const [themeId, setThemeId] = useState(themes[0].id);
  const [editing, setEditing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [trTarget, setTrTarget] = usePersisted("dm.translate_target", "ja");
  const [trOn, setTrOn] = useState(false);
  const [translations, setTranslations] = useState<Record<number, string>>({});

  const theme = useMemo(
    () => themes.find((t) => t.id === themeId) ?? themes[0],
    [themes, themeId],
  );

  useEffect(() => {
    const un = listen<LiveEvent>("danmaku://event", (e) =>
      setEvents((prev) => [...prev.slice(-499), e.payload]),
    );
    const unTr = listen<DanmakuTranslationPayload>(
      "danmaku://translation",
      (e) =>
        setTranslations((prev) => {
          const next = { ...prev, [e.payload.tid]: e.payload.translated };
          // Bound memory: keep the most recent ~500 entries.
          const keys = Object.keys(next);
          if (keys.length > 600) {
            for (const k of keys.slice(0, keys.length - 500)) {
              delete next[Number(k)];
            }
          }
          return next;
        }),
    );
    invoke<number[]>("danmaku_status")
      .then(setConnected)
      .catch(() => {});
    invoke<string | null>("danmaku_translate_status")
      .then((t) => setTrOn(t != null))
      .catch(() => {});
    return () => {
      un.then((f) => f());
      unTr.then((f) => f());
    };
  }, []);

  const toggleTranslate = async () => {
    setError(null);
    try {
      if (trOn) {
        await invoke("danmaku_translate_stop");
        setTrOn(false);
      } else {
        await invoke("danmaku_translate_start", {
          options: { target_lang: trTarget },
        });
        setTrOn(true);
      }
    } catch (e) {
      setError(String(e));
    }
  };

  const connect = async () => {
    setError(null);
    try {
      await invoke("danmaku_connect", { roomId: Number(roomId) });
      setConnected(await invoke<number[]>("danmaku_status"));
    } catch (e) {
      setError(String(e));
    }
  };

  const disconnect = async (id: number) => {
    try {
      await invoke("danmaku_disconnect", { roomId: id });
      setConnected(await invoke<number[]>("danmaku_status"));
    } catch (e) {
      setError(String(e));
    }
  };

  const saveTheme = (t: DanmakuTheme) => {
    const customs = loadCustomThemes().filter((c) => c.id !== t.id);
    customs.push(t);
    saveCustomThemes(customs);
    setThemes(allThemes());
    setThemeId(t.id);
    setEditing(false);
  };

  return (
    <div className="panel" data-testid="danmaku-panel">
      <h2>弹幕</h2>
      <div className="form-row">
        <input
          data-testid="dm-room"
          placeholder="房间号"
          value={roomId}
          onChange={(e) => setRoomId(e.target.value)}
        />
        <button data-testid="dm-connect" disabled={!roomId} onClick={connect}>
          连接
        </button>
        <select
          data-testid="dm-theme-select"
          value={themeId}
          onChange={(e) => setThemeId(e.target.value)}
        >
          {themes.map((t) => (
            <option key={t.id} value={t.id}>
              {t.name}
            </option>
          ))}
        </select>
        <button data-testid="dm-edit-theme" onClick={() => setEditing(!editing)}>
          {editing ? "关闭编辑器" : "编辑主题"}
        </button>
      </div>
      <div className="form-row">
        <label data-testid="dm-translate-label">
          自动翻译
          <select
            data-testid="dm-translate-target"
            value={trTarget}
            disabled={trOn}
            onChange={(e) => setTrTarget(e.target.value)}
          >
            <option value="ja">→ 日本語</option>
            <option value="en">→ English</option>
            <option value="zh">→ 中文</option>
            <option value="ko">→ 한국어</option>
          </select>
        </label>
        <button data-testid="dm-translate-toggle" onClick={toggleTranslate}>
          {trOn ? "停止翻译" : "开启翻译"}
        </button>
        {trOn && <span className="dm-translate-on">翻译中（LLM 设置见「设置」）</span>}
      </div>
      {error && (
        <div className="error" data-testid="dm-error">
          {error}
        </div>
      )}
      <ul data-testid="dm-connected">
        {connected.map((id) => (
          <li key={id}>
            房间 {id}
            <button onClick={() => disconnect(id)}>断开</button>
          </li>
        ))}
      </ul>
      {editing && <ThemeEditor theme={theme} onSave={saveTheme} />}
      <DanmakuList events={events} theme={theme} translations={translations} />
    </div>
  );
}
