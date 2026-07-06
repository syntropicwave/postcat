import { useEffect, useState } from "react";
import {
  collectionScriptsGet,
  collectionScriptsSet,
  itemScriptsGet,
  itemScriptsSet,
} from "../ipc/commands";
import { ScriptsEditor } from "./ScriptsEditor";

interface Props {
  title: string;
  target: { collectionId: number } | { itemId: number };
  onClose: () => void;
}

/** Edit collection/folder scripts — they run for every request inside. */
export function ScriptsDialog({ title, target, onClose }: Props) {
  const [pre, setPre] = useState<string | null>(null);
  const [test, setTest] = useState<string | null>(null);

  useEffect(() => {
    const load =
      "collectionId" in target
        ? collectionScriptsGet(target.collectionId)
        : itemScriptsGet(target.itemId);
    load.then(([p, t]) => {
      setPre(p ?? "");
      setTest(t ?? "");
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (pre === null || test === null) return null;

  const save = async () => {
    if ("collectionId" in target) {
      await collectionScriptsSet(
        target.collectionId,
        pre || null,
        test || null,
      );
    } else {
      await itemScriptsSet(target.itemId, pre || null, test || null);
    }
    onClose();
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal modal-wide" onClick={(e) => e.stopPropagation()}>
        <div className="retention-title">{title}</div>
        <p className="import-hint">
          These scripts run for every request inside (collection scripts first,
          then folder scripts, then the request&apos;s own).
        </p>
        <div className="scripts-dialog-editor">
          <ScriptsEditor
            preRequestScript={pre}
            testScript={test}
            onChange={(p, t) => {
              setPre(p);
              setTest(t);
            }}
          />
        </div>
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
