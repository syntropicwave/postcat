import { useEffect, useState } from "react";

/** useState whose value is mirrored to localStorage under `key`. */
export function usePersistentState<T>(key: string, initial: T) {
  const [value, setValue] = useState<T>(() => {
    try {
      const raw = localStorage.getItem(key);
      return raw != null ? (JSON.parse(raw) as T) : initial;
    } catch {
      return initial;
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem(key, JSON.stringify(value));
    } catch {
      /* storage full / unavailable — non-fatal */
    }
  }, [key, value]);

  return [value, setValue] as const;
}
