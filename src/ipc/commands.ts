import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AuthSpec,
  Collection,
  CollectionItem,
  CookieInfo,
  EndpointGroup,
  Environment,
  HistoryDetail,
  HistorySummary,
  ImportResult,
  KeyValue,
  OAuth2Config,
  RequestSpec,
  RetentionSettings,
  RunOptions,
  RunReport,
  SearchFilters,
  SendResult,
  TokenResult,
  Variable,
  VarScope,
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
  collectionId?: number | null,
  itemId?: number | null,
  preRequestScript?: string | null,
  testScript?: string | null,
): Promise<SendResult> {
  return invoke<SendResult>("send_request", {
    requestId,
    spec,
    collectionId,
    itemId,
    preRequestScript,
    testScript,
  });
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

/* ---------------- collections ---------------- */

export function collectionsList(): Promise<Collection[]> {
  return invoke<Collection[]>("collections_list");
}

export function collectionCreate(name: string): Promise<number> {
  return invoke<number>("collection_create", { name });
}

export function collectionUpdate(
  id: number,
  patch: { name?: string; description?: string },
): Promise<void> {
  return invoke("collection_update", { id, ...patch });
}

export function collectionDelete(id: number): Promise<void> {
  return invoke("collection_delete", { id });
}

export function collectionItems(
  collectionId: number,
): Promise<CollectionItem[]> {
  return invoke<CollectionItem[]>("collection_items", { collectionId });
}

export function itemCreate(args: {
  collectionId: number;
  parentId?: number | null;
  kind: "folder" | "request";
  name: string;
  spec?: RequestSpec;
}): Promise<number> {
  return invoke<number>("item_create", { ...args });
}

export function itemUpdate(
  id: number,
  patch: { name?: string; description?: string; spec?: RequestSpec },
): Promise<void> {
  return invoke("item_update", { id, ...patch });
}

export function itemMove(
  id: number,
  newParentId: number | null,
  beforeId?: number | null,
): Promise<void> {
  return invoke("item_move", { id, newParentId, beforeId });
}

export function itemDelete(id: number): Promise<void> {
  return invoke("item_delete", { id });
}

export function itemDuplicate(id: number): Promise<number> {
  return invoke<number>("item_duplicate", { id });
}

export function historySaveBody(id: number, path: string): Promise<void> {
  return invoke("history_save_body", { id, path });
}

/* ---------------- environments & variables ---------------- */

export function envList(): Promise<Environment[]> {
  return invoke<Environment[]>("env_list");
}

export function envCreate(name: string): Promise<number> {
  return invoke<number>("env_create", { name });
}

export function envRename(id: number, name: string): Promise<void> {
  return invoke("env_rename", { id, name });
}

export function envDelete(id: number): Promise<void> {
  return invoke("env_delete", { id });
}

export function envSetActive(id: number | null): Promise<void> {
  return invoke("env_set_active", { id });
}

export function envExportFile(id: number, path: string): Promise<void> {
  return invoke("env_export_file", { id, path });
}

export function envDuplicate(id: number): Promise<number> {
  return invoke<number>("env_duplicate", { id });
}

export function varsGet(
  scope: VarScope,
  ownerId?: number | null,
): Promise<Variable[]> {
  return invoke<Variable[]>("vars_get", { scope, ownerId });
}

export function varsSave(
  scope: VarScope,
  ownerId: number | null,
  vars: Variable[],
): Promise<void> {
  return invoke("vars_save", { scope, ownerId, vars });
}

export function varsEffective(
  collectionId?: number | null,
): Promise<Variable[]> {
  return invoke<Variable[]>("vars_effective", { collectionId });
}

/* ---------------- import / export ---------------- */

export function importText(text: string): Promise<ImportResult> {
  return invoke<ImportResult>("import_text", { text });
}

export function importFile(path: string): Promise<ImportResult> {
  return invoke<ImportResult>("import_file", { path });
}

export function exportCollectionFile(
  collectionId: number,
  path: string,
): Promise<void> {
  return invoke("export_collection_file", { collectionId, path });
}

export function parseCurlCommand(text: string): Promise<RequestSpec> {
  return invoke<RequestSpec>("parse_curl_command", { text });
}

/* ---------------- auth ---------------- */

export function authStoredGet(target: {
  collectionId?: number;
  itemId?: number;
}): Promise<AuthSpec> {
  return invoke<AuthSpec>("auth_stored_get", { ...target });
}

export function authStoredSet(
  target: { collectionId?: number; itemId?: number },
  auth: AuthSpec,
): Promise<void> {
  return invoke("auth_stored_set", { ...target, auth });
}

export function oauth2FetchToken(config: OAuth2Config): Promise<TokenResult> {
  return invoke<TokenResult>("oauth2_fetch_token", { config });
}

export function oauth2RefreshToken(config: OAuth2Config): Promise<TokenResult> {
  return invoke<TokenResult>("oauth2_refresh_token", { config });
}

export function oauth2Authorize(config: OAuth2Config): Promise<TokenResult> {
  return invoke<TokenResult>("oauth2_authorize", { config });
}

/* ---------------- cookies & settings ---------------- */

export function cookiesList(): Promise<CookieInfo[]> {
  return invoke<CookieInfo[]>("cookies_list");
}

export function cookieDelete(
  domain: string,
  path: string,
  name: string,
): Promise<void> {
  return invoke("cookie_delete", { domain, path, name });
}

export function cookiesClear(): Promise<void> {
  return invoke("cookies_clear");
}

/* ---------------- scripts & runner ---------------- */

export function itemScriptsGet(
  id: number,
): Promise<[string | null, string | null]> {
  return invoke("item_scripts_get", { id });
}

export function itemScriptsSet(
  id: number,
  preRequestScript: string | null,
  testScript: string | null,
): Promise<void> {
  return invoke("item_scripts_set", { id, preRequestScript, testScript });
}

export function collectionScriptsGet(
  id: number,
): Promise<[string | null, string | null]> {
  return invoke("collection_scripts_get", { id });
}

export function collectionScriptsSet(
  id: number,
  preRequestScript: string | null,
  testScript: string | null,
): Promise<void> {
  return invoke("collection_scripts_set", { id, preRequestScript, testScript });
}

export function runCollection(options: RunOptions): Promise<RunReport> {
  return invoke<RunReport>("run_collection", { options });
}

export function runnerCancel(collectionId: number): Promise<void> {
  return invoke("runner_cancel", { collectionId });
}

/* ---------------- GraphQL & WebSocket ---------------- */

export function graphqlIntrospect(
  url: string,
  headers: KeyValue[],
  collectionId?: number | null,
): Promise<unknown> {
  return invoke("graphql_introspect", { url, headers, collectionId });
}

export function wsConnect(
  connId: string,
  url: string,
  headers: KeyValue[],
  collectionId?: number | null,
): Promise<void> {
  return invoke("ws_connect", { connId, url, headers, collectionId });
}

export function wsSend(connId: string, text: string): Promise<void> {
  return invoke("ws_send", { connId, text });
}

export function wsClose(connId: string): Promise<void> {
  return invoke("ws_close", { connId });
}

export function appSettingsGet(): Promise<AppSettings> {
  return invoke<AppSettings>("app_settings_get");
}

export function appSettingsSet(settings: AppSettings): Promise<void> {
  return invoke("app_settings_set", { settingsValue: settings });
}

/* ---------------- sync ---------------- */

export interface SyncStatus {
  signed_in: boolean;
  url: string;
  email: string;
  last_cursor: number;
  pending: number;
}

export interface SyncReport {
  pushed: number;
  pulled: number;
  cursor: number;
}

/** Returns the one-time recovery code. */
export function syncRegister(
  url: string,
  email: string,
  password: string,
): Promise<string> {
  return invoke<string>("sync_register", { url, email, password });
}

export function syncLogin(
  url: string,
  email: string,
  password: string,
): Promise<void> {
  return invoke("sync_login", { url, email, password });
}

export function syncLogout(): Promise<void> {
  return invoke("sync_logout");
}

export function syncStatus(): Promise<SyncStatus> {
  return invoke<SyncStatus>("sync_status");
}

export function syncNow(): Promise<SyncReport> {
  return invoke<SyncReport>("sync_now");
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
