import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * useState that hydrates from the app settings file and writes back on
 * change (debounced). Falls back to plain state in browser dev where the
 * Tauri backend is absent.
 */
export function usePersisted<T>(
  key: string,
  initial: T,
): [T, (v: T) => void] {
  const [value, setValue] = useState<T>(initial);
  const hydrated = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    Promise.resolve(invoke<Record<string, unknown>>("config_load"))
      .then((cfg) => {
        if (cfg && cfg[key] !== undefined && cfg[key] !== null) {
          setValue(cfg[key] as T);
        }
        hydrated.current = true;
      })
      .catch(() => {
        hydrated.current = true;
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  const set = (v: T) => {
    setValue(v);
    if (!hydrated.current) return;
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      Promise.resolve(invoke("config_set", { key, value: v })).catch(() => {});
    }, 400);
  };

  return [value, set];
}
