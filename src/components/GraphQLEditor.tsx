import { useState } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { javascript } from "@codemirror/lang-javascript";
import { graphqlIntrospect } from "../ipc/commands";
import type { KeyValue } from "../types";
import { usePrefersDark } from "../hooks/usePrefersDark";

interface Props {
  query: string;
  variables: string;
  url: string;
  headers: KeyValue[];
  collectionId: number | null;
  onChange: (query: string, variables: string) => void;
}

interface SchemaField {
  name: string;
  args: string;
  type: string;
  description: string | null;
}

interface SchemaSection {
  title: string;
  fields: SchemaField[];
}

export function GraphQLEditor({
  query,
  variables,
  url,
  headers,
  collectionId,
  onChange,
}: Props) {
  const dark = usePrefersDark();
  const [pane, setPane] = useState<"query" | "variables">("query");
  const [schema, setSchema] = useState<SchemaSection[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchSchema = async () => {
    setLoading(true);
    setError(null);
    try {
      const raw = await graphqlIntrospect(url, headers, collectionId);
      setSchema(parseIntrospection(raw));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const insertField = (field: SchemaField, section: string) => {
    const op = section === "Mutations" ? "mutation" : "query";
    const args = field.args ? `(${field.args})` : "";
    const snippet = `${op} {\n  ${field.name}${args} {\n    \n  }\n}\n`;
    onChange(
      query.trim() ? `${query.trimEnd()}\n\n${snippet}` : snippet,
      variables,
    );
  };

  return (
    <div className="graphql-editor">
      <div className="graphql-main">
        <div className="scripts-tabs">
          <button
            className={pane === "query" ? "active" : ""}
            onClick={() => setPane("query")}
          >
            Query
          </button>
          <button
            className={pane === "variables" ? "active" : ""}
            onClick={() => setPane("variables")}
          >
            Variables{variables.trim() ? " •" : ""}
          </button>
          <span style={{ flex: 1 }} />
          <button
            className="icon-btn"
            disabled={loading || !url.trim()}
            title="Fetch schema (introspection)"
            onClick={() => void fetchSchema()}
          >
            {loading ? "Loading…" : schema ? "↻ Schema" : "Schema"}
          </button>
        </div>
        {pane === "query" ? (
          <CodeMirror
            key="q"
            className="body-code"
            value={query}
            height="100%"
            theme={dark ? "dark" : "light"}
            placeholder={
              "query {\n  users(limit: 10) {\n    id\n    name\n  }\n}"
            }
            onChange={(v) => onChange(v, variables)}
          />
        ) : (
          <CodeMirror
            key="v"
            className="body-code"
            value={variables}
            height="100%"
            theme={dark ? "dark" : "light"}
            placeholder='{ "limit": 10 }'
            extensions={[javascript()]}
            onChange={(v) => onChange(query, v)}
          />
        )}
        {error && <div className="app-error">{error}</div>}
      </div>

      {schema && (
        <div className="graphql-schema">
          {schema.map((section) => (
            <div key={section.title}>
              <div className="schema-section">{section.title}</div>
              {section.fields.map((f) => (
                <div
                  key={f.name}
                  className="schema-field"
                  title={f.description ?? undefined}
                  onClick={() => insertField(f, section.title)}
                >
                  <span className="schema-name">{f.name}</span>
                  {f.args && <span className="schema-args">({f.args})</span>}
                  <span className="schema-type">: {f.type}</span>
                </div>
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/* ------------ introspection JSON -> flat sections ------------ */

interface RawType {
  kind: string;
  name: string | null;
  ofType?: RawType | null;
  fields?: RawField[] | null;
}

interface RawField {
  name: string;
  description: string | null;
  args: { name: string; type: RawType }[];
  type: RawType;
}

function parseIntrospection(raw: unknown): SchemaSection[] {
  const root = raw as {
    data?: { __schema?: Record<string, unknown> };
    __schema?: Record<string, unknown>;
  };
  const schema = (root.data?.__schema ?? root.__schema) as
    | {
        queryType?: { name: string } | null;
        mutationType?: { name: string } | null;
        types: { name: string; fields: RawField[] | null }[];
      }
    | undefined;
  if (!schema) throw new Error("no __schema in response");

  const sections: SchemaSection[] = [];
  const add = (title: string, typeName?: string | null) => {
    if (!typeName) return;
    const type = schema.types.find((t) => t.name === typeName);
    if (!type?.fields) return;
    sections.push({
      title,
      fields: type.fields.map((f) => ({
        name: f.name,
        description: f.description,
        args: f.args.map((a) => `${a.name}: ${typeName2(a.type)}`).join(", "),
        type: typeName2(f.type),
      })),
    });
  };
  add("Queries", schema.queryType?.name);
  add("Mutations", schema.mutationType?.name);
  return sections;
}

function typeName2(t: RawType | null | undefined): string {
  if (!t) return "?";
  if (t.kind === "NON_NULL") return `${typeName2(t.ofType)}!`;
  if (t.kind === "LIST") return `[${typeName2(t.ofType)}]`;
  return t.name ?? "?";
}
