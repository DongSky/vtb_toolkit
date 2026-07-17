import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import DanmakuList from "./DanmakuList";
import { BUILTIN_THEMES } from "../themes";
import type { LiveEvent } from "../types";

const theme = BUILTIN_THEMES[0];

function danmaku(text: string, extra: Partial<LiveEvent> = {}): LiveEvent {
  return {
    kind: "danmaku",
    room_id: 1,
    uid: 1,
    username: "观众A",
    text,
    timestamp: new Date().toISOString(),
    medal: null,
    guard_level: 0,
    is_admin: false,
    emoticon: null,
    ...extra,
  } as LiveEvent;
}

describe("DanmakuList", () => {
  it("renders plain danmaku with username and text", () => {
    render(<DanmakuList events={[danmaku("你好")]} theme={theme} />);
    expect(screen.getByText("观众A:")).toBeInTheDocument();
    expect(screen.getByText("你好")).toBeInTheDocument();
  });

  it("renders medal when theme shows it", () => {
    const ev = danmaku("带牌子", {
      medal: { name: "小铃铛", level: 21, anchor_room_id: 5 },
    } as Partial<LiveEvent>);
    render(<DanmakuList events={[ev]} theme={{ ...theme, showMedal: true }} />);
    expect(screen.getByText("小铃铛·21")).toBeInTheDocument();
  });

  it("hides medal when theme disables it", () => {
    const ev = danmaku("无牌子显示", {
      medal: { name: "小铃铛", level: 21, anchor_room_id: 5 },
    } as Partial<LiveEvent>);
    render(<DanmakuList events={[ev]} theme={{ ...theme, showMedal: false }} />);
    expect(screen.queryByText("小铃铛·21")).not.toBeInTheDocument();
  });

  it("renders super chat with price", () => {
    const sc: LiveEvent = {
      kind: "super_chat",
      room_id: 1,
      uid: 2,
      username: "金主",
      text: "加油！",
      price: 30,
      duration_secs: 60,
      timestamp: new Date().toISOString(),
    };
    render(<DanmakuList events={[sc]} theme={theme} />);
    expect(screen.getByTestId("dm-sc")).toBeInTheDocument();
    expect(screen.getByText("¥30")).toBeInTheDocument();
  });

  it("renders gift with value only when paid", () => {
    const gift: LiveEvent = {
      kind: "gift",
      room_id: 1,
      uid: 3,
      username: "土豪",
      gift_name: "小花花",
      gift_id: 1,
      count: 10,
      total_price: 5,
      timestamp: new Date().toISOString(),
    };
    render(<DanmakuList events={[gift]} theme={theme} />);
    expect(screen.getByText(/投喂 小花花 ×10（¥5.0）/)).toBeInTheDocument();
  });

  it("renders guard purchase", () => {
    const guard: LiveEvent = {
      kind: "guard_buy",
      room_id: 1,
      uid: 4,
      username: "新舰长",
      guard_level: 3,
      count: 1,
      price: 198,
      timestamp: new Date().toISOString(),
    };
    render(<DanmakuList events={[guard]} theme={theme} />);
    expect(screen.getByText(/开通 舰长 ×1/)).toBeInTheDocument();
  });

  it("caps rendered rows at maxRows", () => {
    const events = Array.from({ length: 30 }, (_, i) => danmaku(`msg-${i}`));
    render(<DanmakuList events={events} theme={theme} maxRows={10} />);
    expect(screen.getAllByTestId("dm-row").length).toBe(10);
    expect(screen.getByText("msg-29")).toBeInTheDocument();
    expect(screen.queryByText("msg-0")).not.toBeInTheDocument();
  });

  it("applies theme vars to the container", () => {
    render(<DanmakuList events={[]} theme={theme} />);
    const list = screen.getByTestId("dm-list");
    expect(list.style.getPropertyValue("--dm-bg")).toBe(
      theme.vars["--dm-bg"],
    );
  });

  it("ignores watched_change events silently", () => {
    const ev: LiveEvent = { kind: "watched_change", room_id: 1, count: 5 };
    render(<DanmakuList events={[ev]} theme={theme} />);
    expect(screen.queryAllByTestId(/dm-/).length).toBe(1); // only the list itself
  });
});
