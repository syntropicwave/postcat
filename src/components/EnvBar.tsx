import { useEffect, useState } from "react";
import { confirm, save } from "@tauri-apps/plugin-dialog";
import {
  envCreate,
  envDelete,
  envDuplicate,
  envExportFile,
  envList,
  envRename,
  envSetActive,
  varsGet,
  varsSave,
} from "../ipc/commands";
import type { Environment, Variable, VarScope } from "../types";
import { Icon } from "./Icon";

/**
 * Compact environment control: one small button showing the active
 * environment, opening a popover to switch environments and open the full
 * variables manager. Collapses what used to be an always-visible wide selector
 * plus a separate button.
 */
export function EnvBar() {
  const [envs, setEnvs] = useState<Environment[]>([]);
  const [managerOpen, setManagerOpen] = useState(false);
  const [open, setOpen] = useState(false);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    envList().then(setEnvs);
  }, [version, managerOpen]);

  const active = envs.find((e) => e.is_active);

  const pick = async (id: number | null) => {
    await envSetActive(id);
    setVersion((v) => v + 1);
    setOpen(false);
  };

  return (
    <div className="env-bar">
      <button
        className={`env-trigger${open ? " active" : ""}`}
        title="Environment & variables"
        onMouseDown={(e) => e.stopPropagation()}
        onClick={() => setOpen((v) => !v)}
      >
        <Icon name="braces" size={15} />
        <span className="env-trigger-name">{active?.name ?? "No env"}</span>
        <Icon name="chevron-down" size={12} />
      </button>
      {open && (
        <EnvPopover
          envs={envs}
          activeId={active?.id ?? null}
          onPick={pick}
          onManage={() => {
            setOpen(false);
            setManagerOpen(true);
          }}
          onClose={() => setOpen(false)}
        />
      )}
      {managerOpen && <EnvManager onClose={() => setManagerOpen(false)} />}
    </div>
  );
}

function EnvPopover({
  envs,
  activeId,
  onPick,
  onManage,
  onClose,
}: {
  envs: Environment[];
  activeId: number | null;
  onPick: (id: number | null) => void;
  onManage: () => void;
  onClose: () => void;
}) {
  useEffect(() => {
    const off = () => onClose();
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("mousedown", off);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", off);
      document.removeEventListener("keydown", esc);
    };
  }, [onClose]);

  return (
    <div className="env-popover" onMouseDown={(e) => e.stopPropagation()}>
      <button
        className={`env-opt${activeId === null ? " active" : ""}`}
        onClick={() => onPick(null)}
      >
        No environment
      </button>
      {envs.map((e) => (
        <button
          key={e.id}
          className={`env-opt${activeId === e.id ? " active" : ""}`}
          onClick={() => onPick(e.id)}
        >
          {e.name}
        </button>
      ))}
      <div className="env-popover-sep" />
      <button className="env-opt env-manage" onClick={onManage}>
        <Icon name="braces" size={13} /> Manage variables…
      </button>
    </div>
  );
}

/* ------------------------------------------------------------------ */

