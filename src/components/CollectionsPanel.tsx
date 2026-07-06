import { useEffect, useMemo, useState } from "react";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import {
  collectionCreate,
  collectionDelete,
  collectionItems,
  collectionUpdate,
  collectionsList,
  exportCollectionFile,
  importFile,
  importText,
  itemCreate,
  itemDelete,
  itemDuplicate,
  itemMove,
  itemUpdate,
} from "../ipc/commands";
import { useTabs, parseParams } from "../state/tabs";
import type { Collection, CollectionItem } from "../types";
import { StoredAuthDialog } from "./StoredAuthDialog";
import { ScriptsDialog } from "./ScriptsDialog";
import { RunnerDialog } from "./RunnerDialog";
import { Icon } from "./Icon";

export function CollectionsPanel() {
  const collectionsVersion = useTabs((s) => s.collectionsVersion);
  const bump = useTabs((s) => s.bumpCollections);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [openId, setOpenId] = useState<number | null>(null);
  const [items, setItems] = useState<CollectionItem[]>([]);
  const [importOpen, setImportOpen] = useState(false);
  const [status, setStatus] = useState<string | null>(null);

  useEffect(() => {
    let stale = false;
    collectionsList().then((list) => {
      if (!stale) setCollections(list);
    });
    return () => {
      stale = true;
    };
  }, [collectionsVersion]);

  useEffect(() => {
    // items are only rendered for the open collection, so no need to clear.
    if (openId === null) return;
    let stale = false;
    collectionItems(openId).then((list) => {
      if (!stale) setItems(list);
    });
    return () => {
      stale = true;
    };
  }, [openId, collectionsVersion]);

  const flash = (msg: string) => {
    setStatus(msg);
    setTimeout(() => setStatus(null), 4000);
  };

  const addCollection = async () => {
    const id = await collectionCreate("New collection");
    bump();
    setOpenId(id);
  };

  const doImportFile = async () => {
    const path = await open({
      multiple: false,
      filters: [
        {
          name: "Collections",
          extensions: ["json", "yaml", "yml", "har", "txt"],
        },
      ],
    });
    if (typeof path !== "string") return;
    try {
      const r = await importFile(path);
      bump();
      flash(importSummary(r));
    } catch (e) {
      flash(`Import failed: ${e}`);
    }
  };

  const doExport = async (c: Collection) => {
    const path = await save({
      defaultPath: `${c.name.replace(/[^\w-]+/g, "_")}.postman_collection.json`,
      filters: [{ name: "Postman Collection", extensions: ["json"] }],
    });
    if (!path) return;
    await exportCollectionFile(c.id, path);
    flash(`Exported to ${path.split(/[\\/]/).pop()}`);
  };

  return (
    <div className="collections-panel">
      <div className="collections-toolbar">
        <button onClick={() => void addCollection()}>+ Collection</button>
        <button onClick={() => setImportOpen(true)}>Import</button>
      </div>
      {status && <div className="collections-status">{status}</div>}

      {importOpen && (
        <ImportDialog
          onClose={() => setImportOpen(false)}
          onFile={() => void doImportFile()}
          onDone={(msg) => {
            bump();
            flash(msg);
          }}
        />
      )}

      <div className="collections-list">
        {collections.map((c) => (
          <CollectionNode
            key={c.id}
            collection={c}
            open={openId === c.id}
            items={openId === c.id ? items : []}
            onToggle={() => setOpenId(openId === c.id ? null : c.id)}
            onExport={() => void doExport(c)}
            onChanged={bump}
          />
        ))}
        {collections.length === 0 && (
          <div className="history-empty">
            No collections yet. Save a request with Ctrl+S or import from
            Postman / OpenAPI / cURL / HAR.
          </div>
        )}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */

function CollectionNode({
  collection: c,
  open,
  items,
  onToggle,
  onExport,
  onChanged,
}: {
  collection: Collection;
  open: boolean;
  items: CollectionItem[];
  onToggle: () => void;
  onExport: () => void;
  onChanged: () => void;
}) {
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState(c.name);
  const [authOpen, setAuthOpen] = useState(false);
  const [scriptsOpen, setScriptsOpen] = useState(false);
  const [runnerOpen, setRunnerOpen] = useState(false);

  const rename = async () => {
    if (draft.trim() && draft !== c.name) {
      await collectionUpdate(c.id, { name: draft.trim() });
      onChanged();
    }
    setRenaming(false);
  };

  return (
    <div className="collection-node">
      <div className="collection-row" onClick={onToggle}>
        <span className="tree-caret">
          <Icon name={open ? "chevron-down" : "chevron-right"} size={13} />
        </span>
        {renaming ? (
          <input
            autoFocus
            value={draft}
            onClick={(e) => e.stopPropagation()}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void rename();
              if (e.key === "Escape") setRenaming(false);
            }}
            onBlur={() => void rename()}
          />
        ) : (
          <span className="collection-name">{c.name}</span>
        )}
        <span className="tree-actions" onClick={(e) => e.stopPropagation()}>
          <button
            title="New folder"
            onClick={async () => {
              await itemCreate({
                collectionId: c.id,
                kind: "folder",
                name: "New folder",
              });
              onChanged();
            }}
          >
            <Icon name="folder-plus" />
          </button>
          <button
            title="Rename"
            onClick={() => {
              setDraft(c.name);
              setRenaming(true);
            }}
          >
            <Icon name="pencil" />
          </button>
          <button title="Run collection" onClick={() => setRunnerOpen(true)}>
            <Icon name="play" />
          </button>
          <button title="Collection auth" onClick={() => setAuthOpen(true)}>
            <Icon name="key" />
          </button>
          <button
            title="Collection scripts"
            onClick={() => setScriptsOpen(true)}
          >
            <Icon name="script" />
          </button>
          <button title="Export as Postman v2.1" onClick={onExport}>
            <Icon name="download" />
          </button>
          <button
            title="Delete collection"
            onClick={async () => {
              if (
                await confirm(
                  `Delete collection "${c.name}" and everything in it?`,
                  {
                    title: "Delete collection",
                    kind: "warning",
                  },
                )
              ) {
                await collectionDelete(c.id);
                onChanged();
              }
            }}
          >
            <Icon name="trash" />
          </button>
        </span>
      </div>
      {authOpen && (
        <StoredAuthDialog
          title={`Auth — ${c.name}`}
          target={{ collectionId: c.id }}
          allowInherit={false}
          onClose={() => setAuthOpen(false)}
        />
      )}
      {scriptsOpen && (
        <ScriptsDialog
          title={`Scripts — ${c.name}`}
          target={{ collectionId: c.id }}
          onClose={() => setScriptsOpen(false)}
        />
      )}
      {runnerOpen && (
        <RunnerDialog collection={c} onClose={() => setRunnerOpen(false)} />
      )}
      {open && (
        <Tree
          items={items}
          parentId={null}
          depth={1}
          collectionId={c.id}
          onChanged={onChanged}
        />
      )}
    </div>
  );
}

