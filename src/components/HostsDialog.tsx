import { useState } from "react";
import { useHostAliases, ALIAS_COLORS } from "../state/hostAliases";
import { HostChip } from "./HostChip";
import { Icon } from "./Icon";

/** Manage all saved host aliases: rename, recolour, add or remove. */
export function HostsDialog({ onClose }: { onClose: () => void }) {
  const aliases = useHostAliases((s) => s.aliases);
  const upsert = useHostAliases((s) => s.upsert);
  const remove = useHostAliases((s) => s.remove);

  const [newHost, setNewHost] = useState("");
  const [newAlias, setNewAlias] = useState("");
  const [newColor, setNewColor] = useState(ALIAS_COLORS[0]);

  const addNew = () => {
    const h = newHost.trim();
    const a = newAlias.trim();
    if (!h || !a) return;
    void upsert(h, a, newColor);
    setNewHost("");
    setNewAlias("");
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal settings-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="retention-title">Host aliases</div>
        <div className="settings-hint">
          A short coloured label shown in place of a host across the address
          bar, history and tabs.
        </div>

        <div className="settings-body">
          {aliases.length === 0 && (
            <div className="hosts-empty">
              No aliases yet. Put the caret on a host in the address bar to save
              one, or add it below.
            </div>
          )}

          {aliases.map((a) => (
            <HostRow
              key={a.id}
              host={a.host}
              alias={a.alias}
              color={a.color}
              onSave={(alias, color) => void upsert(a.host, alias, color)}
              onRemove={() => void remove(a.id)}
            />
          ))}

          <div className="settings-section">Add alias</div>
          <div className="host-add">
            <input
              className="alias-name"
              value={newHost}
              placeholder="https://api.example.com/v1"
              spellCheck={false}
              onChange={(e) => setNewHost(e.target.value)}
            />
            <input
              className="alias-name"
              value={newAlias}
              placeholder="alias"
              spellCheck={false}
              onChange={(e) => setNewAlias(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && addNew()}
            />
            <Swatches value={newColor} onChange={setNewColor} />
            <button
              className="primary"
              disabled={!newHost.trim() || !newAlias.trim()}
              onClick={addNew}
            >
              Add
            </button>
          </div>
        </div>

        <div className="retention-actions">
          <button className="primary" onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}

function HostRow({
  host,
  alias,
  color,
  onSave,
  onRemove,
}: {
  host: string;
  alias: string;
  color: string;
  onSave: (alias: string, color: string) => void;
  onRemove: () => void;
}) {
  const [name, setName] = useState(alias);
  const [col, setCol] = useState(color || ALIAS_COLORS[0]);
  const dirty = name.trim() !== alias || col !== (color || ALIAS_COLORS[0]);

  return (
    <div className="host-row">
      <HostChip alias={name.trim() || alias} color={col} host={host} />
      <div className="host-row-body">
        <span className="host-row-host" title={host}>
          {host}
        </span>
        <div className="host-row-edit">
          <input
            className="alias-name"
            value={name}
            spellCheck={false}
            onChange={(e) => setName(e.target.value)}
          />
          <Swatches value={col} onChange={setCol} />
          <button
            className="icon-btn"
            title="Save"
            disabled={!name.trim() || !dirty}
            onClick={() => onSave(name.trim(), col)}
          >
            <Icon name="check" />
          </button>
          <button className="icon-btn" title="Remove" onClick={onRemove}>
            <Icon name="trash" />
          </button>
        </div>
      </div>
    </div>
  );
}

function Swatches({
  value,
  onChange,
}: {
  value: string;
  onChange: (c: string) => void;
}) {
  return (
    <div className="alias-swatches">
      {ALIAS_COLORS.map((c) => (
        <button
          key={c}
          type="button"
          className={`alias-swatch${c === value ? " active" : ""}`}
          style={{ background: c }}
          onClick={() => onChange(c)}
        />
      ))}
    </div>
  );
}
