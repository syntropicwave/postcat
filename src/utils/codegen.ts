import type { KeyValue, RequestSpec } from "../types";
import { specToCurl } from "./curl";

export const CODEGEN_LANGUAGES = [
  "cURL",
  "JavaScript (fetch)",
  "Python (requests)",
  "Go (net/http)",
  "C# (HttpClient)",
  "PowerShell",
] as const;

export type CodegenLanguage = (typeof CODEGEN_LANGUAGES)[number];

/** Generate a code snippet for the request. `{{vars}}` are left visible. */
export function generateCode(lang: CodegenLanguage, spec: RequestSpec): string {
  switch (lang) {
    case "cURL":
      return specToCurl(spec);
    case "JavaScript (fetch)":
      return genFetch(spec);
    case "Python (requests)":
      return genPython(spec);
    case "Go (net/http)":
      return genGo(spec);
    case "C# (HttpClient)":
      return genCSharp(spec);
    case "PowerShell":
      return genPowerShell(spec);
  }
}

function enabledHeaders(spec: RequestSpec): KeyValue[] {
  return (spec.headers ?? []).filter((h) => h.enabled && h.key);
}

interface BodyInfo {
  text: string | null;
  contentType: string | null;
  note: string | null;
}

function bodyInfo(spec: RequestSpec): BodyInfo {
  const b = spec.body;
  switch (b.kind) {
    case "none":
      return { text: null, contentType: null, note: null };
    case "raw":
      return { text: b.text, contentType: b.content_type || null, note: null };
    case "url_encoded": {
      const text = b.fields
        .filter((f) => f.enabled && f.key)
        .map(
          (f) => `${encodeURIComponent(f.key)}=${encodeURIComponent(f.value)}`,
        )
        .join("&");
      return {
        text,
        contentType: "application/x-www-form-urlencoded",
        note: null,
      };
    }
    case "form_data":
      return {
        text: null,
        contentType: "multipart/form-data",
        note: "multipart form data — adapt file parts to your runtime",
      };
    case "binary":
      return {
        text: null,
        contentType: "application/octet-stream",
        note: `binary body from file: ${b.path}`,
      };
  }
}

function genFetch(spec: RequestSpec): string {
  const { text, contentType, note } = bodyInfo(spec);
  const headers = enabledHeaders(spec);
  const headerLines = [
    ...headers.map(
      (h) => `    ${JSON.stringify(h.key)}: ${JSON.stringify(h.value)},`,
    ),
    ...(contentType &&
    !headers.some((h) => h.key.toLowerCase() === "content-type")
      ? [`    "Content-Type": ${JSON.stringify(contentType)},`]
      : []),
  ];
  return [
    note ? `// NOTE: ${note}` : null,
    `const response = await fetch(${JSON.stringify(spec.url)}, {`,
    `  method: ${JSON.stringify(spec.method)},`,
    headerLines.length ? `  headers: {\n${headerLines.join("\n")}\n  },` : null,
    text != null ? `  body: ${JSON.stringify(text)},` : null,
    `});`,
    `const data = await response.text();`,
    `console.log(response.status, data);`,
  ]
    .filter((l): l is string => l !== null)
    .join("\n");
}

function genPython(spec: RequestSpec): string {
  const { text, contentType, note } = bodyInfo(spec);
  const headers = enabledHeaders(spec);
  const headerItems = [
    ...headers.map((h) => `    ${py(h.key)}: ${py(h.value)},`),
    ...(contentType &&
    !headers.some((h) => h.key.toLowerCase() === "content-type")
      ? [`    "Content-Type": ${py(contentType)},`]
      : []),
  ];
  return [
    "import requests",
    "",
    note ? `# NOTE: ${note}` : null,
    `url = ${py(spec.url)}`,
    headerItems.length
      ? `headers = {\n${headerItems.join("\n")}\n}`
      : "headers = {}",
    text != null ? `data = ${py(text)}` : "data = None",
    "",
    `response = requests.request(${py(spec.method)}, url, headers=headers, data=data)`,
    "print(response.status_code, response.text)",
  ]
    .filter((l): l is string => l !== null)
    .join("\n");
}

function py(s: string): string {
  return JSON.stringify(s);
}

function genGo(spec: RequestSpec): string {
  const { text, note } = bodyInfo(spec);
  const headers = enabledHeaders(spec);
  return [
    "package main",
    "",
    "import (",
    '\t"fmt"',
    '\t"io"',
    '\t"net/http"',
    text != null ? '\t"strings"' : null,
    ")",
    "",
    "func main() {",
    note ? `\t// NOTE: ${note}` : null,
    text != null
      ? `\tbody := strings.NewReader(${JSON.stringify(text)})`
      : "\tvar body io.Reader",
    `\treq, err := http.NewRequest(${JSON.stringify(spec.method)}, ${JSON.stringify(spec.url)}, body)`,
    "\tif err != nil {",
    "\t\tpanic(err)",
    "\t}",
    ...headers.map(
      (h) =>
        `\treq.Header.Set(${JSON.stringify(h.key)}, ${JSON.stringify(h.value)})`,
    ),
    "",
    "\tresp, err := http.DefaultClient.Do(req)",
    "\tif err != nil {",
    "\t\tpanic(err)",
    "\t}",
    "\tdefer resp.Body.Close()",
    "\tdata, _ := io.ReadAll(resp.Body)",
    "\tfmt.Println(resp.Status, string(data))",
    "}",
  ]
    .filter((l): l is string => l !== null)
    .join("\n");
}

function genCSharp(spec: RequestSpec): string {
  const { text, contentType, note } = bodyInfo(spec);
  const headers = enabledHeaders(spec);
  return [
    "using var client = new HttpClient();",
    note ? `// NOTE: ${note}` : null,
    `var request = new HttpRequestMessage(new HttpMethod(${cs(spec.method)}), ${cs(spec.url)});`,
    ...headers
      .filter((h) => h.key.toLowerCase() !== "content-type")
      .map(
        (h) =>
          `request.Headers.TryAddWithoutValidation(${cs(h.key)}, ${cs(h.value)});`,
      ),
    text != null
      ? `request.Content = new StringContent(${cs(text)}, System.Text.Encoding.UTF8, ${cs(contentType ?? "text/plain")});`
      : null,
    "",
    "var response = await client.SendAsync(request);",
    "var body = await response.Content.ReadAsStringAsync();",
    'Console.WriteLine($"{(int)response.StatusCode} {body}");',
  ]
    .filter((l): l is string => l !== null)
    .join("\n");
}

function cs(s: string): string {
  return JSON.stringify(s);
}

function genPowerShell(spec: RequestSpec): string {
  const { text, contentType, note } = bodyInfo(spec);
  const headers = enabledHeaders(spec);
  const headerLines = headers.map((h) => `    ${psq(h.key)} = ${psq(h.value)}`);
  return [
    note ? `# NOTE: ${note}` : null,
    headerLines.length
      ? `$headers = @{\n${headerLines.join("\n")}\n}`
      : "$headers = @{}",
    text != null ? `$body = ${psq(text)}` : null,
    [
      `Invoke-RestMethod -Method ${spec.method}`,
      `-Uri ${psq(spec.url)}`,
      "-Headers $headers",
      text != null ? "-Body $body" : null,
      contentType ? `-ContentType ${psq(contentType)}` : null,
    ]
      .filter(Boolean)
      .join(" `\n    "),
  ]
    .filter((l): l is string => l !== null)
    .join("\n");
}

function psq(s: string): string {
  return `'${s.replace(/'/g, "''")}'`;
}