function EnvManager({ onClose }: { onClose: () => void }) {
  const [envs, setEnvs] = useState<Environment[]>([]);
  // selection: "globals" or an environment id
  const [selected, setSelected] = useState<"globals" | number>("globals");
  const [version, setVersion] = useState(0);

  useEffect(() => {
    envList().then(setEnvs);
  }, [version]);

  const addEnv = async () => {
    const id = await envCreate("New environment");
    setVersion((v) => v + 1);
    setSelected(id);
  };

  const scope: VarScope = selected === "globals" ? "global" : "environment";
  const ownerId = selected === "globals" ? null : selected;
  const selectedEnv = envs.find((e) => e.id === selected);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal modal-wide" onClick={(e) => e.stopPropagation()}>
        <div className="env-manager">
          <div className="env-list">
            <div
              className={`env-item${selected === "globals" ? " active" : ""}`}
              onClick={() => setSelected("globals")}
            >
              Globals
            </div>
            <div className="env-list-title">Environments</div>
            {envs.map((e) => (
              <div
                key={e.id}
                className={`env-item${selected === e.id ? " active" : ""}`}
                onClick={() => setSelected(e.id)}
              >
                {e.name}
                {e.is_active ? " ●" : ""}
              </div>
            ))}
            <button className="env-add" onClick={() => void addEnv()}>
              + New
            </button>
          </div>
          <div className="env-editor">
            {selectedEnv && (
              <EnvHeader
                key={selectedEnv.id}
                env={selectedEnv}
                onChanged={() => setVersion((v) => v + 1)}
                onDeleted={() => {
                  setSelected("globals");
                  setVersion((v) => v + 1);
                }}
              />
            )}
            {selected === "globals" && (
              <div className="env-header">
                <span className="retention-title">Global variables</span>
              </div>
            )}
            <VarsGrid
              key={`${scope}:${ownerId}`}
              scope={scope}
              ownerId={ownerId}
            />
          </div>
        </div>
        <div className="retention-actions">
          <button className="primary" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

function EnvHeader({
  env,
  onChanged,
  onDeleted,
}: {
  env: Environment;
  onChanged: () => void;
  onDeleted: () => void;
}) {
  // Remounted per environment via key={env.id}, so initial state suffices.
  const [name, setName] = useState(env.name);

  return (
    <div className="env-header">
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        onBlur={async () => {
          if (name.trim() && name !== env.name) {
            await envRename(env.id, name.trim());
            onChanged();
          }
        }}
      />
      {!env.is_active && (
        <button
          onClick={async () => {
            await envSetActive(env.id);
            onChanged();
          }}
        >
          Activate
        </button>
      )}
      <button
        title="Duplicate"
        onClick={async () => {
          await envDuplicate(env.id);
          onChanged();
        }}
      >
        Duplicate
      </button>
      <button
        title="Export as Postman environment"
        onClick={async () => {
          const path = await save({
            defaultPath: `${env.name.replace(/[^\w-]+/g, "_")}.postman_environment.json`,
            filters: [{ name: "Postman Environment", extensions: ["json"] }],
          });
          if (path) await envExportFile(env.id, path);
        }}
      >
        Export
      </button>
      <button
        onClick={async () => {
          if (
            await confirm(`Delete environment "${env.name}"?`, {
              title: "Delete environment",
              kind: "warning",
            })
          ) {
            await envDelete(env.id);
            onDeleted();
          }
        }}
      >
        Delete
      </button>
    </div>
  );
}

/** Editable grid for one variable scope. Saves on every change (debounced). */
export function VarsGrid({
  scope,
  ownerId,
}: {
  scope: VarScope;
  ownerId: number | null;
}) {
  const [vars, setVars] = useState<Variable[]>([]);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    varsGet(scope, ownerId).then((v) => {
      setVars(v);
      setLoaded(true);
    });
  }, [scope, ownerId]);

  useEffect(() => {
    if (!loaded) return;
    const t = setTimeout(() => void varsSave(scope, ownerId, vars), 400);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [vars]);

  const update = (idx: number, patch: Partial<Variable>) => {
    if (idx === vars.length) {
      setVars([
        ...vars,
        {
          key: "",
          initial_value: "",
          current_value: null,
          is_secret: false,
          enabled: true,
          ...patch,
        },
      ]);
    } else {
      setVars(vars.map((v, i) => (i === idx ? { ...v, ...patch } : v)));
    }
  };

  const display: Variable[] = [
    ...vars,
    {
      key: "",
      initial_value: "",
      current_value: null,
      is_secret: false,
      enabled: true,
    },
  ];

  return (
    <div className="vars-grid">
      <div className="vars-head">
        <span />
        <span>Variable</span>
        <span>Initial value</span>
        <span title="Local override — never exported">Current value</span>
        <span title="Secret: masked and excluded from search">🔒</span>
        <span />
      </div>
      {display.map((v, idx) => {
        const isGhost = idx === vars.length;
        return (
          <div className={`vars-row${isGhost ? " kv-ghost" : ""}`} key={idx}>
            <input
              type="checkbox"
              checked={v.enabled}
              disabled={isGhost}
              onChange={(e) => update(idx, { enabled: e.target.checked })}
            />
            <input
              value={v.key}
              placeholder="name"
              onChange={(e) => update(idx, { key: e.target.value })}
            />
            <input
              type={v.is_secret ? "password" : "text"}
              value={v.initial_value}
              placeholder="initial"
              onChange={(e) => update(idx, { initial_value: e.target.value })}
            />
            <input
              type={v.is_secret ? "password" : "text"}
              value={v.current_value ?? ""}
              placeholder="(same as initial)"
              onChange={(e) =>
                update(idx, { current_value: e.target.value || null })
              }
            />
            <input
              type="checkbox"
              checked={v.is_secret}
              disabled={isGhost}
              title="Secret"
              onChange={(e) => update(idx, { is_secret: e.target.checked })}
            />
            <button
              className="kv-remove"
              disabled={isGhost}
              onClick={() => setVars(vars.filter((_, i) => i !== idx))}
            >
              ×
            </button>
          </div>
        );
      })}
    </div>
  );
}
