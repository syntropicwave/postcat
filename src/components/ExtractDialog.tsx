import { useMemo, useState } from "react";
import { varsSave, varsGet, envList } from "../ipc/commands";
import type { VarScope } from "../types";

interface Props {
  bodyText: string;
  collectionId: number | null;
  onClose: () => void;
}

interface Leaf {
  path: string;
  value: string;
}

/**
 * "Extract to variable": flatten a JSON response into dot/bracket paths and
 * let the user store any value into a variable scope — no test script needed.
 */
export function ExtractDialog({ bodyText, collectionId, onClose }: Props) {
  const leaves = useMemo(() => flatten(bodyText), [bodyText]);
  const [filter, setFilter] = useState("");
  const [selected, setSelected] = useState<Leaf | null>(null);
  const [varName, setVarName] = useState("");
  const [scope, setScope] = useState<VarScope>("environment");
  const [saved, setSaved] = useState(false);

  const shown = leaves.filter(
    (l) =>
      l.path.toLowerCase().includes(filter.toLowerCase()) ||
      l.value.toLowerCase().includes(filter.toLowerCase()),
  );

  const pick = (leaf: Leaf) => {
    setSelected(leaf);
    setVarName(
      leaf.path
        .split(/[.[\]]/)
        .filter(Boolean)
        .pop() ?? "value",
    );
  };

  const doSave = async () => {
    if (!selected || !varName.trim()) return;
    const ownerId =
      scope === "environment"
        ? ((await envList()).find((e) => e.is_active)?.id ?? null)
        : scope === "collection"
          ? collectionId
          : null;
    if (scope === "environment" && ownerId === null) {
      setSaved(false);
      alert("No active environment — pick another scope or activate one.");
      return;
    }
    const existing = await varsGet(scope, ownerId);
    const next = existing.filter((v) => v.key !== varName.trim());
    next.push({
      key: varName.trim(),
      initial_value: selected.value,
      current_value: null,
      is_secret: false,
      enabled: true,
    });
    await varsSave(scope, ownerId, next);
    setSaved(true);
    setTimeout(onClose, 700);
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal modal-wide" onClick={(e) => e.stopPropagation()}>
        <div className="retention-title">Extract to variable</div>
        <input
          className="history-search"
          autoFocus
          placeholder="Filter fields…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <div className="extract-list">
          {shown.slice(0, 200).map((leaf) => (
            <div
              key={leaf.path}
              className={`extract-row${selected?.path === leaf.path ? " active" : ""}`}
              onClick={() => pick(leaf)}
            >
              <span className="extract-path">{leaf.path}</span>
              <span className="extract-value">{leaf.value}</span>
            </div>
          ))}
          {leaves.length === 0 && (
            <div className="history-empty">
              Response is not JSON — nothing to extract.
            </div>
          )}
        </div>

        {selected && (
          <div className="extract-form">
            <input
              value={varName}
              placeholder="variable name"
              onChange={(e) => setVarName(e.target.value)}
            />
            <select
              value={scope}
              onChange={(e) => setScope(e.target.value as VarScope)}
            >
              <option value="environment">Environment</option>
              {collectionId !== null && (
                <option value="collection">Collection</option>
              )}
              <option value="global">Global</option>
            </select>
            <button className="primary" onClick={() => void doSave()}>
              {saved ? "Saved ✓" : "Save"}
            </button>
          </div>
        )}
        <div className="retention-actions">
          <button onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}

/** Flatten JSON into leaf paths (`a.b[0].c`). Non-JSON → empty. */
function flatten(text: string): Leaf[] {
  let root: unknown;
  try {
    root = JSON.parse(text);
  } catch {
    return [];
  }
  const out: Leaf[] = [];
  const walk = (value: unknown, path: string) => {
    if (value === null || typeof value !== "object") {
      out.push({ path: path || "$", value: String(value) });
      return;
    }
    if (Array.isArray(value)) {
      value.forEach((v, i) => walk(v, `${path}[${i}]`));
    } else {
      for (const [k, v] of Object.entries(value)) {
        walk(v, path ? `${path}.${k}` : k);
      }
    }
  };
  walk(root, "");
  return out;
}
