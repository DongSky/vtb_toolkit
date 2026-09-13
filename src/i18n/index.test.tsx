import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useEffect, useState } from "react";
import { getLocale, messages, normalizeLocale, setLocale, sourceText, t, tm, useLocale } from ".";
import LanguageSwitcher from "../components/LanguageSwitcher";
import HotwordsPanel from "../components/HotwordsPanel";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => []) }));

describe("three-language catalog", () => {
  it("has complete translations and matching placeholders", () => {
    const placeholders = (text: string) => [...text.matchAll(/\{\d+\}/g)].map((m) => m[0]).sort();
    for (const [source, translations] of Object.entries(messages)) {
      expect(translations, source).toHaveLength(2);
      for (const translation of translations) {
        expect(translation.trim(), source).not.toBe("");
        expect(placeholders(translation), source).toEqual(placeholders(source));
      }
    }
  });
  it("normalizes supported locales and preserves diagnostic arguments", async () => {
    expect(normalizeLocale("en-US")).toBe("en");
    expect(normalizeLocale("ja-JP")).toBe("ja");
    expect(normalizeLocale("zh-Hant")).toBe("zh-CN");
    expect(normalizeLocale("fr")).toBeUndefined();
    await setLocale("en");
    expect(tm("文件不存在: D:\\录播\\test.mp4")).toBe("File not found: D:\\录播\\test.mp4");
    expect(tm("unknown error: 0x42")).toBe("unknown error: 0x42");
    const stored = sourceText("代理副本已生成: {0}", "字幕.mp4");
    expect(tm(stored)).toContain("字幕.mp4");
    await setLocale("ja");
    expect(tm(stored)).toBe(t("代理副本已生成: {0}", "字幕.mp4"));
  });
  it("switches without remounting live components or translating authored data", async () => {
    const mounted = vi.fn();
    function LiveView() {
      useLocale();
      const [note, setNote] = useState("连接");
      useEffect(mounted, []);
      return <><LanguageSwitcher /><h2>{t("自动录制")}</h2><input aria-label="note" value={note} onChange={(e) => setNote(e.target.value)} /><p>{t("文件不存在: {0}", "我的录播.mp4")}</p></>;
    }
    render(<LiveView />);
    fireEvent.change(screen.getByLabelText("note"), { target: { value: "我的备注: 连接" } });
    await act(async () => { fireEvent.change(screen.getByTestId("language-select"), { target: { value: "en" } }); });
    expect(screen.getByRole("heading")).toHaveTextContent("Automatic recording");
    expect(screen.getByLabelText("note")).toHaveValue("我的备注: 连接");
    expect(document.documentElement.lang).toBe("en");
    expect(localStorage.getItem("vtb.locale")).toBe("en");
    await act(async () => { await setLocale("ja"); });
    expect(screen.getByRole("heading")).toHaveTextContent("自動録画");
    expect(mounted).toHaveBeenCalledTimes(1);
    expect(getLocale()).toBe("ja");
  });
  it("preserves glossary names and descriptions that match UI keys", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([{ name: "连接", description: "停止", entries: 2, updated_at: "" }]);
    await setLocale("en");
    render(<HotwordsPanel />);
    expect(await screen.findByText("连接", { selector: "b" })).toBeVisible();
    expect(screen.getByTestId("hw-list")).toHaveTextContent("停止");
  });
  it("keeps a user selection when native hydration completes late and serializes saves", async () => {
    vi.resetModules();
    Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
    let finishLoad!: (settings: object) => void;
    let finishSave!: () => void;
    const saved: string[] = [];
    vi.mocked(invoke).mockImplementation((command, args) => {
      if (command === "config_load") return new Promise((resolve) => { finishLoad = resolve as typeof finishLoad; });
      saved.push((args as { value: string }).value);
      if (saved.length === 1) return new Promise<void>((resolve) => { finishSave = resolve; }) as ReturnType<typeof invoke>;
      return Promise.resolve();
    });
    try {
      const fresh = await import(".");
      const hydration = fresh.initializeLocale();
      const first = fresh.setLocale("en");
      const second = fresh.setLocale("ja");
      await Promise.resolve(); await Promise.resolve();
      expect(saved).toEqual(["en"]);
      finishLoad({ "ui.locale": "zh-CN" });
      await hydration;
      expect(fresh.getLocale()).toBe("ja");
      finishSave(); await Promise.all([first, second]);
      expect(saved).toEqual(["en", "ja"]);
      vi.resetModules();
      const restarted = await import(".");
      expect(restarted.getLocale()).toBe("ja");
    } finally {
      Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
      vi.mocked(invoke).mockResolvedValue([]);
    }
  });
});
