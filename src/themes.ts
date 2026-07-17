// Danmaku display themes: a theme is a set of CSS custom properties plus
// layout options. Built-in themes ship with the app; users can create,
// edit, import and export their own.

export interface DanmakuTheme {
  /** Unique id (built-ins prefixed "builtin:"). */
  id: string;
  name: string;
  /** CSS custom properties applied to the danmaku container. */
  vars: Record<string, string>;
  /** Show avatars / medals / timestamps. */
  showMedal: boolean;
  showTime: boolean;
  /** Entry animation: none | fade | slide. */
  animation: "none" | "fade" | "slide";
}

export const BUILTIN_THEMES: DanmakuTheme[] = [
  {
    id: "builtin:classic",
    name: "经典白",
    vars: {
      "--dm-bg": "#ffffff",
      "--dm-text": "#212121",
      "--dm-username": "#616161",
      "--dm-medal-bg": "#5c9ce6",
      "--dm-sc-bg": "#ffb300",
      "--dm-gift": "#e91e63",
      "--dm-guard": "#7b1fa2",
      "--dm-font-size": "14px",
      "--dm-line-gap": "6px",
      "--dm-radius": "4px",
      "--dm-font-family": "system-ui, 'PingFang SC', sans-serif",
    },
    showMedal: true,
    showTime: false,
    animation: "fade",
  },
  {
    id: "builtin:dark",
    name: "夜间黑",
    vars: {
      "--dm-bg": "#17181c",
      "--dm-text": "#e8e8e8",
      "--dm-username": "#9e9e9e",
      "--dm-medal-bg": "#3d6ea5",
      "--dm-sc-bg": "#c98a00",
      "--dm-gift": "#f06292",
      "--dm-guard": "#ba68c8",
      "--dm-font-size": "14px",
      "--dm-line-gap": "6px",
      "--dm-radius": "4px",
      "--dm-font-family": "system-ui, 'PingFang SC', sans-serif",
    },
    showMedal: true,
    showTime: true,
    animation: "fade",
  },
  {
    id: "builtin:obs-overlay",
    name: "OBS透明叠加",
    vars: {
      "--dm-bg": "transparent",
      "--dm-text": "#ffffff",
      "--dm-username": "#b3e5fc",
      "--dm-medal-bg": "#0288d1",
      "--dm-sc-bg": "#ff8f00",
      "--dm-gift": "#ff4081",
      "--dm-guard": "#ce93d8",
      "--dm-font-size": "18px",
      "--dm-line-gap": "8px",
      "--dm-radius": "6px",
      "--dm-font-family": "system-ui, 'PingFang SC', sans-serif",
    },
    showMedal: false,
    showTime: false,
    animation: "slide",
  },
];

const STORAGE_KEY = "vtb.customThemes";

export function loadCustomThemes(storage: Storage = localStorage): DanmakuTheme[] {
  try {
    const raw = storage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter(isValidTheme) : [];
  } catch {
    return [];
  }
}

export function saveCustomThemes(
  themes: DanmakuTheme[],
  storage: Storage = localStorage,
): void {
  storage.setItem(STORAGE_KEY, JSON.stringify(themes));
}

export function isValidTheme(t: unknown): t is DanmakuTheme {
  if (typeof t !== "object" || t === null) return false;
  const o = t as Record<string, unknown>;
  return (
    typeof o.id === "string" &&
    typeof o.name === "string" &&
    typeof o.vars === "object" &&
    o.vars !== null &&
    Object.entries(o.vars as object).every(
      ([k, v]) => k.startsWith("--") && typeof v === "string",
    ) &&
    typeof o.showMedal === "boolean" &&
    typeof o.showTime === "boolean" &&
    ["none", "fade", "slide"].includes(o.animation as string)
  );
}

/** Export a theme as pretty JSON. */
export function exportTheme(theme: DanmakuTheme): string {
  return JSON.stringify(theme, null, 2);
}

/** Parse an imported theme; throws on invalid input. */
export function importTheme(json: string): DanmakuTheme {
  const parsed = JSON.parse(json);
  if (!isValidTheme(parsed)) {
    throw new Error("无效的主题文件");
  }
  // Imported themes never collide with builtins.
  if (parsed.id.startsWith("builtin:")) {
    parsed.id = `custom:${parsed.id.slice("builtin:".length)}-${Date.now()}`;
  }
  return parsed;
}

/** Create a new custom theme based on an existing one. */
export function deriveTheme(base: DanmakuTheme, name: string): DanmakuTheme {
  return {
    ...base,
    vars: { ...base.vars },
    id: `custom:${Date.now()}`,
    name,
  };
}

export function allThemes(storage: Storage = localStorage): DanmakuTheme[] {
  return [...BUILTIN_THEMES, ...loadCustomThemes(storage)];
}
