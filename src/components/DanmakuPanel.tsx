import { t, t as translate, tm, useLocale } from "../i18n";
import { useEffect, useMemo, useState } from "react";
import { usePersisted } from "../hooks/usePersisted";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isDeleted, roomKey, type ChatDelete, type DanmakuTranslationPayload, type LiveEvent } from "../types";

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
import YoutubeRooms from "./YoutubeRooms";

export default function DanmakuPanel() {
  useLocale();
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
  // 打点: rooms with an active recording session + transient toast.
  const [markerRooms, setMarkerRooms] = useState<string[]>([]);
  const [markerTarget, setMarkerTarget] = useState("");
  const [markerToast, setMarkerToast] = useState<{ source: string; note?: string | null; auto?: boolean; alert?: boolean } | null>(null);
  const [markerNote, setMarkerNote] = useState("");

  const theme = useMemo(
    () => themes.find((t) => t.id === themeId) ?? themes[0],
    [themes, themeId],
  );

  useEffect(() => {
    const un = listen<LiveEvent>("danmaku://event", (e) =>
      setEvents((prev) => [...prev.filter((old) => !e.payload.source?.message_id || old.source?.message_id !== e.payload.source.message_id || roomKey(old) !== roomKey(e.payload)).slice(-499), e.payload]),
    );
    const unDelete = listen<ChatDelete>("danmaku://delete", ({ payload }) => setEvents((prev) => prev.filter((e) => !isDeleted(e, payload))));
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
    invoke<{ uid: number; username: string; note: string; key?: string }[]>("user_notes_list")
      .then((list) => {
        const map: Record<string, string> = {};
        for (const n of list) {
          const key = n.key || (n.uid !== 0 ? `uid:${n.uid}` : `name:${n.username}`);
          map[key] = n.note;
        }
        setNotes(map);
      })
      .catch(() => {});
    invoke<string[]>("twitch_status")
      .then((v) => setTwitchConnected(Array.isArray(v) ? v : []))
      .catch(() => {});
    invoke<string[]>("marker_sources")
      .then((v) => setMarkerRooms(Array.isArray(v) ? v : []))
      .catch(() => {});
    // Recording start/stop changes which rooms accept markers.
    const unRec = listen("recorder://event", () => {
      invoke<string[]>("marker_sources")
        .then((v) => setMarkerRooms(Array.isArray(v) ? v : []))
        .catch(() => {});
    });
    const unPlatform = listen("platform_rec://event", () => {
      invoke<string[]>("marker_sources").then((v) => setMarkerRooms(Array.isArray(v) ? v : [])).catch(() => {});
    });
    // 打点 toast: any marker (hotkey/button/口令/auto alert) flashes here.
    const unMarker = listen<{ room_id: number; room_key?: string; kind: string; note: string | null }>(
      "marker://added",
      (e) => {
        const p = e.payload;
        setMarkerToast(
          { source: p.room_key || `bilibili:${p.room_id}`, note: p.note, auto: p.kind === "auto" },
        );
        setTimeout(() => setMarkerToast(null), 4000);
      },
    );
    const unAlert = listen<{ room_id: number; room_key?: string; reason: string }>(
      "highlight://alert",
      (e) => {
        setMarkerToast({ source: e.payload.room_key || `bilibili:${e.payload.room_id}`, note: e.payload.reason, alert: true });
        setTimeout(() => setMarkerToast(null), 6000);
      },
    );
    return () => {
      un.then((f) => f());
      unDelete.then((f) => f());
      unPlatform.then((f) => f());
      unTr.then((f) => f());
      unMarker.then((f) => f());
      unAlert.then((f) => f());
      unRec.then((f) => f());
    };
  }, []);

  const addMarker = async () => {
    setError(null);
    try {
      // Prefer the entered room; fall back to the sole recording room.
      const target = markerRooms.includes(markerTarget) ? markerTarget : markerRooms.length === 1 ? markerRooms[0] : "";
      if (!target) throw new Error("请选择要打点的录制会话");
      await invoke("marker_add_source", {
        key: target,
        note: markerNote.trim() || null,
      });
      setMarkerNote("");
    } catch (e) {
      setError(String(e));
    }
  };

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

  const editNote = async (uid: number, username: string, userKey?: string) => {
    const key = userKey || (uid !== 0 ? `uid:${uid}` : `name:${username}`);
    const next = window.prompt(t("备注 {0}", username), notes[key] ?? "");
    if (next === null) return;
    try {
      await invoke("user_note_set", { uid, username, userKey, note: next });
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
      <h2>{t("弹幕 / 直播评论")}</h2>
      <YoutubeRooms compact />
      <h3>Bilibili</h3>
      <div className="form-row">
        <input
          data-testid="dm-room"
          placeholder={t("房间号")}
          value={roomId}
          onChange={(e) => setRoomId(e.target.value)}
        />
        <button data-testid="dm-connect" disabled={!roomId} onClick={connect}>
          {t("连接")}{" "}</button>
        <select
          data-testid="dm-theme-select"
          value={themeId}
          onChange={(e) => setThemeId(e.target.value)}
        >
          {themes.map((t) => (
            <option key={t.id} value={t.id}>
              {t.id.startsWith("builtin:") ? translate(t.name) : t.name}
            </option>
          ))}
        </select>
        <button data-testid="dm-edit-theme" onClick={() => setEditing(!editing)}>
          {editing ? t("关闭编辑器") : t("编辑主题")}
        </button>
      </div>
      <div className="form-row">
        <input
          data-testid="dm-twitch-channel"
          placeholder={t("Twitch 频道名")}
          value={twitchChannel}
          onChange={(e) => setTwitchChannel(e.target.value)}
        />
        <button
          data-testid="dm-twitch-connect"
          disabled={!twitchChannel.trim()}
          onClick={connectTwitch}
        >
          {t("连接 Twitch")}{" "}</button>
        {twitchConnected.map((ch) => (
          <span key={ch}>
            #{ch}
            <button onClick={() => disconnectTwitch(ch)}>{t("断开")}</button>
          </span>
        ))}
      </div>
      <div className="form-row">
        <label data-testid="dm-translate-label">
          {t("自动翻译")}{" "}<select
            data-testid="dm-translate-target"
            value={trTarget}
            disabled={trOn}
            onChange={(e) => setTrTarget(e.target.value)}
          >
            <option value="ja">{t("→ 日本語")}</option>
            <option value="en">→ English</option>
            <option value="zh">{t("→ 中文")}</option>
            <option value="ko">→ 한국어</option>
          </select>
        </label>
        <button data-testid="dm-translate-toggle" onClick={toggleTranslate}>
          {trOn ? t("停止翻译") : t("开启翻译")}
        </button>
        {trOn && <span className="dm-translate-on">{t("翻译中（LLM 设置见「设置」）")}</span>}
        <button data-testid="dm-ctrl-toggle" onClick={() => setShowCtrl(!showCtrl)}>
          {showCtrl ? t("收起场控") : t("场控")}
        </button>
      </div>
      <div className="form-row">
        <select aria-label={t("打点录制会话")} value={markerRooms.includes(markerTarget) ? markerTarget : markerRooms.length === 1 ? markerRooms[0] : ""} onChange={(e) => setMarkerTarget(e.target.value)}>
          <option value="">{t("选择录制会话")}</option>
          {markerRooms.map((key) => <option key={key} value={key}>{key}</option>)}
        </select>
        <input
          data-testid="dm-marker-note"
          placeholder={t("打点备注（可空）")}
          value={markerNote}
          maxLength={40}
          onChange={(e) => setMarkerNote(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && markerRooms.length > 0) addMarker();
          }}
        />
        <button
          data-testid="dm-marker-add"
          disabled={markerRooms.length === 0 || (markerRooms.length > 1 && !markerRooms.includes(markerTarget))}
          onClick={addMarker}
          title={t("全局快捷键 Cmd/Ctrl+Shift+M；房管弹幕发「打点 备注」也可触发")}
        >
          {t("打点 ⏱")}{" "}</button>
        {markerRooms.length === 0 && (
          <span className="hint">{t("录制开始后可打点（Ctrl/Cmd+Shift+M）")}</span>
        )}
        {markerToast && (
          <span className="dm-marker-toast" data-testid="dm-marker-toast">
            {t(markerToast.alert ? "疑似高能 {0}" : markerToast.auto ? "已打点 {0}（自动）" : "已打点 {0}", markerToast.source)}{markerToast.note && `: ${markerToast.auto || markerToast.alert ? tm(markerToast.note) : markerToast.note}`}
          </span>
        )}
      </div>
      {showCtrl && (
        <div className="dm-ctrl" data-testid="dm-ctrl">
          <div className="form-row">
            <input
              data-testid="dm-send-text"
              placeholder={t("以自己账号发送弹幕（需登录）")}
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
              {t("发送")}{" "}</button>
            {sendOk && <span className="dm-send-ok">{tm(sendOk)}</span>}
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
                  {t("答谢礼物")}{" "}</label>
                <label>
                  <input
                    type="checkbox"
                    data-testid="dm-thank-guard"
                    checked={thank.thank_guard}
                    onChange={(e) => updateThank({ thank_guard: e.target.checked })}
                  />
                  {t("答谢舰长")}{" "}</label>
                <label>
                  <input
                    type="checkbox"
                    data-testid="dm-thank-sc"
                    checked={thank.thank_sc}
                    onChange={(e) => updateThank({ thank_sc: e.target.checked })}
                  />
                  {t("答谢SC")}{" "}</label>
              </div>
              {thank.thank_gift && (
                <div className="form-row">
                  <input
                    data-testid="dm-thank-gift-template"
                    value={thank.gift_template}
                    onChange={(e) => updateThank({ gift_template: e.target.value })}
                  />
                  <span className="hint">{t("占位符")}{" "}{"{user} {gift} {count} {price}"}</span>
                </div>
              )}
            </div>
          )}
          <div className="form-row">
            <input
              data-testid="dm-timer-text"
              placeholder={t("定时弹幕内容")}
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
            <span>{t("秒/次")}</span>
            <button
              data-testid="dm-timer-toggle"
              disabled={!timerRunning && (!timerText.trim() || !roomNum)}
              onClick={toggleTimer}
            >
              {timerRunning ? t("停止定时") : t("启动定时")}
            </button>
          </div>
        </div>
      )}
      {error && (
        <div className="error" data-testid="dm-error">
          {tm(error)}
        </div>
      )}
      <ul data-testid="dm-connected">
        {connected.map((id) => (
          <li key={id}>
            {t("房间")}{" "}{id}
            <button onClick={() => disconnect(id)}>{t("断开")}</button>
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
