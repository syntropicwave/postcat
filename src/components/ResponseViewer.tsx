import { useMemo, useState } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { json } from "@codemirror/lang-json";
import { html } from "@codemirror/lang-html";
import { xml } from "@codemirror/lang-xml";
import type { SendResult } from "../types";
import { usePrefersDark } from "../hooks/usePrefersDark";

type View = "pretty" | "raw" | "preview" | "headers" | "tests";

interface Props {
  response: SendResult | null;
  error: string | null;
  sending: boolean;
  streamText?: string;
}

export function ResponseViewer({
  response,
  error,
  sending,
  streamText,
}: Props) {
  const [view, setView] = useState<View>("pretty");

  if (sending) {
    // Live SSE stream: show events as they arrive.
    if (streamText) {
      return (
        <div className="response-viewer">
          <div className="response-status">
            <span className="status-badge status-ok">STREAMING</span>
            <span className="response-meta">
              live events — the full body is saved to history when the
              connection closes
            </span>
          </div>
          <pre className="stream-live">{streamText}</pre>
        </div>
      );
    }
    return <div className="response-viewer empty">Sending…</div>;
  }
  if (error) {
    return (
      <div className="response-viewer">
        <div className="response-status">
          <span className="status-badge status-error">Error</span>
        </div>
        <pre className="response-error-text">{error}</pre>
      </div>
    );
  }
  if (!response) {
    return (
      <div className="response-viewer empty">
        Send a request to see the response. It is saved to history
        automatically.
      </div>
    );
  }

  return (
    <div className="response-viewer">
      <div className="response-status">
        <span className={`status-badge status-${statusClass(response.status)}`}>
          {response.status} {response.status_text}
        </span>
        <span className="response-meta">
          {response.http_version} · {formatDuration(response.duration_ms)} (TTFB{" "}
          {formatDuration(response.ttfb_ms)}) · {formatSize(response.size)}
          {response.body_truncated ? " · body truncated" : ""}
        </span>
        <span className="response-views">
          {(["pretty", "raw", "preview", "headers"] as View[]).map((v) => (
            <button
              key={v}
              className={view === v ? "active" : ""}
              onClick={() => setView(v)}
            >
              {v}
            </button>
          ))}
          {(response.tests.length > 0 ||
            response.console.length > 0 ||
            response.script_error) && (
            <button
              className={`${view === "tests" ? "active" : ""} ${
                response.tests.some((t) => !t.passed) || response.script_error
                  ? "tests-failed"
                  : "tests-passed"
              }`}
              onClick={() => setView("tests")}
            >
              tests ({response.tests.filter((t) => t.passed).length}/
              {response.tests.length})
            </button>
          )}
        </span>
      </div>
      <ResponseBody response={response} view={view} />
    </div>
  );
}

function ResponseBody({
  response,
  view,
}: {
  response: SendResult;
  view: View;
}) {
  const dark = usePrefersDark();
  const contentType =
    response.headers.find(([k]) => k.toLowerCase() === "content-type")?.[1] ??
    "";

  const prettyText = useMemo(() => {
    const text = response.body_text ?? "";
    if (view === "pretty" && contentType.includes("json")) {
      try {
        return JSON.stringify(JSON.parse(text), null, 2);
      } catch {
        return text;
      }
    }
    return text;
  }, [response.body_text, view, contentType]);

  if (view === "tests") {
    return (
      <div className="tests-panel">
        {response.script_error && (
          <div className="app-error">Script error: {response.script_error}</div>
        )}
        {response.tests.map((t, i) => (
          <div key={i} className={`test-row ${t.passed ? "pass" : "fail"}`}>
            <span className="test-mark">{t.passed ? "✓" : "✗"}</span>
            <span className="test-name">{t.name}</span>
            {t.error && <span className="test-error">{t.error}</span>}
          </div>
        ))}
        {response.console.length > 0 && (
          <>
            <div className="console-title">Console</div>
            {response.console.map((c, i) => (
              <div key={i} className={`console-line console-${c.level}`}>
                {c.message}
              </div>
            ))}
          </>
        )}
      </div>
    );
  }

  if (view === "headers") {
    return (
      <div className="response-headers">
        <table>
          <tbody>
            {response.headers.map(([k, v], i) => (
              <tr key={i}>
                <td className="header-name">{k}</td>
                <td className="header-value">{v}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  }

  if (view === "preview") {
    if (response.body_base64 && contentType.startsWith("image/")) {
      return (
        <div className="response-preview">
          <img
            src={`data:${contentType};base64,${response.body_base64}`}
            alt="response"
          />
        </div>
      );
    }
    if (contentType.includes("html")) {
      return (
        <iframe
          className="response-preview-frame"
          sandbox=""
          srcDoc={response.body_text ?? ""}
          title="preview"
        />
      );
    }
    return (
      <div className="response-viewer empty">
        No preview for {contentType || "this content type"}.
      </div>
    );
  }

  if (response.body_base64 && !response.body_text) {
    return (
      <div className="response-viewer empty">
        Binary response ({formatSize(response.size)}). Use Preview for images.
      </div>
    );
  }

  const extensions =
    view === "pretty"
      ? contentType.includes("json")
        ? [json()]
        : contentType.includes("html")
          ? [html()]
          : contentType.includes("xml")
            ? [xml()]
            : []
      : [];

  return (
    <CodeMirror
      className="response-code"
      value={view === "pretty" ? prettyText : (response.body_text ?? "")}
      readOnly
      height="100%"
      theme={dark ? "dark" : "light"}
      extensions={extensions}
    />
  );
}

function statusClass(status: number): string {
  if (status >= 200 && status < 300) return "ok";
  if (status >= 300 && status < 400) return "redirect";
  if (status >= 400 && status < 500) return "client-error";
  return "server-error";
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
