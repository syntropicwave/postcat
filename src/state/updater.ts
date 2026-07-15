import { create } from "zustand";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export const AUTO_UPDATE_KEY = "postcat.autoUpdateCheck";

/** Whether to check for updates on launch (localStorage, default on). */
export function autoUpdateEnabled(): boolean {
  try {
    const raw = localStorage.getItem(AUTO_UPDATE_KEY);
    return raw == null ? true : (JSON.parse(raw) as boolean);
  } catch {
    return true;
  }
}

type Status =
  | "idle"
  | "checking"
  | "available"
  | "uptodate"
  | "downloading"
  | "installing"
  | "error";

interface UpdaterState {
  status: Status;
  update: Update | null;
  version: string | null;
  notes: string | null;
  progress: number; // 0..1 (0 = unknown/indeterminate)
  error: string | null;
  dismissed: boolean;
  /** Version the user dismissed — don't re-nag for the same one. */
  dismissedVersion: string | null;
  /** Epoch ms of the last check attempt (for throttling on focus). */
  lastCheckedAt: number;
  /** Check for an update. `manual` surfaces "up to date"/errors in the UI. */
  runCheck: (manual?: boolean) => Promise<void>;
  install: () => Promise<void>;
  dismiss: () => void;
}

export const useUpdater = create<UpdaterState>((set, get) => ({
  status: "idle",
  update: null,
  version: null,
  notes: null,
  progress: 0,
  error: null,
  dismissed: false,
  dismissedVersion: null,
  lastCheckedAt: 0,

  runCheck: async (manual = false) => {
    if (get().status === "checking" || get().status === "downloading") return;
    set({ status: "checking", error: null, lastCheckedAt: Date.now() });
    try {
      const upd = await check();
      if (upd) {
        set({
          status: "available",
          update: upd,
          version: upd.version,
          notes: upd.body ?? null,
          // Keep it dismissed only if it's the same version we already dismissed.
          dismissed: get().dismissedVersion === upd.version,
        });
      } else {
        // No update. Only surface it when the user asked explicitly.
        set({ status: manual ? "uptodate" : "idle", update: null });
      }
    } catch (e) {
      // Auto-checks fail silently (offline, no release yet, …).
      set({ status: manual ? "error" : "idle", error: String(e) });
    }
  },

  install: async () => {
    const upd = get().update;
    if (!upd) return;
    set({ status: "downloading", progress: 0, error: null });
    try {
      let total = 0;
      let got = 0;
      await upd.downloadAndInstall((ev) => {
        if (ev.event === "Started") {
          total = ev.data.contentLength ?? 0;
        } else if (ev.event === "Progress") {
          got += ev.data.chunkLength;
          set({ progress: total > 0 ? got / total : 0 });
        } else if (ev.event === "Finished") {
          set({ status: "installing", progress: 1 });
        }
      });
      // Installed — relaunch into the new version.
      await relaunch();
    } catch (e) {
      set({ status: "error", error: String(e) });
    }
  },

  dismiss: () => set({ dismissed: true, dismissedVersion: get().version }),
}));
