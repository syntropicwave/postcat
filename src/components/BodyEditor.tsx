import CodeMirror from "@uiw/react-codemirror";
import { EditorView } from "@codemirror/view";
import { json } from "@codemirror/lang-json";
import { xml } from "@codemirror/lang-xml";
import { open } from "@tauri-apps/plugin-dialog";
import type { BodySpec, FormField, KeyValue } from "../types";
import { KeyValueEditor } from "./KeyValueEditor";
import { GraphQLEditor } from "./GraphQLEditor";
import { usePrefersDark } from "../hooks/usePrefersDark";

/** Decode a JSON document pasted in string-escaped form — either the bare
 * contents of a string literal (`{\"a\":1}`) or a quoted literal
 * (`"{\"a\":1}"`), possibly double-encoded. Returns pretty-printed JSON, or
 * null when the text is not escaped JSON (plain JSON returns null too, so
 * callers know nothing needs rewriting). */
export function decodeEscapedJson(raw: string): string | null {
  const text = raw.trim();
  if (!text) return null;
  let value: unknown = text;
  let layers = 0;
  while (typeof value === "string" && layers < 4) {
    const s = value;
    try {
      value = JSON.parse(s);
    } catch {
      // Not valid JSON as-is. On the first layer, `\"` escapes suggest the
      // clipboard holds the *contents* of a string literal — re-wrap it in
      // quotes (escaping any literal control chars) and parse that.
      if (layers > 0 || !s.includes('\\"')) return null;
      try {
        value = JSON.parse(
          `"${s
            .replace(/\r/g, "\\r")
            .replace(/\n/g, "\\n")
            .replace(/\t/g, "\\t")}"`,
        );
      } catch {
        return null;
      }
    }
    layers++;
  }
  if (layers < 2 || typeof value !== "object" || value === null) return null;
  return JSON.stringify(value, null, 2);
}

/** Paste that replaces the whole body and looks like escaped JSON is decoded
 * and pretty-printed in place of the raw clipboard text. */
const pasteDecodesEscapedJson = EditorView.domEventHandlers({
  paste(event, view) {
    const { from, to } = view.state.selection.main;
    if (!(from === 0 && to === view.state.doc.length)) return false;
    const clip = event.clipboardData?.getData("text/plain") ?? "";
    const decoded = decodeEscapedJson(clip);
    if (!decoded) return false;
    event.preventDefault();
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: decoded },
    });
    return true;
  },
});

interface Props {
  body: BodySpec;
  onChange: (body: BodySpec) => void;
  /** Context for GraphQL introspection. */
  url: string;
  headers: KeyValue[];
  collectionId: number | null;
}

const RAW_TYPES: { label: string; contentType: string }[] = [
  { label: "JSON", contentType: "application/json" },
  { label: "Text", contentType: "text/plain" },
  { label: "XML", contentType: "application/xml" },
  { label: "HTML", contentType: "text/html" },
];

