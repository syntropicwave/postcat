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
  | { kind: "binary"; path: string };

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
