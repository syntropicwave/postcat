import CodeMirror from "@uiw/react-codemirror";
import { json } from "@codemirror/lang-json";
import { xml } from "@codemirror/lang-xml";
import { open } from "@tauri-apps/plugin-dialog";
import type { BodySpec, FormField, KeyValue } from "../types";
import { KeyValueEditor } from "./KeyValueEditor";
import { GraphQLEditor } from "./GraphQLEditor";
import { usePrefersDark } from "../hooks/usePrefersDark";

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
              ? [json()]
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