export function BodyEditor({
  body,
  onChange,
  url,
  headers,
  collectionId,
}: Props) {
  const dark = usePrefersDark();
  const setKind = (kind: string) => {
    switch (kind) {
      case "none":
        onChange({ kind: "none" });
        break;
      case "raw":
        onChange({ kind: "raw", content_type: "application/json", text: "" });
        break;
      case "url_encoded":
        onChange({ kind: "url_encoded", fields: [] });
        break;
      case "form_data":
        onChange({ kind: "form_data", fields: [] });
        break;
      case "binary":
        onChange({ kind: "binary", path: "" });
        break;
      case "graphql":
        onChange({ kind: "graphql", query: "", variables: "" });
        break;
    }
  };

  const beautify = () => {
    if (body.kind !== "raw") return;
    const decoded = decodeEscapedJson(body.text);
    if (decoded) {
      onChange({ ...body, text: decoded });
      return;
    }
    try {
      onChange({
        ...body,
        text: JSON.stringify(JSON.parse(body.text), null, 2),
      });
    } catch {
      // not valid JSON — leave as is
    }
  };

  return (
    <div className="body-editor">
      <div className="body-toolbar">
        <select value={body.kind} onChange={(e) => setKind(e.target.value)}>
          <option value="none">none</option>
          <option value="raw">raw</option>
          <option value="url_encoded">x-www-form-urlencoded</option>
          <option value="form_data">form-data</option>
          <option value="binary">binary</option>
          <option value="graphql">GraphQL</option>
        </select>

        {body.kind === "raw" && (
          <>
            <select
              value={body.content_type}
              onChange={(e) =>
                onChange({ ...body, content_type: e.target.value })
              }
            >
              {RAW_TYPES.map((t) => (
                <option key={t.contentType} value={t.contentType}>
                  {t.label}
                </option>
              ))}
            </select>
            {body.content_type === "application/json" && (
              <button onClick={beautify}>Beautify</button>
            )}
          </>
        )}
      </div>

      {body.kind === "raw" && (
        <CodeMirror
          value={body.text}
          height="100%"
          theme={dark ? "dark" : "light"}
          className="body-code"
          extensions={
            body.content_type === "application/json"
              ? [json(), pasteDecodesEscapedJson]
              : body.content_type.includes("xml") ||
                  body.content_type.includes("html")
                ? [xml()]
                : []
          }
          onChange={(text) => onChange({ ...body, text })}
        />
      )}

      {body.kind === "url_encoded" && (
        <KeyValueEditor
          rows={body.fields}
          onChange={(fields: KeyValue[]) => onChange({ ...body, fields })}
        />
      )}

      {body.kind === "form_data" && (
        <FormDataEditor
          fields={body.fields}
          onChange={(fields) => onChange({ ...body, fields })}
        />
      )}

      {body.kind === "graphql" && (
        <GraphQLEditor
          query={body.query}
          variables={body.variables}
          url={url}
          headers={headers}
          collectionId={collectionId}
          onChange={(query, variables) =>
            onChange({ kind: "graphql", query, variables })
          }
        />
      )}

      {body.kind === "binary" && (
        <div className="binary-picker">
          <button
            onClick={async () => {
              const path = await open({ multiple: false });
              if (typeof path === "string") onChange({ ...body, path });
            }}
          >
            Choose file…
          </button>
          <span className="binary-path">{body.path || "no file selected"}</span>
        </div>
      )}
    </div>
  );
}

function FormDataEditor({
  fields,
  onChange,
}: {
  fields: FormField[];
  onChange: (fields: FormField[]) => void;
}) {
  const update = (idx: number, patch: Partial<FormField>) => {
    if (idx === fields.length) {
      onChange([
        ...fields,
        { key: "", value: "", enabled: true, is_file: false, ...patch },
      ]);
    } else {
      onChange(fields.map((f, i) => (i === idx ? { ...f, ...patch } : f)));
    }
  };

  const display: FormField[] = [
    ...fields,
    { key: "", value: "", enabled: true, is_file: false },
  ];

  return (
    <div className="kv-editor">
      {display.map((row, idx) => {
        const isGhost = idx === fields.length;
        return (
          <div className={`kv-row${isGhost ? " kv-ghost" : ""}`} key={idx}>
            <input
              type="checkbox"
              checked={row.enabled}
              disabled={isGhost}
              onChange={(e) => update(idx, { enabled: e.target.checked })}
            />
            <input
              className="kv-key"
              value={row.key}
              placeholder="field"
              onChange={(e) => update(idx, { key: e.target.value })}
            />
            {row.is_file ? (
              <button
                className="kv-file"
                onClick={async () => {
                  const path = await open({ multiple: false });
                  if (typeof path === "string") update(idx, { value: path });
                }}
              >
                {row.value ? row.value.split(/[\\/]/).pop() : "Choose file…"}
              </button>
            ) : (
              <input
                className="kv-value"
                value={row.value}
                placeholder="value"
                onChange={(e) => update(idx, { value: e.target.value })}
              />
            )}
            <select
              value={row.is_file ? "file" : "text"}
              disabled={isGhost}
              onChange={(e) =>
                update(idx, { is_file: e.target.value === "file", value: "" })
              }
            >
              <option value="text">text</option>
              <option value="file">file</option>
            </select>
            <button
              className="kv-remove"
              disabled={isGhost}
              onClick={() => onChange(fields.filter((_, i) => i !== idx))}
            >
              ×
            </button>
          </div>
        );
      })}
    </div>
  );
}
