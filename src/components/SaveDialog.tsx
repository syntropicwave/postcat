import { useEffect, useState } from "react";
import {
  collectionCreate,
  collectionItems,
  collectionsList,
  itemCreate,
} from "../ipc/commands";
import { useTabs, specFromTab, type Tab } from "../state/tabs";
import type { Collection, CollectionItem } from "../types";

/** "Save request to collection" — for tabs not yet bound to an item. */
export function SaveDialog({
  tab,
  onClose,
}: {
  tab: Tab;
  onClose: () => void;
}) {
  const { updateTab, bumpCollections } = useTabs();
  const [collections, setCollections] = useState<Collection[]>([]);
  const [collectionId, setCollectionId] = useState<number | "new">("new");
  const [newCollectionName, setNewCollectionName] = useState("My API");
  const [folders, setFolders] = useState<CollectionItem[]>([]);
  const [folderId, setFolderId] = useState<number | null>(null);
  const [name, setName] = useState(suggestName(tab));

  useEffect(() => {
    collectionsList().then((list) => {
      setCollections(list);
      if (list.length > 0) setCollectionId(list[0].id);
    });
  }, []);

  useEffect(() => {
    // Folders are only rendered for a numeric selection; stale state is
    // invisible while "new collection" is chosen.
    if (typeof collectionId !== "number") return;
    let stale = false;
    collectionItems(collectionId).then((items) => {
      if (stale) return;
      setFolders(items.filter((i) => i.kind === "folder"));
      setFolderId(null);
    });
    return () => {
      stale = true;
    };
  }, [collectionId]);

  const doSave = async () => {
    let cid: number;
    if (collectionId === "new") {
      cid = await collectionCreate(newCollectionName.trim() || "My API");
    } else {
      cid = collectionId;
    }
    const itemId = await itemCreate({
      collectionId: cid,
      parentId: folderId,
      kind: "request",
      name: name.trim() || suggestName(tab),
      spec: specFromTab(tab),
    });
    updateTab(tab.id, {
      collectionId: cid,
      itemId,
      itemName: name.trim(),
      dirty: false,
    });
    bumpCollections();
    onClose();
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="retention-title">Save request</div>
        <label className="modal-field">
          Name
          <input
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void doSave();
              if (e.key === "Escape") onClose();
            }}
          />
        </label>
        <label className="modal-field">
          Collection
          <select
            value={collectionId}
            onChange={(e) =>
              setCollectionId(
                e.target.value === "new" ? "new" : Number(e.target.value),
              )
            }
          >
            {collections.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
            <option value="new">+ New collection…</option>
          </select>
        </label>
        {collectionId === "new" && (
          <label className="modal-field">
            New collection name
            <input
              value={newCollectionName}
              onChange={(e) => setNewCollectionName(e.target.value)}
            />
          </label>
        )}
        {typeof collectionId === "number" && folders.length > 0 && (
          <label className="modal-field">
            Folder
            <select
              value={folderId ?? ""}
              onChange={(e) =>
                setFolderId(e.target.value ? Number(e.target.value) : null)
              }
            >
              <option value="">(root)</option>
              {folders.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.name}
                </option>
              ))}
            </select>
          </label>
        )}
        <div className="retention-actions">
          <button onClick={onClose}>Cancel</button>
          <button className="primary" onClick={() => void doSave()}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}

function suggestName(tab: Tab): string {
  try {
    const u = new URL(tab.url.includes("://") ? tab.url : `http://${tab.url}`);
    return `${tab.method} ${u.pathname === "/" ? u.host : u.pathname}`;
  } catch {
    return `${tab.method} request`;
  }
}
