import { create } from "zustand";
import { appSettingsGet, appSettingsSet } from "../ipc/commands";
import type { AppSettings, SendSettings } from "../types";

interface State {
  settings: AppSettings | null;
  load: () => Promise<void>;
  /** Merge a patch, persist to the backend, and re-apply appearance. */
  update: (patch: Partial<AppSettings>) => Promise<void>;
}

export const useAppSettings = create<State>((set, get) => ({
  settings: null,
  load: async () => {
    const s = await appSettingsGet();
    set({ settings: s });
    applyAppearance(s);
  },
  update: async (patch) => {
    const cur = get().settings;
    if (!cur) return;
    const next = { ...cur, ...patch };
    set({ settings: next });
    applyAppearance(next);
    await appSettingsSet(next);
  },
}));

/** SendSettings for a new request, seeded from the global defaults. */
export function requestDefaults(base: SendSettings): SendSettings {
  const s = useAppSettings.getState().settings;
  if (!s) return { ...base };
  return {
    ...base,
    timeout_ms: s.default_timeout_ms,
    follow_redirects: s.default_follow_redirects,
    max_redirects: s.default_max_redirects,
    verify_ssl: s.default_verify_ssl,
  };
}

/** Push theme + font size to the document so CSS/CodeMirror pick them up. */
export function applyAppearance(s: AppSettings) {
  const root = document.documentElement;
  if (s.theme === "system") {
    delete root.dataset.theme;
    root.style.colorScheme = "light dark";
  } else {
    root.dataset.theme = s.theme;
    root.style.colorScheme = s.theme;
  }
  root.style.setProperty("--editor-font-size", `${s.editor_font_size}px`);
}
