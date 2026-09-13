import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import ui from "./ui.json";
import workflows from "./workflows.json";
import native from "./native.json";
import ai from "./ai.json";

export const LOCALES = ["zh-CN", "en", "ja"] as const;
export type Locale = typeof LOCALES[number];
export const messages: Record<string, readonly string[]> = { ...ui, ...workflows, ...native, ...ai };
const storageKey = "vtb.locale";
const subscribers = new Set<() => void>();
const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
export function normalizeLocale(value: unknown): Locale | undefined {
  if (typeof value !== "string") return;
  if (/^zh(?:-|$)/i.test(value)) return "zh-CN";
  if (/^ja(?:-|$)/i.test(value)) return "ja";
  if (/^en(?:-|$)/i.test(value)) return "en";
}
function initialLocale(): Locale {
  try {
    const saved = normalizeLocale(localStorage.getItem(storageKey));
    if (saved) return saved;
  } catch { /* Storage may be unavailable in restricted browser contexts. */ }
  return normalizeLocale(globalThis.navigator?.language) ?? "zh-CN";
}
let locale = initialLocale();
let revision = 0;
let initialized: Promise<void> | undefined;
let persistence = Promise.resolve();
export function getLocale() { return locale; }
const subscribe = (callback: () => void) => { subscribers.add(callback); return () => { subscribers.delete(callback); }; };
export function useLocale(): Locale { return useSyncExternalStore(subscribe, getLocale, getLocale); }
function applyLocale(next: Locale) {
  locale = next;
  if (typeof document !== "undefined") {
    document.documentElement.lang = next;
    document.title = "VTB Toolkit";
  }
  try { localStorage.setItem(storageKey, next); } catch { /* Native settings remain the durable fallback. */ }
  subscribers.forEach((fn) => fn());
}
/** A user choice takes precedence over a slow startup settings response. */
export async function initializeLocale() {
  initialized ??= (async () => {
    const atStart = revision;
    applyLocale(locale);
    if (!isTauri()) return;
    try {
      const settings = await invoke<Record<string, unknown>>("config_load");
      const saved = normalizeLocale(settings?.["ui.locale"]);
      if (saved && revision === atStart) applyLocale(saved);
    } catch { /* Browser/first launch still uses the local preference. */ }
  })();
  return initialized;
}
export function setLocale(next: Locale): Promise<void> {
  if (!LOCALES.includes(next)) return Promise.reject(new Error("Unsupported locale"));
  revision++;
  applyLocale(next);
  // Serialize writes so rapidly switching languages cannot persist an older choice.
  persistence = persistence.catch(() => {}).then(async () => {
    if (isTauri()) await invoke("config_set", { key: "ui.locale", value: next });
  });
  return persistence;
}
export function t(source: string, ...values: unknown[]): string {
  const translated = locale === "zh-CN" ? source : messages[source]?.[locale === "en" ? 0 : 1] ?? source;
  return sourceText(translated, ...values);
}
/** Store a system message in its source language so switching remains live. */
export function sourceText(source: string, ...values: unknown[]): string {
  return source.replace(/\{(\d+)\}/g, (token, index) => Number(index) < values.length ? String(values[Number(index)] ?? "") : token);
}

const escapeRegex = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const templates = Object.keys(messages).filter((key) => /\{\d+\}/.test(key)).map((key) => {
  const indices: number[] = [];
  const expression = key.split(/(\{\d+\})/).map((part) => {
    if (/^\{\d+\}$/.test(part)) { indices.push(Number(part.slice(1, -1))); return "([\\s\\S]*?)"; }
    return escapeRegex(part);
  }).join("");
  return { key, indices, regex: new RegExp(`^${expression}$`) };
});
const chineseSystemLabels: Record<string, string> = {
  ExtractAudio: "提取音频", Transcribe: "语音识别", Translate: "翻译",
  ExportSubtitles: "导出字幕", DetectHighlights: "检测高能片段",
  AnalyzeSemantic: "分析转录语义高光", AnalyzeVisual: "分析画面",
  RefineHighlights: "复核高能片段", CutClips: "导出切片", Done: "完成",
  "built without whisper support": "此版本未包含 Whisper 支持",
  "no playable stream url returned by the platform": "平台未返回可播放的直播地址，直播可能尚未开始或已经结束",
};
/** Translate only system messages at presentation boundaries. Never pass chat,
 * titles, notes, prompts or other user-authored content through this function. */
export function tm(value: unknown): string {
  const source = String(value ?? "");
  if (locale === "zh-CN") return chineseSystemLabels[source] ?? source;
  if (Object.prototype.hasOwnProperty.call(messages, source)) return t(source);
  for (const entry of templates) {
    const match = entry.regex.exec(source);
    if (!match) continue;
    const args: string[] = [];
    entry.indices.forEach((index, i) => { args[index] = match[i + 1]; });
    return t(entry.key, ...args);
  }
  if (source.startsWith("Error: ")) return `Error: ${tm(source.slice(7))}`;
  // Unknown third-party errors retain the original diagnostic text.
  return source;
}
export function number(value: number, options?: Intl.NumberFormatOptions) {
  return new Intl.NumberFormat(locale, options).format(value);
}
