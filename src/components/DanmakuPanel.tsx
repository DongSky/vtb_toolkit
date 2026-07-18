import { useEffect, useMemo, useState } from "react";
import { usePersisted } from "../hooks/usePersisted";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { DanmakuTranslationPayload, LiveEvent } from "../types";

export interface AutoThankConfig {
  thank_gift: boolean;
  thank_guard: boolean;
  thank_sc: boolean;
  gift_template: string;
  guard_template: string;
  sc_template: string;
  min_gift_price: number;
}
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
  // 场控
  const [showCtrl, setShowCtrl] = useState(false);
  const [sendText, setSendText] = useState("");
  const [sendOk, setSendOk] = useState<string | null>(null);
  const [thank, setThank] = useState<AutoThankConfig | null>(null);
  const [timerText, setTimerText] = usePersisted("dm.timer_text", "");
  const [timerSecs, setTimerSecs] = usePersisted("dm.timer_secs", "300");
  const [timerRooms, setTimerRooms] = useState<number[]>([]);
  // 粉丝画像: viewer notes keyed by note_key.
  const [notes, setNotes] = useState<Record<string, string>>({});
  // Twitch chat.
  const [twitchChannel, setTwitchChannel] = usePersisted("dm.twitch", "");
  const [twitchConnected, setTwitchConnected] = useState<string[]>([]);

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
    invoke<AutoThankConfig>("danmaku_autothank_get")
      .then(setThank)
      .catch(() => {});
    invoke<number[]>("danmaku_timer_status")
      .then(setTimerRooms)
      .catch(() => {});
    invoke<{ uid: number; username: string; note: string }[]>("user_notes_list")
      .then((list) => {
        const map: Record<string, string> = {};
        for (const n of list) {
          const key = n.uid !== 0 ? `uid:${n.uid}` : `name:${n.username}`;
          map[key] = n.note;
        }
        setNotes(map);
      })
      .catch(() => {});
    invoke<string[]>("twitch_status")
      .then((v) => setTwitchConnected(Array.isArray(v) ? v : []))
      .catch(() => {});
    return () => {
      un.then((f) => f());
      unTr.then((f) => f());
    };
  }, []);

  const connectTwitch = async () => {
    setError(null);
    try {
      await invoke("twitch_connect", { channel: twitchChannel });
      setTwitchConnected(await invoke<string[]>("twitch_status"));
    } catch (e) {
      setError(String(e));
    }
  };

  const disconnectTwitch = async (ch: string) => {
    try {
      await invoke("twitch_disconnect", { channel: ch });
      setTwitchConnected(await invoke<string[]>("twitch_status"));
    } catch (e) {
      setError(String(e));
    }
  };

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

  const roomNum = Number(roomId);

  const editNote = async (uid: number, username: string) => {
    const key = uid !== 0 ? `uid:${uid}` : `name:${username}`;
    const next = window.prompt(`备注 ${username}`, notes[key] ?? "");
    if (next === null) return;
    try {
      await invoke("user_note_set", { uid, username, note: next });
      setNotes((prev) => {
        const copy = { ...prev };
        if (next.trim()) copy[key] = next.trim();
        else delete copy[key];
        return copy;
      });
    } catch (e) {
      setError(String(e));
    }
  };

  const sendDanmaku = async () => {
    setError(null);
    setSendOk(null);
    try {
      await invoke("danmaku_send", { roomId: roomNum, text: sendText });
      setSendOk("已发送");
      setSendText("");
    } catch (e) {
      setError(String(e));
    }
  };

  const updateThank = async (patch: Partial<AutoThankConfig>) => {
    if (!thank) return;
    const next = { ...thank, ...patch };
    setThank(next);
    try {
      await invoke("danmaku_autothank_set", { config: next });
    } catch (e) {
      setError(String(e));
    }
  };

  const timerRunning = timerRooms.includes(roomNum);
  const toggleTimer = async () => {
    setError(null);
    try {
      if (timerRunning) {
        await invoke("danmaku_timer_stop", { roomId: roomNum });
      } else {
        await invoke("danmaku_timer_start", {
          roomId: roomNum,
          text: timerText,
          intervalSecs: Number(timerSecs) || 300,
        });
      }
      setTimerRooms(await invoke<number[]>("danmaku_timer_status"));
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
        <input
          data-testid="dm-twitch-channel"
          placeholder="Twitch 频道名"
          value={twitchChannel}
          onChange={(e) => setTwitchChannel(e.target.value)}
        />
        <button
          data-testid="dm-twitch-connect"
          disabled={!twitchChannel.trim()}
          onClick={connectTwitch}
        >
          连接 Twitch
        </button>
        {twitchConnected.map((ch) => (
          <span key={ch}>
            #{ch}
            <button onClick={() => disconnectTwitch(ch)}>断开</button>
          </span>
        ))}
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
        <button data-testid="dm-ctrl-toggle" onClick={() => setShowCtrl(!showCtrl)}>
          {showCtrl ? "收起场控" : "场控"}
        </button>
      </div>
      {showCtrl && (
        <div className="dm-ctrl" data-testid="dm-ctrl">
          <div className="form-row">
            <input
              data-testid="dm-send-text"
              placeholder="以自己账号发送弹幕（需登录）"
              value={sendText}
              maxLength={40}
              onChange={(e) => setSendText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && sendText.trim() && roomNum) sendDanmaku();
              }}
            />
            <button
              data-testid="dm-send-btn"
              disabled={!sendText.trim() || !roomNum}
              onClick={sendDanmaku}
            >
              发送
            </button>
            {sendOk && <span className="dm-send-ok">{sendOk}</span>}
          </div>
          {thank && (
            <div className="dm-thank">
              <div className="form-row">
                <label>
                  <input
                    type="checkbox"
                    data-testid="dm-thank-gift"
                    checked={thank.thank_gift}
                    onChange={(e) => updateThank({ thank_gift: e.target.checked })}
                  />
                  答谢礼物
                </label>
                <label>
                  <input
                    type="checkbox"
                    data-testid="dm-thank-guard"
                    checked={thank.thank_guard}
                    onChange={(e) => updateThank({ thank_guard: e.target.checked })}
                  />
                  答谢舰长
                </label>
                <label>
                  <input
                    type="checkbox"
                    data-testid="dm-thank-sc"
                    checked={thank.thank_sc}
                    onChange={(e) => updateThank({ thank_sc: e.target.checked })}
                  />
                  答谢SC
                </label>
              </div>
              {thank.thank_gift && (
                <div className="form-row">
                  <input
                    data-testid="dm-thank-gift-template"
                    value={thank.gift_template}
                    onChange={(e) => updateThank({ gift_template: e.target.value })}
                  />
                  <span className="hint">占位符 {"{user} {gift} {count} {price}"}</span>
                </div>
              )}
            </div>
          )}
          <div className="form-row">
            <input
              data-testid="dm-timer-text"
              placeholder="定时弹幕内容"
              value={timerText}
              maxLength={40}
              onChange={(e) => setTimerText(e.target.value)}
            />
            <input
              data-testid="dm-timer-secs"
              type="number"
              min={30}
              style={{ width: 80 }}
              value={timerSecs}
              onChange={(e) => setTimerSecs(e.target.value)}
            />
            <span>秒/次</span>
            <button
              data-testid="dm-timer-toggle"
              disabled={!timerRunning && (!timerText.trim() || !roomNum)}
              onClick={toggleTimer}
            >
              {timerRunning ? "停止定时" : "启动定时"}
            </button>
          </div>
        </div>
      )}
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
      <DanmakuList
        events={events}
        theme={theme}
        translations={translations}
        notes={notes}
        onUserClick={editNote}
      />
    </div>
  );
}
