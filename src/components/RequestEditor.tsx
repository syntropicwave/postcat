import { useState } from "react";
import type { Tab } from "../state/tabs";
import { useTabs, parseParams, specFromTab, isWsUrl } from "../state/tabs";
import { itemScriptsSet, itemUpdate } from "../ipc/commands";
import { ScriptsEditor } from "./ScriptsEditor";
import { HTTP_METHODS } from "../types";
import { KeyValueEditor } from "./KeyValueEditor";
import { BodyEditor } from "./BodyEditor";
import { UrlInput } from "./UrlInput";
import { VarInput } from "./VarInput";
import { SaveDialog } from "./SaveDialog";
import { AuthEditor } from "./AuthEditor";
import { CodeDialog } from "./CodeDialog";

/** Save a bound tab in place; unbound tabs open the SaveDialog instead. */
export async function saveBoundTab(tab: Tab): Promise<boolean> {
  if (!tab.itemId) return false;
  await itemUpdate(tab.itemId, {
    spec: specFromTab(tab),
    description: tab.description,
  });
  await itemScriptsSet(
    tab.itemId,
    tab.preRequestScript || null,
    tab.testScript || null,
  );
  useTabs.getState().updateTab(tab.id, { dirty: false });
  useTabs.getState().bumpCollections();
  return true;
}

/**
 * Path variables: `:id`-style segments in the URL become editable rows. The
 * value fills a `{{__path_<name>}}`-free substitution done here on send by
 * rewriting the URL — but simplest is to keep values in the URL directly, so
 * here we just surface them and let the user rename/fill via the URL itself.
 * We track values in a local map and rewrite the URL when they change.
 */
function PathVariables({ tab }: { tab: Tab }) {
  const { updateTab } = useTabs();
  const segments = extractPathVars(tab.url);
  if (segments.length === 0) return null;

  return (
    <div className="path-vars">
      <div className="path-vars-title">Path variables</div>
      {segments.map((name) => (
        <div className="kv-row" key={name}>
          <span className="path-var-name">:{name}</span>
          <VarInput
            className="kv-value"
            value={tab.pathVars[name] ?? ""}
            collectionId={tab.collectionId}
            placeholder={`value for :${name}`}
            onChange={(value) =>
              updateTab(tab.id, {
                pathVars: { ...tab.pathVars, [name]: value },
                dirty: true,
              })
            }
          />
        </div>
      ))}
    </div>
  );
}

export function extractPathVars(url: string): string[] {
  const base = url.split("?")[0];
  const found = new Set<string>();
  const re = /\/:([A-Za-z0-9_]+)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(base)) !== null) found.add(m[1]);
  return [...found];
}

/** Method dropdown that also accepts a typed custom method (PROPFIND, PURGE…). */
function MethodSelect({
  method,
  onChange,
}: {
  method: string;
  onChange: (method: string) => void;
}) {
  const [custom, setCustom] = useState(
    !HTTP_METHODS.includes(method as (typeof HTTP_METHODS)[number]),
  );

  if (custom) {
    return (
      <input
        className={`method method-${method}`}
        value={method}
        autoFocus
        spellCheck={false}
        title="Custom method — pick from the list to go back"
        onChange={(e) => onChange(e.target.value.toUpperCase())}
        onBlur={(e) => {
          if (e.target.value.trim() === "") {
            setCustom(false);
            onChange("GET");
          }
        }}
      />
    );
  }
  return (
    <select
      className={`method method-${method}`}
      value={method}
      onChange={(e) => {
        if (e.target.value === "__custom__") {
          setCustom(true);
          onChange("");
        } else {
          onChange(e.target.value);
        }
      }}
    >
      {HTTP_METHODS.map((m) => (
        <option key={m} value={m}>
          {m}
        </option>
      ))}
      <option value="__custom__">Custom…</option>
    </select>
  );
}

/** Per-request send settings (timeout, redirects, SSL). */
function SettingsPopover({ tab }: { tab: Tab }) {
  const { updateTab } = useTabs();
  const [open, setOpen] = useState(false);
  const s = tab.settings;
  const patch = (p: Partial<typeof s>) =>
    updateTab(tab.id, { settings: { ...s, ...p }, dirty: true });

  return (
    <div className="settings-popover-wrap">
      <button
        type="button"
        className={`icon-btn${open ? " active" : ""}`}
        title="Request settings (timeout, redirects, SSL)"
        onClick={() => setOpen((v) => !v)}
      >
        ⏱
      </button>
      {open && (
        <div className="settings-popover">
          <label>
            Timeout (ms, 0 = none — needed for SSE)
            <input
              type="number"
              min={0}
              value={s.timeout_ms}
              onChange={(e) =>
                patch({ timeout_ms: Math.max(0, Number(e.target.value)) })
              }
            />
          </label>
          <label className="auth-check">
            <input
              type="checkbox"
              checked={s.follow_redirects}
              onChange={(e) => patch({ follow_redirects: e.target.checked })}
            />
            follow redirects (max {s.max_redirects})
          </label>
          <label className="auth-check">
            <input
              type="checkbox"
              checked={s.verify_ssl}
              onChange={(e) => patch({ verify_ssl: e.target.checked })}
            />
            verify SSL certificates
          </label>
          <button className="filter-reset" onClick={() => setOpen(false)}>
            close
          </button>
        </div>
      )}
    </div>
  );
}

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

