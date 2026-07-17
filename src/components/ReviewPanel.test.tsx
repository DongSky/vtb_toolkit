import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import ReviewPanel from "./ReviewPanel";

beforeEach(() => invokeMock.mockReset());

describe("ReviewPanel", () => {
  it("loads analysis, shows highlights and stats report", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "config_load") return Promise.resolve({});
      if (cmd === "review_load")
        return Promise.resolve({
          signals: {
            window_ms: 10000,
            total_ms: 60000,
            density: [1, 5, 2],
            keyword: null,
            gift: null,
            audio: [0.1, 0.5, 0.2],
          },
          highlights: [
            {
              start_ms: 10000,
              end_ms: 20000,
              score: 0.9,
              reason: "弹幕密度峰值",
              title: "AI标题",
            },
          ],
          clips: ["/x/clips/clip_10s-20s.mp4"],
          video: "/x/rec.flv",
        });
      if (cmd === "stats_report")
        return Promise.resolve({
          danmaku_count: 42,
          unique_chatters: 7,
          sc_total: 30,
          gift_total: 5.5,
          revenue_total: 35.5,
        });
      return Promise.resolve();
    });
    render(<ReviewPanel />);
    fireEvent.change(screen.getByTestId("review-dir"), {
      target: { value: "/x/out" },
    });
    fireEvent.click(screen.getByTestId("review-load"));

    await waitFor(() =>
      expect(screen.getByTestId("review-canvas")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("review-highlights").textContent).toContain(
      "AI标题",
    );
    expect(screen.getByTestId("review-report").textContent).toContain(
      "弹幕 42 条",
    );
    expect(screen.getByTestId("review-report").textContent).toContain(
      "¥35.5",
    );
  });

  it("shows load errors", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "review_load" ? Promise.reject("目录不存在") : Promise.resolve({}),
    );
    render(<ReviewPanel />);
    fireEvent.change(screen.getByTestId("review-dir"), {
      target: { value: "/nope" },
    });
    fireEvent.click(screen.getByTestId("review-load"));
    await waitFor(() =>
      expect(screen.getByTestId("review-error")).toHaveTextContent(
        "目录不存在",
      ),
    );
  });
});
