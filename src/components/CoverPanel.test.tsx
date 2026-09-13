import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
  convertFileSrc: (p: string) => `asset://localhost/${encodeURIComponent(p)}`,
}));

import CoverPanel from "./CoverPanel";

beforeEach(() => {
  invokeMock.mockReset();
  localStorage.clear();
});

describe("CoverPanel", () => {
  it("extracts candidate frames and lists them", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "cover_list")
        return Promise.resolve(["/x/out/cover/frame_00_1000.jpg"]);
      if (cmd === "cover_extract_frames")
        return Promise.resolve(["/x/out/cover/frame_00_1000.jpg"]);
      return Promise.resolve(null);
    });
    render(<CoverPanel dir="/x/out" />);
    await waitFor(() => expect(screen.getByTestId("cover-grid")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("cover-extract"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("cover_extract_frames", {
        dir: "/x/out",
      }),
    );
  });

  it("generates ideas via the LLM and shows them", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "cover_list") return Promise.resolve([]);
      if (cmd === "cover_ideas")
        return Promise.resolve([
          {
            title: "猫耳歌回",
            subtitle: "新歌初披露",
            composition: "脸右置",
            palette: "粉紫",
            platform: "B站 16:10",
            image_prompt: "anime cat-ear vtuber singing",
          },
        ]);
      return Promise.resolve(null);
    });
    render(<CoverPanel dir="/x/out" />);
    fireEvent.click(screen.getByTestId("cover-ideas"));
    await waitFor(() =>
      expect(screen.getByTestId("cover-ideas-list").textContent).toContain(
        "猫耳歌回",
      ),
    );
    // "用作生成 prompt" fills the generation textarea.
    fireEvent.click(screen.getByTestId("cover-use-prompt-0"));
    expect(
      (screen.getByTestId("cover-gen-prompt") as HTMLTextAreaElement).value,
    ).toContain("anime cat-ear vtuber singing");
  });

  it("generates an image using saved backend settings and selected references", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "cover_list")
        return Promise.resolve(["/x/out/cover/frame_00_1000.jpg"]);
      if (cmd === "cover_generate")
        return Promise.resolve(["/x/out/cover/gen_000.png"]);
      return Promise.resolve(null);
    });
    render(<CoverPanel dir="/x/out" />);
    await waitFor(() => expect(screen.getByTestId("cover-grid")).toBeInTheDocument());

    // Select the frame as a reference image.
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.change(screen.getByTestId("cover-gen-prompt"), {
      target: { value: "cover art" },
    });
    fireEvent.click(screen.getByTestId("cover-generate"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "cover_generate",
        expect.objectContaining({
          options: expect.objectContaining({
            prompt: "cover art",
            reference_images: ["/x/out/cover/frame_00_1000.jpg"],
          }),
        }),
      ),
    );
  });

  it("disables generation until a prompt is supplied", async () => {
    invokeMock.mockResolvedValue([]);
    render(<CoverPanel dir="/x/out" />);
    expect(screen.getByTestId("cover-generate")).toBeDisabled();
  });

  it("shows backend errors", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "cover_ideas"
        ? Promise.reject("未配置 LLM API key")
        : Promise.resolve([]),
    );
    render(<CoverPanel dir="/x/out" />);
    fireEvent.click(screen.getByTestId("cover-ideas"));
    await waitFor(() =>
      expect(screen.getByTestId("cover-error")).toHaveTextContent(
        "未配置 LLM API key",
      ),
    );
  });
});
