import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import HotwordsPanel from "./HotwordsPanel";

function summary(name: string, entries = 2) {
  return { name, description: "", entries, updated_at: "2026-07-18T00:00:00Z" };
}

beforeEach(() => invokeMock.mockReset());

describe("HotwordsPanel", () => {
  it("saves raw text as a table (AI-normalized) and lists it", async () => {
    let saved = false;
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "hotwords_list")
        return Promise.resolve(saved ? [summary("DOTA")] : []);
      if (cmd === "hotwords_save_raw") {
        saved = true;
        expect(args!.name).toBe("DOTA");
        expect(args!.useLlm).toBe(true);
        return Promise.resolve({
          used_llm: true,
          table: {
            name: "DOTA",
            description: "",
            entries: [
              { term: "GPK", aliases: ["gpk", "鸡皮开"], category: "选手" },
              { term: "帕克", translation: "Puck" },
            ],
            updated_at: "2026-07-18T00:00:00Z",
          },
        });
      }
      return Promise.resolve();
    });
    render(<HotwordsPanel />);
    fireEvent.change(screen.getByTestId("hw-name"), { target: { value: "DOTA" } });
    fireEvent.change(screen.getByTestId("hw-raw"), {
      target: { value: "我们打野是GPK,常被打成gpk/鸡皮开;帕克=Puck" },
    });
    fireEvent.click(screen.getByTestId("hw-save"));
    await waitFor(() =>
      expect(screen.getByTestId("hw-msg")).toHaveTextContent("2 个词条"),
    );
    expect(screen.getByTestId("hw-msg")).toHaveTextContent("AI 规整");
    // Detail shows normalized entries.
    expect(screen.getByTestId("hw-detail")).toHaveTextContent("鸡皮开");
    expect(screen.getByTestId("hw-detail")).toHaveTextContent("Puck");
    // Table list refreshed.
    await waitFor(() =>
      expect(screen.getByTestId("hw-table-DOTA")).toBeInTheDocument(),
    );
  });

  it("reports rule-parser fallback when AI unavailable", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "hotwords_list") return Promise.resolve([]);
      if (cmd === "hotwords_save_raw")
        return Promise.resolve({
          used_llm: false,
          table: {
            name: "T",
            description: "",
            entries: [{ term: "GPK" }],
            updated_at: "2026-07-18T00:00:00Z",
          },
        });
      return Promise.resolve();
    });
    render(<HotwordsPanel />);
    fireEvent.change(screen.getByTestId("hw-name"), { target: { value: "T" } });
    fireEvent.change(screen.getByTestId("hw-raw"), { target: { value: "GPK" } });
    fireEvent.click(screen.getByTestId("hw-save"));
    await waitFor(() =>
      expect(screen.getByTestId("hw-msg")).toHaveTextContent("AI 不可用"),
    );
  });

  it("merges selected tables and deletes a table", async () => {
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "hotwords_list")
        return Promise.resolve([summary("a"), summary("b")]);
      if (cmd === "hotwords_merge") {
        expect(args!.names).toEqual(["a", "b"]);
        expect(args!.newName).toBe("全部");
        return Promise.resolve({
          name: "全部",
          description: "a + b",
          entries: [{ term: "x" }, { term: "y" }, { term: "z" }],
          updated_at: "2026-07-18T00:00:00Z",
        });
      }
      if (cmd === "hotwords_delete") {
        expect(args!.name).toBe("a");
        return Promise.resolve();
      }
      return Promise.resolve();
    });
    render(<HotwordsPanel />);
    await waitFor(() =>
      expect(screen.getByTestId("hw-table-a")).toBeInTheDocument(),
    );
    // Select both for merge.
    const checkboxes = screen
      .getAllByRole("checkbox")
      .filter((c) => (c as HTMLInputElement).checked === false);
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);
    await waitFor(() =>
      expect(screen.getByTestId("hw-merge-bar")).toBeInTheDocument(),
    );
    fireEvent.change(screen.getByTestId("hw-merge-name"), {
      target: { value: "全部" },
    });
    fireEvent.click(screen.getByTestId("hw-merge"));
    await waitFor(() =>
      expect(screen.getByTestId("hw-msg")).toHaveTextContent("3 词条"),
    );

    fireEvent.click(screen.getByTestId("hw-del-a"));
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some((c) => c[0] === "hotwords_delete"),
      ).toBe(true),
    );
  });
});
