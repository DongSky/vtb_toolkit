import { useEffect, useMemo, useState } from "react";
import { usePersisted } from "../hooks/usePersisted";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { LiveEvent } from "../types";
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

  const theme = useMemo(
    () => themes.find((t) => t.id === themeId) ?? themes[0],
    [themes, themeId],
  );

  useEffect(() => {
    const un = listen<LiveEvent>("danmaku://event", (e) =>
      setEvents((prev) => [...prev.slice(-499), e.payload]),
    );
    invoke<number[]>("danmaku_status")
      .then(setConnected)
      .catch(() => {});
    return () => {
      un.then((f) => f());
    };
  }, []);

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
      <DanmakuList events={events} theme={theme} />
    </div>
  );
}
