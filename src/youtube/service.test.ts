import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mock = vi.hoisted(() => ({ invoke: vi.fn(), getInfo: vi.fn(), execute: vi.fn(), handlers: {} as Record<string, (e: { payload: any }) => void> }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mock.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (name, handler) => { mock.handlers[name] = handler; return () => {}; }) }));
vi.mock("youtubei.js/web", () => ({ Innertube: { create: vi.fn(async () => ({ getInfo: mock.getInfo })) } }));

const event = { type: "AddChatItemAction", item: { type: "LiveChatTextMessage", id: "msg", author: { id: "UC1", name: "观众" }, message: "你好" } };
const info = () => ({ basic_info: { id: "abcdefghijk", title: "直播", is_live: true }, livechat: { continuation: "token", is_replay: false }, actions: { execute: mock.execute } });
async function settle() { for (let i = 0; i < 40; i++) await Promise.resolve(); }
function request(id: number, method: string, args: object) { mock.handlers["youtube://request"]({ payload: { id, method, args } }); }

beforeEach(async () => {
  vi.useFakeTimers(); vi.resetModules(); mock.invoke.mockReset(); mock.getInfo.mockReset(); mock.execute.mockReset(); mock.handlers = {};
  mock.invoke.mockResolvedValue(true); mock.getInfo.mockResolvedValue(info());
  mock.execute.mockResolvedValue({ continuation_contents: { continuation: { token: "next", timeout_ms: 1000 }, actions: [event, event] } });
  await (await import("./service")).startYoutubeBridge();
});
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });

describe("YouTube connection lifecycle", () => {
  it("restores recording and preview consumers after a WebView reload", async () => {
    vi.resetModules();
    mock.invoke.mockImplementation(async (command) => command === "youtube_bridge_ready" ? [{ video_id: "abcdefghijk", title: "直播", consumers: ["ui", "record:session"] }] : true);
    await (await import("./service")).startYoutubeBridge(); await settle();
    expect(mock.invoke).toHaveBeenCalledWith("youtube_connection", expect.objectContaining({ status: expect.objectContaining({ consumers: ["ui", "record:session"] }) }));
    request(9, "disconnect", { video_id: "abcdefghijk", consumer: "ui" }); await settle();
    await vi.advanceTimersByTimeAsync(1000);
    expect(mock.execute).toHaveBeenCalledTimes(2);
  });
  it("deduplicates messages and retains recording when preview disconnects", async () => {
    request(1, "connect", { input: "abcdefghijk", consumer: "ui" }); await settle();
    request(2, "connect", { input: "abcdefghijk", consumer: "record:session" }); await settle();
    expect(mock.invoke.mock.calls.filter(([cmd]) => cmd === "youtube_event")).toHaveLength(1);
    request(3, "disconnect", { video_id: "abcdefghijk" }); await settle();
    await vi.advanceTimersByTimeAsync(1000);
    expect(mock.execute).toHaveBeenCalledTimes(2);
    request(4, "disconnect", { video_id: "abcdefghijk", consumer: "record:session" }); await settle();
    await vi.advanceTimersByTimeAsync(10000);
    expect(mock.execute).toHaveBeenCalledTimes(2);
    expect(mock.invoke).toHaveBeenCalledWith("youtube_connection", expect.objectContaining({ status: expect.objectContaining({ state: "stopped" }) }));
  });
  it("does not create a connection after a native request is cancelled", async () => {
    let resolve!: (value: unknown) => void;
    mock.getInfo.mockImplementation(() => new Promise((r) => { resolve = r; }));
    request(1, "connect", { input: "abcdefghijk" }); await settle();
    mock.handlers["youtube://cancel"]({ payload: 1 }); resolve(info()); await settle();
    expect(mock.execute).not.toHaveBeenCalled();
    expect(mock.invoke.mock.calls.some(([cmd]) => cmd === "youtube_event")).toBe(false);
  });
  it("forwards message removal with the actual video and message IDs", async () => {
    mock.execute.mockResolvedValue({ continuation_contents: { continuation: { token: "next" }, actions: [{ type: "MarkChatItemAsDeletedAction", target_item_id: "gone" }] } });
    request(1, "connect", { input: "abcdefghijk" }); await settle();
    expect(mock.invoke).toHaveBeenCalledWith("youtube_delete", { videoId: "abcdefghijk", messageId: "gone", userId: null });
  });
});
