import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import RoomsPanel from "./RoomsPanel";

function card(id: number, live = 1) {
  return {
    room_id: id,
    real_room_id: id,
    title: `直播间${id}`,
    uname: `主播${id}`,
    cover: "",
    area: "虚拟·杂谈",
    live_status: live,
    live_start_time: null,
    danmaku_connected: false,
    recording: false,
  };
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("RoomsPanel", () => {
  it("adds a room, fetches info, shows status", async () => {
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "config_load") return Promise.resolve({});
      if (cmd === "room_info")
        return Promise.resolve(card(Number(args!.roomId)));
      return Promise.resolve();
    });
    render(<RoomsPanel />);
    // Wait for both usePersisted hooks to hydrate before interacting, so
    // the debounced write-back is armed.
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === "config_load").length,
      ).toBeGreaterThanOrEqual(2),
    );
    fireEvent.change(screen.getByTestId("rooms-add-input"), {
      target: { value: "24158116" },
    });
    fireEvent.click(screen.getByTestId("rooms-add"));
    await waitFor(() =>
      expect(screen.getByTestId("room-24158116")).toBeInTheDocument(),
    );
    await waitFor(() =>
      expect(screen.getByTestId("room-24158116-status")).toHaveTextContent(
        "直播中",
      ),
    );
    expect(screen.getByText("直播间24158116")).toBeInTheDocument();
    // Persisted the list.
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) => c[0] === "config_set" && c[1].key === "rooms.list",
        ),
      ).toBe(true),
    );
  });

  it("hydrates saved rooms from config", async () => {
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "config_load")
        return Promise.resolve({ "rooms.list": [320], "rec.outputDir": "/rec" });
      if (cmd === "room_info") return Promise.resolve(card(Number(args!.roomId), 0));
      return Promise.resolve();
    });
    render(<RoomsPanel />);
    await waitFor(() =>
      expect(screen.getByTestId("room-320")).toBeInTheDocument(),
    );
    await waitFor(() =>
      expect(screen.getByTestId("room-320-status")).toHaveTextContent("未开播"),
    );
  });

  it("starts danmaku from a card", async () => {
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "config_load")
        return Promise.resolve({ "rooms.list": [55] });
      if (cmd === "room_info") return Promise.resolve(card(Number(args!.roomId)));
      return Promise.resolve();
    });
    render(<RoomsPanel />);
    await waitFor(() =>
      expect(screen.getByTestId("room-55-danmaku")).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId("room-55-danmaku"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("danmaku_connect", {
        roomId: 55,
      }),
    );
  });
});
