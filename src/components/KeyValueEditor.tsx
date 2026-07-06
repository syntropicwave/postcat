import type { KeyValue } from "../types";

interface Props {
  rows: KeyValue[];
  onChange: (rows: KeyValue[]) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  keySuggestions?: string[];
  suggestionsId?: string;
}

/**
 * Key-value table with per-row enable checkboxes. Always shows one trailing
 * empty row; typing into it appends a real row.
 */
export function KeyValueEditor({
  rows,
  onChange,
  keyPlaceholder = "key",
  valuePlaceholder = "value",
  keySuggestions,
  suggestionsId,
}: Props) {
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

  const display: KeyValue[] = [...rows, { key: "", value: "", enabled: true }];

  return (
    <div className="kv-editor">
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
            <input
              className="kv-value"
              value={row.value}
              placeholder={valuePlaceholder}
              onChange={(e) => update(idx, { value: e.target.value })}
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
