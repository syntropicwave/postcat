import { useEffect, useState } from "react";
import { authStoredGet, authStoredSet } from "../ipc/commands";
import type { AuthSpec } from "../types";
import { AuthEditor } from "./AuthEditor";

interface Props {
  title: string;
  target: { collectionId?: number; itemId?: number };
  /** Folders may inherit from the collection; the collection root may not. */
  allowInherit: boolean;
  onClose: () => void;
}

/** Edit the auth stored on a collection or folder (inherited by children). */
export function StoredAuthDialog({
  title,
  target,
  allowInherit,
  onClose,
}: Props) {
  const [auth, setAuth] = useState<AuthSpec | null>(null);

  useEffect(() => {
    authStoredGet(target).then(setAuth);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target.collectionId, target.itemId]);

  if (!auth) return null;

  const save = async () => {
    await authStoredSet(target, auth);
    onClose();
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="retention-title">{title}</div>
        <p className="import-hint">
          Requests inside inherit this auth when their own Auth tab is set to
          “Inherit from parent”.
        </p>
        <AuthEditor
          auth={auth}
          onChange={setAuth}
          allowInherit={allowInherit}
        />
        <div className="retention-actions">
          <button onClick={onClose}>Cancel</button>
          <button className="primary" onClick={() => void save()}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
