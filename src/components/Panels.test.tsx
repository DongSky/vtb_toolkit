import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
const listenHandlers: Record<string, (e: { payload: unknown }) => void> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, cb: (e: { payload: unknown }) => void) => {
    listenHandlers[name] = cb;
    return Promise.resolve(() => delete listenHandlers[name]);
  },
}));

import DanmakuPanel from "./DanmakuPanel";
import RecorderPanel from "./RecorderPanel";
import OfflinePanel from "./OfflinePanel";

beforeEach(() => {
  invokeMock.mockReset();
  localStorage.clear();
  for (const k of Object.keys(listenHandlers)) delete listenHandlers[k];
});

describe("DanmakuPanel", () => {
  it("connects a room via the danmaku_connect command", async () => {
    invokeMock.mockResolvedValue([]);
    render(<DanmakuPanel />);
    fireEvent.change(screen.getByTestId("dm-room"), {
      target: { value: "12345" },
    });
    fireEvent.click(screen.getByTestId("dm-connect"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("danmaku_connect", {
        roomId: 12345,
      }),
    );
  });

  it("shows backend errors", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "danmaku_connect"
        ? Promise.reject("boom")
        : Promise.resolve([]),
    );
    render(<DanmakuPanel />);
    fireEvent.change(screen.getByTestId("dm-room"), {
      target: { value: "1" },
    });
    fireEvent.click(screen.getByTestId("dm-connect"));
    await waitFor(() =>
      expect(screen.getByTestId("dm-error")).toHaveTextContent("boom"),
    );
  });

  it("renders incoming danmaku events from the event channel", async () => {
    invokeMock.mockResolvedValue([]);
    render(<DanmakuPanel />);
    await waitFor(() => expect(listenHandlers["danmaku://event"]).toBeDefined());
    listenHandlers["danmaku://event"]({
      payload: {
        kind: "danmaku",
        room_id: 1,
        uid: 9,
        username: "测试君",
        text: "初见！",
        timestamp: new Date().toISOString(),
        medal: null,
        guard_level: 0,
        is_admin: false,
        emoticon: null,
      },
    });
    await waitFor(() =>
      expect(screen.getByText("初见！")).toBeInTheDocument(),
    );
  });

  it("theme selector switches theme and editor saves custom theme", async () => {
    invokeMock.mockResolvedValue([]);
    render(<DanmakuPanel />);
    const select = screen.getByTestId("dm-theme-select") as HTMLSelectElement;
    expect(select.options.length).toBe(3);
    fireEvent.click(screen.getByTestId("dm-edit-theme"));
    fireEvent.click(screen.getByTestId("theme-save"));
    // A new custom theme appears in the selector and is selected.
    await waitFor(() => expect(select.options.length).toBe(4));
    expect(select.value.startsWith("custom:")).toBe(true);
  });
});

describe("RecorderPanel", () => {
  it("starts monitoring with segmentation options", async () => {
    invokeMock.mockResolvedValue([]);
    render(<RecorderPanel />);
    fireEvent.change(screen.getByTestId("rec-room"), {
      target: { value: "42" },
    });
    fireEvent.change(screen.getByTestId("rec-dir"), {
      target: { value: "/tmp/rec" },
    });
    fireEvent.change(screen.getByTestId("rec-segment-mode"), {
      target: { value: "duration" },
    });
    fireEvent.change(screen.getByTestId("rec-segment-value"), {
      target: { value: "600" },
    });
    fireEvent.click(screen.getByTestId("rec-start"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("recorder_start", {
        options: {
          room_id: 42,
          output_dir: "/tmp/rec",
          segment_mode: "duration",
          segment_value: 600,
        },
      }),
    );
  });

  it("logs recorder lifecycle events", async () => {
    invokeMock.mockResolvedValue([]);
    render(<RecorderPanel />);
    await waitFor(() =>
      expect(listenHandlers["recorder://event"]).toBeDefined(),
    );
    listenHandlers["recorder://event"]({
      payload: { kind: "started", room_id: 42, output_dir: "/tmp/rec/x" },
    });
    await waitFor(() =>
      expect(screen.getByTestId("rec-log").textContent).toContain(
        "房间 42 开始录制",
      ),
    );
    listenHandlers["recorder://event"]({
      payload: {
        kind: "stopped",
        room_id: 42,
        total_bytes: 1048576 * 10,
        segments: 2,
      },
    });
    await waitFor(() =>
      expect(screen.getByTestId("rec-log").textContent).toContain(
        "录制结束（2 段, 10.0 MB）",
      ),
    );
  });
});

describe("OfflinePanel", () => {
  it("requires input/output/model before starting", () => {
    render(<OfflinePanel />);
    expect(screen.getByTestId("off-start")).toBeDisabled();
  });

  it("invokes offline_process and shows progress + done", async () => {
    invokeMock.mockResolvedValue(undefined);
    render(<OfflinePanel />);
    fireEvent.change(screen.getByTestId("off-input"), {
      target: { value: "/rec/full.flv" },
    });
    fireEvent.change(screen.getByTestId("off-output"), {
      target: { value: "/rec/out" },
    });
    fireEvent.change(screen.getByTestId("off-model"), {
      target: { value: "/models/ggml-small.bin" },
    });
    fireEvent.click(screen.getByTestId("off-start"));
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some((c) => c[0] === "offline_process"),
      ).toBe(true),
    );
    const call = invokeMock.mock.calls.find((c) => c[0] === "offline_process")!;
    expect(call[1].options.input).toBe("/rec/full.flv");
    expect(call[1].options.model_path).toBe("/models/ggml-small.bin");

    listenHandlers["job://progress"]({
      payload: {
        job_id: "j",
        stage: "Transcribe",
        fraction: 0.5,
        message: "语音识别",
      },
    });
    await waitFor(() =>
      expect(screen.getByTestId("off-progress").textContent).toContain(
        "语音识别 50%",
      ),
    );

    listenHandlers["job://done"]({
      payload: {
        job_id: "j",
        ok: true,
        message: "done",
        transcript_segments: 12,
        translations: 12,
        highlights: 3,
        clips: 3,
      },
    });
    await waitFor(() =>
      expect(screen.getByTestId("off-done").textContent).toContain(
        "12 段字幕",
      ),
    );
  });
});
