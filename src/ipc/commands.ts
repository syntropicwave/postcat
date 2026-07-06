import { invoke } from "@tauri-apps/api/core";
import type {
  HistoryDetail,
  HistorySummary,
  RequestSpec,
  SendResult,
} from "../types";

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

export function sendRequest(
  requestId: string,
  spec: RequestSpec,
): Promise<SendResult> {
  return invoke<SendResult>("send_request", { requestId, spec });
}

export function cancelRequest(requestId: string): Promise<void> {
  return invoke("cancel_request", { requestId });
}

export function historyList(options?: {
  limit?: number;
  offset?: number;
  query?: string;
}): Promise<HistorySummary[]> {
  return invoke<HistorySummary[]>("history_list", { ...options });
}

export function historyGet(id: number): Promise<HistoryDetail> {
  return invoke<HistoryDetail>("history_get", { id });
}

export function historyDelete(id: number): Promise<void> {
  return invoke("history_delete", { id });
}

export function historyClear(): Promise<void> {
  return invoke("history_clear");
}
