import { useEffect, useState } from "react";
import { retentionGet, retentionSet } from "../ipc/commands";

/** History retention settings. Defaults keep everything forever. */
export function RetentionPopover({ onClose }: { onClose: () => void }) {
  const [ageDays, setAgeDays] = useState(0);
  const [maxEntries, setMaxEntries] = useState(0);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    retentionGet().then((s) => {
      setAgeDays(s.max_age_days);
      setMaxEntries(s.max_entries);
      setLoaded(true);
    });
  }, []);

  const save = async () => {
    await retentionSet({
      max_age_days: Math.max(0, ageDays),
      max_entries: Math.max(0, maxEntries),
    });
    onClose();
  };

  if (!loaded) return null;

  return (
    <div className="retention-popover">
      <div className="retention-title">History retention</div>
      <label>
        Keep entries for
        <input
          type="number"
          min={0}
          value={ageDays}
          onChange={(e) => setAgeDays(Number(e.target.value))}
        />
        days (0 = forever)
      </label>
      <label>
        Keep at most
        <input
          type="number"
          min={0}
          value={maxEntries}
          onChange={(e) => setMaxEntries(Number(e.target.value))}
        />
        entries (0 = unlimited)
      </label>
      <div className="retention-note">★ Pinned entries are never deleted.</div>
      <div className="retention-actions">
        <button onClick={onClose}>Cancel</button>
        <button className="primary" onClick={() => void save()}>
          Save
        </button>
      </div>
    </div>
  );
}
