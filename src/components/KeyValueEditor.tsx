import { useState } from "react";
import type { KeyValue } from "../types";
import { VarInput } from "./VarInput";

interface Props {
  rows: KeyValue[];
  onChange: (rows: KeyValue[]) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  keySuggestions?: string[];
  suggestionsId?: string;
  /** Enables `{{var}}` autocomplete in value cells. */
  collectionId?: number | null;
}

/**
 * Key-value table with per-row enable checkboxes. Always shows one trailing
 * empty row; typing into it appends a real row. A bulk-edit toggle swaps the
 * table for a `key: value` textarea (paste headers straight from DevTools).
 */
export function KeyValueEditor({
  rows,
  onChange,
  keyPlaceholder = "key",
  valuePlaceholder = "value",
  keySuggestions,
  suggestionsId,
  collectionId = null,
}: Props) {
  const [bulk, setBulk] = useState(false);

  const update = (idx: number, patch: Partial<KeyValue>) => {
    if (idx === rows.length) {
      onChange([...rows, { key: "", value: "", enabled: true, ...patch }]);
    } else {
      onChange(rows.map((r, i) => (i === idx ? { ...r, ...patch } : r)));
    }
  };

  const remove = (idx: number) => {
    onChange(rows.filter((_, i) => i !== idx));
  };

  if (bulk) {
    return (
      <div className="kv-editor">
        <div className="kv-toolbar">
          <button className="kv-mode" onClick={() => setBulk(false)}>
            Table edit
          </button>
        </div>
        <textarea
          className="kv-bulk"
          value={toBulk(rows)}
          placeholder={
            "Content-Type: application/json\nAuthorization: Bearer {{token}}"
          }
          onChange={(e) => onChange(fromBulk(e.target.value))}
        />
      </div>
    );
  }

  const display: KeyValue[] = [...rows, { key: "", value: "", enabled: true }];

  return (
    <div className="kv-editor">
      <div className="kv-toolbar">
        <button className="kv-mode" onClick={() => setBulk(true)}>
          Bulk edit
        </button>
      </div>
      {keySuggestions && suggestionsId && (
        <datalist id={suggestionsId}>
          {keySuggestions.map((s) => (
            <option key={s} value={s} />
          ))}
        </datalist>
      )}
      {display.map((row, idx) => {
        const isGhost = idx === rows.length;
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
              placeholder={keyPlaceholder}
              list={suggestionsId}
              onChange={(e) => update(idx, { key: e.target.value })}
            />
            <VarInput
              className="kv-value"
              value={row.value}
              collectionId={collectionId}
              placeholder={valuePlaceholder}
              onChange={(value) => update(idx, { value })}
            />
            <button
              className="kv-remove"
              title="Remove row"
              disabled={isGhost}
              onClick={() => remove(idx)}
            >
              ×
            </button>
          </div>
        );
      })}
    </div>
  );
}

function toBulk(rows: KeyValue[]): string {
  return rows
    .map((r) => `${r.enabled ? "" : "# "}${r.key}: ${r.value}`)
    .join("\n");
}

function fromBulk(text: string): KeyValue[] {
  return text
    .split("\n")
    .filter((line) => line.trim() !== "")
    .map((line) => {
      const enabled = !line.trimStart().startsWith("#");
      const clean = enabled ? line : line.replace(/^\s*#\s?/, "");
      const idx = clean.indexOf(":");
      if (idx === -1) return { key: clean.trim(), value: "", enabled };
      return {
        key: clean.slice(0, idx).trim(),
        value: clean.slice(idx + 1).trim(),
        enabled,
      };
    });
}