type Section = "params" | "auth" | "headers" | "body" | "scripts" | "docs";

export function RequestEditor({ tab }: { tab: Tab }) {
  const { updateTab, setUrl, setParams, send, cancel } = useTabs();
  const [section, setSection] = useState<Section>("params");
  const [saveOpen, setSaveOpen] = useState(false);
  const [codeOpen, setCodeOpen] = useState(false);

  const saveTab = async (t: Tab) => {
    if (!(await saveBoundTab(t))) setSaveOpen(true);
  };

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
        <MethodSelect
          method={tab.method}
          onChange={(method) => updateTab(tab.id, { method, dirty: true })}
        />
        <UrlInput
          value={tab.url}
          collectionId={tab.collectionId}
          onChange={(url) => setUrl(tab.id, url)}
          onCurl={(spec) =>
            updateTab(tab.id, {
              method: spec.method,
              url: spec.url,
              params: parseParams(spec.url),
              headers: spec.headers,
              body: spec.body,
              settings: spec.settings,
              dirty: true,
            })
          }
        />
        {isWsUrl(tab.url) ? (
          <button
            type="submit"
            className={`send-btn${tab.wsStatus !== "closed" ? " cancel" : ""}`}
            disabled={!tab.url.trim()}
          >
            {tab.wsStatus === "closed"
              ? "Connect"
              : tab.wsStatus === "connecting"
                ? "…"
                : "Disconnect"}
          </button>
        ) : tab.sending ? (
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
        <SettingsPopover tab={tab} />
        <button
          type="button"
          className="save-btn"
          title="Save to collection (Ctrl+S)"
          onClick={() => void saveTab(tab)}
        >
          Save{tab.dirty && tab.itemId ? " •" : ""}
        </button>
        <button
          type="button"
          className="save-btn"
          title="Generate code snippet"
          onClick={() => setCodeOpen(true)}
        >
          {"</>"}
        </button>
      </form>

      {codeOpen && <CodeDialog tab={tab} onClose={() => setCodeOpen(false)} />}

      {saveOpen && <SaveDialog tab={tab} onClose={() => setSaveOpen(false)} />}

      <div className="section-tabs">
        <button
          className={section === "params" ? "active" : ""}
          onClick={() => setSection("params")}
        >
          Params{badge(enabledParams)}
        </button>
        <button
          className={section === "auth" ? "active" : ""}
          onClick={() => setSection("auth")}
        >
          Auth{tab.auth.kind !== "none" ? " •" : ""}
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
        <button
          className={section === "scripts" ? "active" : ""}
          onClick={() => setSection("scripts")}
        >
          Scripts{tab.preRequestScript || tab.testScript ? " •" : ""}
        </button>
        <button
          className={section === "docs" ? "active" : ""}
          onClick={() => setSection("docs")}
        >
          Docs{tab.description ? " •" : ""}
        </button>
      </div>

      <div className="section-content">
        {section === "params" && (
          <>
            <KeyValueEditor
              rows={tab.params}
              onChange={(rows) => setParams(tab.id, rows)}
              keyPlaceholder="param"
              collectionId={tab.collectionId}
            />
            <PathVariables tab={tab} />
          </>
        )}
        {section === "auth" && (
          <AuthEditor
            auth={tab.auth}
            allowInherit={tab.collectionId !== null}
            onChange={(auth) => updateTab(tab.id, { auth, dirty: true })}
          />
        )}
        {section === "headers" && (
          <KeyValueEditor
            rows={tab.headers}
            onChange={(rows) =>
              updateTab(tab.id, { headers: rows, dirty: true })
            }
            keyPlaceholder="header"
            keySuggestions={COMMON_HEADERS}
            suggestionsId="header-suggestions"
            collectionId={tab.collectionId}
          />
        )}
        {section === "body" && (
          <BodyEditor
            body={tab.body}
            url={tab.url}
            headers={tab.headers}
            collectionId={tab.collectionId}
            onChange={(body) => updateTab(tab.id, { body, dirty: true })}
          />
        )}
        {section === "scripts" && (
          <ScriptsEditor
            preRequestScript={tab.preRequestScript}
            testScript={tab.testScript}
            onChange={(pre, test) =>
              updateTab(tab.id, {
                preRequestScript: pre,
                testScript: test,
                dirty: true,
              })
            }
          />
        )}
        {section === "docs" && (
          <textarea
            className="docs-editor"
            value={tab.description}
            placeholder="Describe this request (Markdown). Saved with the request in its collection."
            onChange={(e) =>
              updateTab(tab.id, { description: e.target.value, dirty: true })
            }
          />
        )}
      </div>
    </div>
  );
}
