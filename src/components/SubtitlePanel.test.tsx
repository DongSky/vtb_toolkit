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

import SubtitlePanel from "./SubtitlePanel";

beforeEach(() => {
  invokeMock.mockReset();
  for (const k of Object.keys(listenHandlers)) delete listenHandlers[k];
});

describe("SubtitlePanel", () => {
  it("requires room and model before start", () => {
    render(<SubtitlePanel />);
    expect(screen.getByTestId("sub-start")).toBeDisabled();
  });

  it("starts live subtitles with options", async () => {
    invokeMock.mockResolvedValue(undefined);
    render(<SubtitlePanel />);
    fireEvent.change(screen.getByTestId("sub-room"), {
      target: { value: "77" },
    });
    fireEvent.change(screen.getByTestId("sub-model"), {
      target: { value: "/m/ggml-small.bin" },
    });
    fireEvent.click(screen.getByTestId("sub-start"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("live_subtitle_start", {
        options: {
          room_id: 77,
          model_path: "/m/ggml-small.bin",
          translate: false,
          llm_provider: undefined,
          llm_base_url: undefined,
          llm_model: undefined,
          hotword_tables: [],
        },
      }),
    );
    // Toggles to stop button.
    expect(screen.getByTestId("sub-stop")).toBeInTheDocument();
  });

  it("renders incoming subtitle segments with translation", async () => {
    render(<SubtitlePanel />);
    await waitFor(() =>
      expect(listenHandlers["subtitle://segment"]).toBeDefined(),
    );
    listenHandlers["subtitle://segment"]({
      payload: {
        room_id: 77,
        start_ms: 12_500,
        end_ms: 15_000,
        text: "こんばんは",
        lang: "ja",
        translated: "晚上好",
      },
    });
    await waitFor(() => {
      expect(screen.getByText("こんばんは")).toBeInTheDocument();
      expect(screen.getByText("晚上好")).toBeInTheDocument();
      expect(screen.getByText("12.5s")).toBeInTheDocument();
    });
  });

  it("shows start errors", async () => {
    invokeMock.mockRejectedValue("model not found");
    render(<SubtitlePanel />);
    fireEvent.change(screen.getByTestId("sub-room"), {
      target: { value: "1" },
    });
    fireEvent.change(screen.getByTestId("sub-model"), {
      target: { value: "/nope.bin" },
    });
    fireEvent.click(screen.getByTestId("sub-start"));
    await waitFor(() =>
      expect(screen.getByTestId("sub-error")).toHaveTextContent(
        "model not found",
      ),
    );
  });
});
