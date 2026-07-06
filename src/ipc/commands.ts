import { invoke } from "@tauri-apps/api/core";
import type {
  EndpointGroup,
  HistoryDetail,
  HistorySummary,
  RequestSpec,
  RetentionSettings,
  SearchFilters,
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

export function historySearch(
  filters: SearchFilters,
  options?: { limit?: number; offset?: number },
): Promise<HistorySummary[]> {
  return invoke<HistorySummary[]>("history_search", { filters, ...options });
}

export function historyEndpoints(limit?: number): Promise<EndpointGroup[]> {
  return invoke<EndpointGroup[]>("history_endpoints", { limit });
}

export function historySetPinned(id: number, pinned: boolean): Promise<void> {
  return invoke("history_set_pinned", { id, pinned });
}

export function historySetLabel(
  id: number,
  label: string | null,
): Promise<void> {
  return invoke("history_set_label", { id, label });
}

export function retentionGet(): Promise<RetentionSettings> {
  return invoke<RetentionSettings>("retention_get");
}

export function retentionSet(settings: RetentionSettings): Promise<void> {
  return invoke("retention_set", { settings });
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