function Tree({
  items,
  parentId,
  depth,
  collectionId,
  onChanged,
}: {
  items: CollectionItem[];
  parentId: number | null;
  depth: number;
  collectionId: number;
  onChanged: () => void;
}) {
  const children = useMemo(
    () => items.filter((i) => i.parent_id === parentId),
    [items, parentId],
  );
  return (
    <>
      {children.map((item) => (
        <TreeNode
          key={item.id}
          item={item}
          items={items}
          depth={depth}
          collectionId={collectionId}
          onChanged={onChanged}
        />
      ))}
    </>
  );
}

function TreeNode({
  item,
  items,
  depth,
  collectionId,
  onChanged,
}: {
  item: CollectionItem;
  items: CollectionItem[];
  depth: number;
  collectionId: number;
  onChanged: () => void;
}) {
  const newTab = useTabs((s) => s.newTab);
  const [open, setOpen] = useState(true);
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState(item.name);
  const [dragOver, setDragOver] = useState(false);
  const [authOpen, setAuthOpen] = useState(false);
  const [scriptsOpen, setScriptsOpen] = useState(false);

  const openRequest = () => {
    const spec = item.req_spec;
    if (!spec) return;
    newTab({
      method: spec.method,
      url: spec.url,
      params: parseParams(spec.url),
      headers: spec.headers ?? [],
      body: spec.body ?? { kind: "none" },
      settings: spec.settings,
      auth: spec.auth ?? { kind: "none" },
      preRequestScript: item.pre_request_script ?? "",
      testScript: item.test_script ?? "",
      description: item.description ?? "",
      collectionId,
      itemId: item.id,
      itemName: item.name,
    });
  };

  const rename = async () => {
    if (draft.trim() && draft !== item.name) {
      await itemUpdate(item.id, { name: draft.trim() });
      onChanged();
    }
    setRenaming(false);
  };

  const isFolder = item.kind === "folder";

  return (
    <div>
      <div
        className={`tree-row${dragOver ? " drag-over" : ""}`}
        style={{ paddingLeft: 10 + depth * 14 }}
        draggable
        onDragStart={(e) => {
          e.dataTransfer.setData("postcat/item", String(item.id));
          e.dataTransfer.effectAllowed = "move";
        }}
        onDragOver={(e) => {
          if (e.dataTransfer.types.includes("postcat/item")) {
            e.preventDefault();
            setDragOver(true);
          }
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={async (e) => {
          e.preventDefault();
          setDragOver(false);
          const dragged = Number(e.dataTransfer.getData("postcat/item"));
          if (!dragged || dragged === item.id) return;
          // Drop onto a folder = move inside; onto a request = before it.
          try {
            if (isFolder) {
              await itemMove(dragged, item.id, null);
            } else {
              await itemMove(dragged, item.parent_id, item.id);
            }
            onChanged();
          } catch {
            /* cycle move rejected by backend */
          }
        }}
        onClick={() => (isFolder ? setOpen(!open) : openRequest())}
      >
        {isFolder ? (
          <span className="tree-caret">
            <Icon name={open ? "chevron-down" : "chevron-right"} size={13} />
          </span>
        ) : (
          <span
            className={`hist-method method-${item.req_spec?.method ?? "GET"}`}
          >
            {item.req_spec?.method ?? "GET"}
          </span>
        )}
        {renaming ? (
          <input
            autoFocus
            value={draft}
            onClick={(e) => e.stopPropagation()}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void rename();
              if (e.key === "Escape") setRenaming(false);
            }}
            onBlur={() => void rename()}
          />
        ) : (
          <span className="tree-name" title={item.req_spec?.url ?? item.name}>
            {item.name}
          </span>
        )}
        <span className="tree-actions" onClick={(e) => e.stopPropagation()}>
          {isFolder && (
            <>
              <button
                title="New folder inside"
                onClick={async () => {
                  await itemCreate({
                    collectionId,
                    parentId: item.id,
                    kind: "folder",
                    name: "New folder",
                  });
                  onChanged();
                }}
              >
                <Icon name="folder-plus" />
              </button>
              <button title="Folder auth" onClick={() => setAuthOpen(true)}>
                <Icon name="key" />
              </button>
              <button
                title="Folder scripts"
                onClick={() => setScriptsOpen(true)}
              >
                <Icon name="script" />
              </button>
            </>
          )}
          <button
            title="Rename"
            onClick={() => {
              setDraft(item.name);
              setRenaming(true);
            }}
          >
            <Icon name="pencil" />
          </button>
          <button
            title="Duplicate"
            onClick={async () => {
              await itemDuplicate(item.id);
              onChanged();
            }}
          >
            <Icon name="copy" />
          </button>
          <button
            title="Delete"
            onClick={async () => {
              if (
                !isFolder ||
                (await confirm(
                  `Delete folder "${item.name}" and its contents?`,
                  {
                    title: "Delete folder",
                    kind: "warning",
                  },
                ))
              ) {
                await itemDelete(item.id);
                onChanged();
              }
            }}
          >
            <Icon name="trash" />
          </button>
        </span>
      </div>
      {authOpen && (
        <StoredAuthDialog
          title={`Auth — ${item.name}`}
          target={{ itemId: item.id }}
          allowInherit={true}
          onClose={() => setAuthOpen(false)}
        />
      )}
      {scriptsOpen && (
        <ScriptsDialog
          title={`Scripts — ${item.name}`}
          target={{ itemId: item.id }}
          onClose={() => setScriptsOpen(false)}
        />
      )}
      {isFolder && open && (
        <Tree
          items={items}
          parentId={item.id}
          depth={depth + 1}
          collectionId={collectionId}
          onChanged={onChanged}
        />
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */

function ImportDialog({
  onClose,
  onFile,
  onDone,
}: {
  onClose: () => void;
  onFile: () => void;
  onDone: (msg: string) => void;
}) {
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);

  const doImport = async () => {
    try {
      const r = await importText(text);
      onDone(importSummary(r));
      onClose();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="import-dialog">
      <div className="retention-title">Import</div>
      <p className="import-hint">
        Paste a Postman collection/environment (v2.1 JSON), OpenAPI 3 (JSON or
        YAML), HAR, or a cURL command — the format is detected automatically.
      </p>
      <textarea
        autoFocus
        value={text}
        placeholder="Paste here…"
        onChange={(e) => setText(e.target.value)}
      />
      {error && <div className="app-error">{error}</div>}
      <div className="retention-actions">
        <button onClick={onFile}>From file…</button>
        <span style={{ flex: 1 }} />
        <button onClick={onClose}>Cancel</button>
        <button
          className="primary"
          disabled={!text.trim()}
          onClick={() => void doImport()}
        >
          Import
        </button>
      </div>
    </div>
  );
}

function importSummary(r: {
  name: string;
  requests: number;
  folders: number;
  environments: number;
  variables: number;
}): string {
  if (r.environments > 0)
    return `Imported environment "${r.name}" (${r.variables} vars)`;
  return `Imported "${r.name}": ${r.requests} requests, ${r.folders} folders`;
}
