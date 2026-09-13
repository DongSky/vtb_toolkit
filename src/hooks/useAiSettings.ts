import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type ServiceKind = "text" | "vision" | "image";
export interface ServiceConfig { provider: string; base_url: string; model: string; use_env: boolean; no_auth: boolean }
export interface AiConfig { text: ServiceConfig; vision: ServiceConfig; image: ServiceConfig; vision_uses_text: boolean }
export interface EffectiveService { provider: string; base_url: string; model: string; base_source: string; model_source: string; key_source: string; ready: boolean; using_text: boolean; has_saved_key: boolean }
export interface AiSnapshot { config: AiConfig; effective: Record<ServiceKind, EffectiveService> }

/** Only public settings enter React state; stored credentials never return to JS. */
export function useAiSettings() {
  const [snapshot, setSnapshot] = useState<AiSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const load = useCallback(async () => {
    setError(null); setBusy(true);
    try { setSnapshot(await invoke<AiSnapshot>("ai_settings_load")); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  }, []);
  useEffect(() => { void load(); }, [load]);
  const save = async (kind: ServiceKind, service: ServiceConfig, visionUsesText: boolean, apiKey: string) => {
    const value = await invoke<AiSnapshot>("ai_settings_save", { kind, service, visionUsesText, apiKey: apiKey.trim() || null });
    setSnapshot(value); return value;
  };
  const clear = async (kind: ServiceKind) => {
    const value = await invoke<AiSnapshot>("ai_key_clear", { kind });
    setSnapshot(value); return value;
  };
  return { snapshot, error, busy, load, save, clear };
}
