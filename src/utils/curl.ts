import type { RequestSpec } from "../types";

/** Render a request spec as a copy-pasteable curl command. */
export function specToCurl(spec: RequestSpec): string {
  const parts: string[] = ["curl"];
  if (spec.method !== "GET") parts.push(`-X ${spec.method}`);
  parts.push(sq(spec.url));

  for (const h of spec.headers ?? []) {
    if (h.enabled && h.key) parts.push(`-H ${sq(`${h.key}: ${h.value}`)}`);
  }

  const body = spec.body;
  switch (body.kind) {
    case "none":
      break;
    case "raw":
      if (body.content_type)
        parts.push(`-H ${sq(`Content-Type: ${body.content_type}`)}`);
      parts.push(`--data-raw ${sq(body.text)}`);
      break;
    case "url_encoded":
      for (const f of body.fields) {
        if (f.enabled && f.key)
          parts.push(`--data-urlencode ${sq(`${f.key}=${f.value}`)}`);
      }
      break;
    case "form_data":
      for (const f of body.fields) {
        if (!f.enabled || !f.key) continue;
        parts.push(`-F ${sq(`${f.key}=${f.is_file ? "@" : ""}${f.value}`)}`);
      }
      break;
    case "binary":
      parts.push(`--data-binary ${sq(`@${body.path}`)}`);
      break;
  }

  const s = spec.settings;
  if (s) {
    if (!s.verify_ssl) parts.push("-k");
    if (s.follow_redirects) parts.push("-L");
  }

  return parts.join(" \\\n  ");
}

/** Single-quote for POSIX shells; escapes embedded single quotes. */
function sq(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}
