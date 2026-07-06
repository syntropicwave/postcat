import { useState } from "react";
import type { Tab } from "../state/tabs";
import { useTabs } from "../state/tabs";
import { HTTP_METHODS } from "../types";
import { KeyValueEditor } from "./KeyValueEditor";
import { BodyEditor } from "./BodyEditor";

const COMMON_HEADERS = [
  "Accept",
  "Accept-Encoding",
  "Accept-Language",
  "Authorization",
  "Cache-Control",
  "Content-Type",
  "Cookie",
  "If-Match",
  "If-None-Match",
  "Origin",
  "Referer",
  "User-Agent",
  "X-Api-Key",
  "X-Request-Id",
];

type Section = "params" | "headers" | "body";

export function RequestEditor({ tab }: { tab: Tab }) {
  const { updateTab, setUrl, setParams, send, cancel } = useTabs();
  const [section, setSection] = useState<Section>("params");

  const badge = (n: number) => (n > 0 ? ` (${n})` : "");
  const enabledParams = tab.params.filter((p) => p.enabled && p.key).length;
  const enabledHeaders = tab.headers.filter((h) => h.enabled && h.key).length;

  return (
    <div className="request-editor">
      <form
        className="url-bar"
        onSubmit={(e) => {
          e.preventDefault();
          void send(tab.id);
        }}
      >
        <select
          className={`method method-${tab.method}`}
          value={tab.method}
          onChange={(e) => updateTab(tab.id, { method: e.target.value })}
        >
          {HTTP_METHODS.map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
        <input
          className="url-input"
          value={tab.url}
          placeholder="https://api.example.com/v1/users?limit=10"
          spellCheck={false}
          onChange={(e) => setUrl(tab.id, e.target.value)}
        />
        {tab.sending ? (
          <button
            type="button"
            className="send-btn cancel"
            onClick={() => cancel(tab.id)}
          >
            Cancel
          </button>
        ) : (
          <button type="submit" className="send-btn" disabled={!tab.url.trim()}>
            Send
          </button>
        )}
      </form>

      <div className="section-tabs">
        <button
          className={section === "params" ? "active" : ""}
          onClick={() => setSection("params")}
        >
          Params{badge(enabledParams)}
        </button>
        <button
          className={section === "headers" ? "active" : ""}
          onClick={() => setSection("headers")}
        >
          Headers{badge(enabledHeaders)}
        </button>
        <button
          className={section === "body" ? "active" : ""}
          onClick={() => setSection("body")}
        >
          Body{tab.body.kind !== "none" ? " •" : ""}
        </button>
      </div>

      <div className="section-content">
        {section === "params" && (
          <KeyValueEditor
            rows={tab.params}
            onChange={(rows) => setParams(tab.id, rows)}
            keyPlaceholder="param"
          />
        )}
        {section === "headers" && (
          <KeyValueEditor
            rows={tab.headers}
            onChange={(rows) => updateTab(tab.id, { headers: rows })}
            keyPlaceholder="header"
            keySuggestions={COMMON_HEADERS}
            suggestionsId="header-suggestions"
          />
        )}
        {section === "body" && (
          <BodyEditor
            body={tab.body}
            onChange={(body) => updateTab(tab.id, { body })}
          />
        )}
      </div>
    </div>
  );
}
