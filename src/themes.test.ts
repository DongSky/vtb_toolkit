import { describe, expect, it, beforeEach } from "vitest";
import {
  BUILTIN_THEMES,
  allThemes,
  deriveTheme,
  exportTheme,
  importTheme,
  isValidTheme,
  loadCustomThemes,
  saveCustomThemes,
} from "./themes";

beforeEach(() => localStorage.clear());

describe("themes", () => {
  it("ships three builtin themes with required vars", () => {
    expect(BUILTIN_THEMES.length).toBe(3);
    for (const t of BUILTIN_THEMES) {
      expect(t.id.startsWith("builtin:")).toBe(true);
      expect(t.vars["--dm-bg"]).toBeDefined();
      expect(t.vars["--dm-text"]).toBeDefined();
      expect(isValidTheme(t)).toBe(true);
    }
  });

  it("persists and reloads custom themes", () => {
    const custom = deriveTheme(BUILTIN_THEMES[0], "我的主题");
    saveCustomThemes([custom]);
    const loaded = loadCustomThemes();
    expect(loaded).toEqual([custom]);
    expect(allThemes().length).toBe(4);
  });

  it("ignores corrupted storage", () => {
    localStorage.setItem("vtb.customThemes", "{not json");
    expect(loadCustomThemes()).toEqual([]);
    localStorage.setItem("vtb.customThemes", JSON.stringify([{ bogus: 1 }]));
    expect(loadCustomThemes()).toEqual([]);
  });

  it("export/import roundtrip", () => {
    const custom = deriveTheme(BUILTIN_THEMES[1], "roundtrip");
    const json = exportTheme(custom);
    const imported = importTheme(json);
    expect(imported).toEqual(custom);
  });

  it("import rejects invalid json and invalid theme", () => {
    expect(() => importTheme("nope")).toThrow();
    expect(() => importTheme('{"id":1}')).toThrow(/无效/);
  });

  it("imported builtin ids are namespaced to custom", () => {
    const stolen = { ...BUILTIN_THEMES[0], vars: { ...BUILTIN_THEMES[0].vars } };
    const imported = importTheme(JSON.stringify(stolen));
    expect(imported.id.startsWith("custom:")).toBe(true);
  });

  it("derive creates an independent copy", () => {
    const d = deriveTheme(BUILTIN_THEMES[0], "copy");
    d.vars["--dm-bg"] = "#000000";
    expect(BUILTIN_THEMES[0].vars["--dm-bg"]).not.toBe("#000000");
    expect(d.id.startsWith("custom:")).toBe(true);
    expect(d.name).toBe("copy");
  });

  it("validates var keys must be css custom properties", () => {
    const bad = {
      ...deriveTheme(BUILTIN_THEMES[0], "bad"),
      vars: { color: "red" },
    };
    expect(isValidTheme(bad)).toBe(false);
  });
});
