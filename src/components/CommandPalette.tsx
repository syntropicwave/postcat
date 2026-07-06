import { useEffect, useMemo, useRef, useState } from "react";
import {
  collectionItems,
  collectionsList,
  historyGet,
  historySearch,
} from "../ipc/commands";
import { useTabs, parseParams } from "../state/tabs";
import type { HistorySummary, RequestSpec } from "../types";

interface Props {
  onClose: () => void;
}

type Item =
  | { kind: "action"; id: string; label: string; run: () => void }
  | { kind: "history"; entry: HistorySummary }
  | { kind: "request"; id: number; name: string; collection: string };

/** Ctrl+K palette: one search box over history (FTS), collections and actions. */
export function CommandPalette({ onClose }: Props) {
  const { newTab } = useTabs();
  const [query, setQuery] = useState("");
  const [history, setHistory] = useState<HistorySummary[]>([]);
  const [requests, setRequests] = useState<
    { id: number; name: string; collection: string; spec: unknown }[]
  >([]);
  const [highlight, setHighlight] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Collections are loaded once; history is searched live.
  useEffect(() => {
    (async () => {
      const cols = await collectionsList();
      const all: typeof requests = [];
      for (const c of cols) {
        const items = await collectionItems(c.id);
        for (const it of items) {
          if (it.kind === "request")
            all.push({
              id: it.id,
              name: it.name,
              collection: c.name,
              spec: it.req_spec,
            });
        }
      }
      setRequests(all);
    })();
  }, []);

  useEffect(() => {
    const t = setTimeout(() => {
      historySearch(query ? { query } : {}, { limit: 12 }).then(setHistory);
    }, 100);
    return () => clearTimeout(t);
  }, [query]);

  const actions: Item[] = useMemo(
    () => [
      {
        kind: "action",
        id: "new",
        label: "New request tab",
        run: () => {
          newTab();
          onClose();
        },
      },
    ],
    [newTab, onClose],
  );

  const items: Item[] = useMemo(() => {
    const q = query.toLowerCase();
    const reqMatches = requests
      .filter(
        (r) =>
          !q ||
          r.name.toLowerCase().includes(q) ||
          r.collection.toLowerCase().includes(q),
      )
      .slice(0, 8)
      .map<Item>((r) => ({
        kind: "request",
        id: r.id,
        name: r.name,
        collection: r.collection,
      }));
    const actMatches = actions.filter(
      (a) => a.kind === "action" && a.label.toLowerCase().includes(q),
    );
    const histItems = history.map<Item>((entry) => ({
      kind: "history",
      entry,
    }));
    return [...actMatches, ...reqMatches, ...histItems];
  }, [query, requests, history, actions]);

  const run = async (item: Item) => {
    if (item.kind === "action") {
      item.run();
      return;
    }
    if (item.kind === "request") {
      const req = requests.find((r) => r.id === item.id);
      const spec = req?.spec as RequestSpec | undefined;
      if (spec) {
        newTab({
          method: spec.method,
          url: spec.url,
          params: parseParams(spec.url),
          headers: spec.headers ?? [],
          body: spec.body ?? { kind: "none" },
          settings: spec.settings,
          auth: spec.auth ?? { kind: "none" },
        });
      }
      onClose();
      return;
    }
    // history entry → open as draft
    const detail = await historyGet(item.entry.id);
    const spec = detail.req_spec;
    newTab({
      method: spec.method,
      url: spec.url,
      params: parseParams(spec.url),
      headers: spec.headers ?? [],
      body: spec.body ?? { kind: "none" },
      settings: spec.settings,
      auth: spec.auth ?? { kind: "none" },
    });
    onClose();
  };

  return (
    <div className="modal-backdrop palette-backdrop" onClick={onClose}>
      <div className="palette" onClick={(e) => e.stopPropagation()}>
        <input
          ref={inputRef}
          className="palette-input"
          placeholder="Search history, collections, actions…"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setHighlight(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setHighlight((h) => Math.min(h + 1, items.length - 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setHighlight((h) => Math.max(h - 1, 0));
            } else if (e.key === "Enter") {
              e.preventDefault();
              if (items[highlight]) void run(items[highlight]);
            } else if (e.key === "Escape") {
              onClose();
            }
          }}
        />
        <div className="palette-list">
          {items.map((item, i) => (
            <div
              key={paletteKey(item, i)}
              className={`palette-item${i === highlight ? " active" : ""}`}
              onMouseEnter={() => setHighlight(i)}
              onClick={() => void run(item)}
            >
              {item.kind === "action" && (
                <>
                  <span className="palette-tag">action</span>
                  <span className="palette-label">{item.label}</span>
                </>
              )}
              {item.kind === "request" && (
                <>
                  <span className="palette-tag">saved</span>
                  <span className="palette-label">{item.name}</span>
                  <span className="palette-sub">{item.collection}</span>
                </>
              )}
              {item.kind === "history" && (
                <>
                  <span className={`hist-method method-${item.entry.method}`}>
                    {item.entry.method}
                  </span>
                  <span className="palette-label">{shortUrl(item.entry)}</span>
                  <span className="palette-sub">
                    {item.entry.error ? "ERR" : item.entry.status}
                  </span>
                </>
              )}
            </div>
          ))}
          {items.length === 0 && (
            <div className="history-empty">Nothing matches.</div>
          )}
        </div>
      </div>
    </div>
  );
}

function paletteKey(item: Item, i: number): string {
  if (item.kind === "history") return `h${item.entry.id}`;
  if (item.kind === "request") return `r${item.id}`;
  return `a${item.id}-${i}`;
}

function shortUrl(e: HistorySummary): string {
  try {
    const u = new URL(e.url);
    return u.host + u.pathname;
  } catch {
    return e.url;
  }
}
