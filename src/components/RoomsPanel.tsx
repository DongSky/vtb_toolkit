import { t, tm, useLocale } from "../i18n";
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { usePersisted } from "../hooks/usePersisted";

import YoutubeRooms from "./YoutubeRooms";

export interface RoomCard {
  room_id: number;
  real_room_id: number;
  title: string;
  uname: string;
  cover: string;
  area: string;
  live_status: number;
  live_start_time: string | null;
  danmaku_connected: boolean;
  recording: boolean;
}

const STATUS: Record<number, string> = { 0: "未开播", 1: "直播中", 2: "轮播" };

export default function RoomsPanel() {
  useLocale();
  const [rooms, setRooms] = usePersisted<number[]>("rooms.list", []);
  const [outputDir] = usePersisted("rec.outputDir", "");
  const [cards, setCards] = useState<Record<number, RoomCard>>({});
  const [newRoom, setNewRoom] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async (list: number[]) => {
    for (const id of list) {
      try {
        const card = await invoke<RoomCard>("room_info", { roomId: id });
        setCards((c) => ({ ...c, [id]: card }));
      } catch {
        /* room fetch failed; keep stale card */
      }
    }
  }, []);

  useEffect(() => {
    if (rooms.length) refresh(rooms);
    const t = setInterval(() => rooms.length && refresh(rooms), 30_000);
    return () => clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rooms]);

  const addRoom = () => {
    const id = Number(newRoom);
    if (!id || rooms.includes(id)) return;
    const next = [...rooms, id];
    setRooms(next);
    setNewRoom("");
    refresh([id]);
  };

  const removeRoom = (id: number) => {
    setRooms(rooms.filter((r) => r !== id));
    setCards((c) => {
      const { [id]: _gone, ...rest } = c;
      return rest;
    });
  };

  const act = async (cmd: string, args: Record<string, unknown>, id: number) => {
    setError(null);
    try {
      await invoke(cmd, args);
      await refresh([id]);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="panel" data-testid="rooms-panel">
      <h2>{t("房间管理")}</h2>
      <YoutubeRooms />
      <h3>Bilibili</h3>
      <div className="form-row">
        <input
          data-testid="rooms-add-input"
          placeholder={t("房间号")}
          value={newRoom}
          onChange={(e) => setNewRoom(e.target.value)}
        />
        <button data-testid="rooms-add" disabled={!newRoom} onClick={addRoom}>
          {t("添加房间")}{" "}</button>
      </div>
      {error && (
        <div className="error" data-testid="rooms-error">
          {tm(error)}
        </div>
      )}
      <div className="room-grid" data-testid="rooms-grid">
        {rooms.map((id) => {
          const c = cards[id];
          return (
            <div className="room-card" key={id} data-testid={`room-${id}`}>
              {c?.cover ? (
                <img className="room-cover" src={c.cover} alt="" />
              ) : (
                <div className="room-cover placeholder" />
              )}
              <div className="room-body">
                <div className="room-title">{c?.title || t("房间 {0}", id)}</div>
                <div className="room-meta">
                  {c?.uname || "…"} · {c?.area || ""}
                  <span
                    className={`room-status s${c?.live_status ?? 0}`}
                    data-testid={`room-${id}-status`}
                  >
                    {t(STATUS[c?.live_status ?? 0])}
                  </span>
                </div>
                <div className="form-row">
                  {c?.danmaku_connected ? (
                    <button onClick={() => act("danmaku_disconnect", { roomId: c.real_room_id }, id)}>
                      {t("断开弹幕")}{" "}</button>
                  ) : (
                    <button
                      data-testid={`room-${id}-danmaku`}
                      onClick={() => act("danmaku_connect", { roomId: c?.real_room_id ?? id }, id)}
                    >
                      {t("连接弹幕")}{" "}</button>
                  )}
                  {c?.recording ? (
                    <button onClick={() => act("recorder_stop", { roomId: c.real_room_id }, id)}>
                      {t("停止录制")}{" "}</button>
                  ) : (
                    <button
                      data-testid={`room-${id}-record`}
                      disabled={!outputDir}
                      title={outputDir ? "" : t("请先在录制页设置输出目录")}
                      onClick={() =>
                        act(
                          "recorder_start",
                          {
                            options: {
                              room_id: c?.real_room_id ?? id,
                              output_dir: outputDir,
                              segment_mode: "duration",
                              segment_value: 3600,
                            },
                          },
                          id,
                        )
                      }
                    >
                      {t("开始录制")}{" "}</button>
                  )}
                  <button className="danger" onClick={() => removeRoom(id)}>
                    {t("移除")}{" "}</button>
                </div>
              </div>
            </div>
          );
        })}
        {rooms.length === 0 && <p className="login-note">{t("还没有房间，输入房间号添加。")}</p>}
      </div>
    </div>
  );
}
