import { invoke } from "@tauri-apps/api/core";

// Typed wrappers around Tauri commands. Every backend command gets a wrapper
// here — components never call invoke() directly, so the IPC surface stays
// greppable and typed in one place.

export interface AppInfo {
  version: string;
  db_path: string;
  schema_version: number;
}

export function appInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("app_info");
}
