// Domain types mirrored from the Rust core (src-tauri/src/http_engine, history).

export interface KeyValue {
  key: string;
  value: string;
  enabled: boolean;
}

export interface FormField extends KeyValue {
  is_file: boolean;
}

export type BodySpec =
  | { kind: "none" }
  | { kind: "raw"; content_type: string; text: string }
  | { kind: "url_encoded"; fields: KeyValue[] }
  | { kind: "form_data"; fields: FormField[] }
  | { kind: "binary"; path: string }
  | { kind: "graphql"; query: string; variables: string };

export interface WsEvent {
  conn_id: string;
  kind: "open" | "in" | "out" | "closed" | "error";
  text: string;
}

export interface WsMessage {
  kind: "open" | "in" | "out" | "closed" | "error";
  text: string;
  ts: number;
}

export interface SendSettings {
  timeout_ms: number;
  follow_redirects: boolean;
  max_redirects: number;
  verify_ssl: boolean;
}

export interface RequestSpec {
  method: string;
  url: string;
  headers: KeyValue[];
  body: BodySpec;
  settings: SendSettings;
  auth?: AuthSpec;
}

export interface TestResult {
  name: string;
  passed: boolean;
  error: string | null;
}

export interface ConsoleLine {
  level: string;
  message: string;
}

export interface Timings {
  dns_ms: number | null;
  connect_ms: number | null;
  tls_ms: number | null;
  server_ms: number;
  download_ms: number;
  total_ms: number;
  redirects: number;
}

/** Stage of the request where a failure happened (drives the error pipeline). */
export type ErrorPhase =
  "dns" | "tcp" | "tls" | "send" | "receive" | "timeout" | "request" | "other";

export interface SendErrorInfo {
  phase: ErrorPhase;
  message: string;
  hint: string | null;
}

export interface SendResult {
  history_id: number;
  status: number;
  status_text: string;
  http_version: string;
  headers: [string, string][];
  body_text: string | null;
  body_base64: string | null;
  body_truncated: boolean;
  size: number;
  duration_ms: number;
  ttfb_ms: number;
  timings: Timings;
  tests: TestResult[];
  console: ConsoleLine[];
  script_error: string | null;
}

export interface RunOptions {
  collection_id: number;
  folder_id?: number | null;
  iterations: number;
  delay_ms: number;
  data?: unknown[] | null;
}

export interface RequestRunResult {
  iteration: number;
  item_id: number;
  name: string;
  url: string;
  method: string;
  status: number | null;
  error: string | null;
  duration_ms: number;
  tests: TestResult[];
  console: ConsoleLine[];
  skipped: boolean;
}

export interface RunReport {
  total_requests: number;
  passed_tests: number;
  failed_tests: number;
  errors: number;
  cancelled: boolean;
  duration_ms: number;
  results: RequestRunResult[];
}

export interface HistorySummary {
  id: number;
  sent_at: string;
  method: string;
  url: string;
  host: string;
  status: number | null;
  error: string | null;
  duration_ms: number | null;
  resp_size: number | null;
  pinned: boolean;
  label: string | null;
  /** Match context with `[[`..`]]` around hits; only for text search. */
  snippet: string | null;
}

export interface SearchFilters {
  query?: string;
  method?: string;
  host?: string;
  status_exact?: number;
  status_class?: number;
  errors_only?: boolean;
  pinned_only?: boolean;
  date_from?: string;
  date_to?: string;
  endpoint?: { method: string; url_base: string };
}

export interface EndpointGroup {
  method: string;
  url_base: string;
  count: number;
  last_sent_at: string;
  last_status: number | null;
  last_error: string | null;
}

export interface RetentionSettings {
  max_age_days: number;
  max_entries: number;
}

export interface Collection {
  id: number;
  name: string;
  description: string;
  sort_order: number;
}

export interface CollectionItem {
  id: number;
  collection_id: number;
  parent_id: number | null;
  kind: "folder" | "request";
  name: string;
  description: string;
  sort_order: number;
  req_spec: RequestSpec | null;
  pre_request_script: string | null;
  test_script: string | null;
}

export interface Environment {
  id: number;
  name: string;
  is_active: boolean;
}

export type VarScope = "global" | "environment" | "collection";

export interface Variable {
  key: string;
  initial_value: string;
  current_value: string | null;
  is_secret: boolean;
  enabled: boolean;
}

export interface OAuth2Config {
  grant_type: string;
  token_url: string;
  auth_url: string;
  client_id: string;
  client_secret: string;
  scope: string;
  username: string;
  password: string;
  credentials_in_body: boolean;
  access_token: string;
  refresh_token: string;
  expires_at: number;
}

export const EMPTY_OAUTH2: OAuth2Config = {
  grant_type: "client_credentials",
  token_url: "",
  auth_url: "",
  client_id: "",
  client_secret: "",
  scope: "",
  username: "",
  password: "",
  credentials_in_body: false,
  access_token: "",
  refresh_token: "",
  expires_at: 0,
};

export type AuthSpec =
  | { kind: "none" }
  | { kind: "inherit" }
  | { kind: "api_key"; key: string; value: string; in_query: boolean }
  | { kind: "bearer"; token: string }
  | { kind: "basic"; username: string; password: string }
  | ({ kind: "oauth2" } & OAuth2Config)
  | {
      kind: "aws_sig_v4";
      access_key: string;
      secret_key: string;
      region: string;
      service: string;
      session_token: string;
    };

export interface TokenResult {
  access_token: string;
  refresh_token: string;
  expires_at: number;
  token_type: string;
  raw: unknown;
}

export interface CookieInfo {
  domain: string;
  path: string;
  name: string;
  value: string;
  secure: boolean;
  expires: string | null;
}

export interface AppSettings {
  proxy_mode: string;
  proxy_url: string;
  ca_cert_paths: string[];
  client_cert_path: string;
  client_cert_password: string;
  max_captured_body_kb: number;
  default_timeout_ms: number;
  default_verify_ssl: boolean;
  default_follow_redirects: boolean;
  default_max_redirects: number;
  theme: "system" | "light" | "dark";
  response_layout: "bottom" | "right";
  editor_font_size: number;
  wrap_response: boolean;
}

export interface HostAlias {
  id: number;
  host: string;
  alias: string;
  color: string;
}

export interface ImportResult {
  collection_id: number;
  name: string;
  requests: number;
  folders: number;
  environments: number;
  variables: number;
}

export interface HistoryDetail extends HistorySummary {
  req_spec: RequestSpec;
  req_headers: [string, string][];
  req_body_text: string | null;
  status_text: string | null;
  http_version: string | null;
  resp_headers: [string, string][] | null;
  resp_body_text: string | null;
  resp_body_base64: string | null;
  resp_body_truncated: boolean;
  ttfb_ms: number | null;
  timings: Timings;
  error_phase: ErrorPhase | null;
  error_hint: string | null;
}

export const HTTP_METHODS = [
  "GET",
  "POST",
  "PUT",
  "PATCH",
  "DELETE",
  "HEAD",
  "OPTIONS",
] as const;

export const DEFAULT_SETTINGS: SendSettings = {
  timeout_ms: 30_000,
  follow_redirects: true,
  max_redirects: 10,
  verify_ssl: true,
};
