import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import ThemeEditor from "./ThemeEditor";
import { BUILTIN_THEMES, deriveTheme, exportTheme } from "../themes";

describe("ThemeEditor", () => {
  it("editing a builtin theme creates a copy", () => {
    const onSave = vi.fn();
    render(<ThemeEditor theme={BUILTIN_THEMES[0]} onSave={onSave} />);
    fireEvent.click(screen.getByTestId("theme-save"));
    expect(onSave).toHaveBeenCalledOnce();
    const saved = onSave.mock.calls[0][0];
    expect(saved.id.startsWith("custom:")).toBe(true);
    expect(saved.name).toContain("副本");
  });

  it("edits a css variable and saves it", () => {
    const custom = deriveTheme(BUILTIN_THEMES[0], "编辑测试");
    const onSave = vi.fn();
    render(<ThemeEditor theme={custom} onSave={onSave} />);
    fireEvent.change(screen.getByTestId("var---dm-font-size"), {
      target: { value: "20px" },
    });
    fireEvent.click(screen.getByTestId("theme-save"));
    expect(onSave.mock.calls[0][0].vars["--dm-font-size"]).toBe("20px");
    // Same id preserved for custom themes (edit-in-place).
    expect(onSave.mock.calls[0][0].id).toBe(custom.id);
  });

  it("toggles medal/time display flags", () => {
    const custom = deriveTheme(BUILTIN_THEMES[0], "flags");
    const onSave = vi.fn();
    render(<ThemeEditor theme={custom} onSave={onSave} />);
    fireEvent.click(screen.getByTestId("theme-show-medal"));
    fireEvent.click(screen.getByTestId("theme-show-time"));
    fireEvent.click(screen.getByTestId("theme-save"));
    const saved = onSave.mock.calls[0][0];
    expect(saved.showMedal).toBe(!custom.showMedal);
    expect(saved.showTime).toBe(!custom.showTime);
  });

  it("imports a valid theme json", () => {
    const donor = deriveTheme(BUILTIN_THEMES[1], "导入源");
    const onSave = vi.fn();
    render(<ThemeEditor theme={BUILTIN_THEMES[0]} onSave={onSave} />);
    fireEvent.change(screen.getByTestId("theme-import-input"), {
      target: { value: exportTheme(donor) },
    });
    fireEvent.click(screen.getByTestId("theme-import"));
    fireEvent.click(screen.getByTestId("theme-save"));
    expect(onSave.mock.calls[0][0].name).toBe("导入源");
  });

  it("shows an error for invalid import", () => {
    render(<ThemeEditor theme={BUILTIN_THEMES[0]} onSave={vi.fn()} />);
    fireEvent.change(screen.getByTestId("theme-import-input"), {
      target: { value: "junk{" },
    });
    fireEvent.click(screen.getByTestId("theme-import"));
    expect(screen.getByTestId("theme-import-error")).toBeInTheDocument();
  });
});
