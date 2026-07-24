import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
  convertFileSrc: (p: string) => `asset://localhost/${encodeURIComponent(p)}`,
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
          word_freq: [
            ["草", 20],
            ["哈哈哈", 8],
          ],
          top_chatters: [
            ["观众A", 15],
            ["观众B", 6],
          ],
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
    // Word cloud + top chatters rendered from previously-ignored fields.
    expect(screen.getByTestId("word-cloud")).toBeInTheDocument();
    expect(screen.getByText("草")).toBeInTheDocument();
    expect(screen.getByTestId("review-top-chatters").textContent).toContain(
      "观众A",
    );
  });

  it("opens the built-in preview player via the asset protocol", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "config_load") return Promise.resolve({});
      if (cmd === "review_load")
        return Promise.resolve({
          signals: {
            window_ms: 10000,
            total_ms: 60000,
            density: [1, 2],
            keyword: null,
            gift: null,
            audio: null,
          },
          highlights: [],
          clips: ["/x/clips/a.mp4"],
          video: "/x/rec.flv",
        });
      return Promise.resolve(null);
    });
    render(<ReviewPanel />);
    fireEvent.change(screen.getByTestId("review-dir"), {
      target: { value: "/x/out" },
    });
    fireEvent.click(screen.getByTestId("review-load"));
    await waitFor(() =>
      expect(screen.getByTestId("review-preview-video")).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByTestId("review-preview-video"));
    const player = screen.getByTestId("review-player") as HTMLVideoElement;
    expect(player.getAttribute("src")).toContain("asset://localhost/");
    // Clicking again closes it.
    fireEvent.click(screen.getByTestId("review-preview-video"));
    expect(screen.queryByTestId("review-player")).not.toBeInTheDocument();
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

  it("renders 打点 + SC timeline entries and exports timestamps", async () => {
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
            audio: null,
          },
          highlights: [],
          clips: [],
          video: null,
        });
      if (cmd === "marker_timeline")
        return Promise.resolve({
          markers: [
            { at_ms: 15000, kind: "manual", source: "hotkey", note: "名场面" },
            { at_ms: 30000, kind: "auto", source: "realtime", note: "弹幕爆发" },
          ],
          events: [
            { at_ms: 5000, kind: "super_chat", label: "SC ¥30 观众A: 加油" },
          ],
        });
      if (cmd === "timestamps_export")
        return Promise.resolve("/x/out/timestamps.txt");
      return Promise.resolve(null);
    });
    render(<ReviewPanel />);
    fireEvent.change(screen.getByTestId("review-dir"), {
      target: { value: "/x/out" },
    });
    fireEvent.click(screen.getByTestId("review-load"));

    await waitFor(() =>
      expect(screen.getByTestId("review-markers")).toBeInTheDocument(),
    );
    const list = screen.getByTestId("review-markers").textContent!;
    expect(list).toContain("名场面");
    expect(list).toContain("SC ¥30");
    // Sorted by time: SC (5s) before manual marker (15s).
    expect(list.indexOf("SC ¥30")).toBeLessThan(list.indexOf("名场面"));

    fireEvent.click(screen.getByTestId("review-ts-export"));
    await waitFor(() =>
      expect(screen.getByTestId("review-export-msg")).toHaveTextContent(
        "时间戳清单已导出",
      ),
    );
    expect(invokeMock).toHaveBeenCalledWith("timestamps_export", {
      dir: "/x/out",
    });
  });

  it("exports editor projects (FCPXML / 剪映 / 素材包)", async () => {
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
            audio: null,
          },
          highlights: [
            {
              start_ms: 10000,
              end_ms: 20000,
              score: 0.9,
              reason: "峰值",
              title: "名场面",
            },
          ],
          clips: [],
          video: "/x/rec.flv",
        });
      if (cmd === "fcpxml_export") return Promise.resolve("/x/out/highlights.fcpxml");
      if (cmd === "jianying_export") return Promise.resolve("/x/out/jianying/s");
      if (cmd === "asset_bundle_export")
        return Promise.resolve({ dir: "/x/out/素材包", files: ["a", "b"] });
      return Promise.resolve(null);
    });
    render(<ReviewPanel />);
    fireEvent.change(screen.getByTestId("review-dir"), {
      target: { value: "/x/out" },
    });
    fireEvent.click(screen.getByTestId("review-load"));
    await waitFor(() =>
      expect(screen.getByTestId("review-fcpxml-export")).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByTestId("review-fcpxml-export"));
    await waitFor(() =>
      expect(screen.getByTestId("review-export-msg")).toHaveTextContent(
        "FCPXML 已导出",
      ),
    );

    fireEvent.click(screen.getByTestId("review-jianying-export"));
    await waitFor(() =>
      expect(screen.getByTestId("review-export-msg")).toHaveTextContent(
        "剪映草稿已导出",
      ),
    );

    fireEvent.click(screen.getByTestId("review-bundle-export"));
    await waitFor(() =>
      expect(screen.getByTestId("review-export-msg")).toHaveTextContent(
        "素材包已导出 (2 个文件)",
      ),
    );
  });

  it("review loads fine when marker timeline is unavailable", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "config_load") return Promise.resolve({});
      if (cmd === "review_load")
        return Promise.resolve({
          signals: {
            window_ms: 10000,
            total_ms: 60000,
            density: [1, 2],
            keyword: null,
            gift: null,
            audio: null,
          },
          highlights: [],
          clips: [],
          video: null,
        });
      if (cmd === "marker_timeline") return Promise.reject("无会话");
      return Promise.resolve(null);
    });
    render(<ReviewPanel />);
    fireEvent.change(screen.getByTestId("review-dir"), {
      target: { value: "/x/out" },
    });
    fireEvent.click(screen.getByTestId("review-load"));
    await waitFor(() =>
      expect(screen.getByTestId("review-canvas")).toBeInTheDocument(),
    );
    expect(screen.queryByTestId("review-markers")).not.toBeInTheDocument();
  });
});
