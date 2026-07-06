import { useEffect, useState } from "react";
import {
  historyClear,
  historyDelete,
  historyGet,
  historyList,
} from "../ipc/commands";
import { useTabs, parseParams } from "../state/tabs";
import type { HistorySummary } from "../types";
import { formatDuration } from "./ResponseViewer";

const PAGE = 100;

export function HistorySidebar() {
  const historyVersion = useTabs((s) => s.historyVersion);
  const newTab = useTabs((s) => s.newTab);
  const [entries, setEntries] = useState<HistorySummary[]>([]);
  const [query, setQuery] = useState("");
  const [hasMore, setHasMore] = useState(false);

  const loadMore = async (offset: number) => {
    const page = await historyList({
      limit: PAGE,
      offset,
      query: query || undefined,
    });
    setHasMore(page.length === PAGE);
    setEntries((prev) => [...prev, ...page]);
  };

  useEffect(() => {
    let stale = false;
    historyList({ limit: PAGE, offset: 0, query: query || undefined }).then(
      (page) => {
        if (stale) return;
        setHasMore(page.length === PAGE);
        setEntries(page);
      },
    );
    return () => {
      stale = true;
    };
  }, [query, historyVersion]);

  const openEntry = async (id: number) => {
    const detail = await historyGet(id);
    const spec = detail.req_spec;
    newTab({
      method: spec.method,
      url: spec.url,
      params: parseParams(spec.url),
      headers: spec.headers ?? [],
      body: spec.body ?? { kind: "none" },
      settings: spec.settings,
    });
  };

  const groups = groupByDay(entries);

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <input
          className="history-search"
          placeholder="Search history…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button
          className="history-clear"
          title="Clear history"
          onClick={async () => {
            if (window.confirm("Delete ALL history entries?")) {
              await historyClear();
              setEntries([]);
              setHasMore(false);
            }
          }}
        >
          Clear
        </button>
      </div>

      <div className="history-list">
        {groups.map(([day, items]) => (
          <div key={day}>
            <div className="history-day">{day}</div>
            {items.map((e) => (
              <div
                key={e.id}
                className="history-item"
                onClick={() => void openEntry(e.id)}
                title={e.url}
              >
                <span className={`hist-method method-${e.method}`}>
                  {e.method}
                </span>
                <span className="hist-url">{shortUrl(e)}</span>
                <span className={`hist-status ${histStatusClass(e)}`}>
                  {e.error ? "ERR" : (e.status ?? "")}
                </span>
                {e.duration_ms != null && (
                  <span className="hist-time">
                    {formatDuration(e.duration_ms)}
                  </span>
                )}
                <button
                  className="hist-delete"
                  title="Delete entry"
                  onClick={async (ev) => {
                    ev.stopPropagation();
                    await historyDelete(e.id);
                    setEntries((prev) => prev.filter((x) => x.id !== e.id));
                  }}
                >
                  ×
                </button>
              </div>
            ))}
          </div>
        ))}
        {entries.length === 0 && (
          <div className="history-empty">
            {query ? "Nothing found." : "Every request you send lands here."}
          </div>
        )}
        {hasMore && (
          <button
            className="history-more"
            onClick={() => void loadMore(entries.length)}
          >
            Load more
          </button>
        )}
      </div>
    </aside>
  );
}

function shortUrl(e: HistorySummary): string {
  try {
    const u = new URL(e.url);
    return u.pathname + u.search || "/";
  } catch {
    return e.url;
  }
}

function histStatusClass(e: HistorySummary): string {
  if (e.error) return "status-error";
  const s = e.status ?? 0;
  if (s >= 200 && s < 300) return "status-ok";
  if (s >= 300 && s < 400) return "status-redirect";
  if (s >= 400 && s < 500) return "status-client-error";
  return "status-server-error";
}

function groupByDay(entries: HistorySummary[]): [string, HistorySummary[]][] {
  const map = new Map<string, HistorySummary[]>();
  for (const e of entries) {
    const day = formatDay(e.sent_at);
    const list = map.get(day) ?? [];
    list.push(e);
    map.set(day, list);
  }
  return [...map.entries()];
}

function formatDay(sentAt: string): string {
  const d = new Date(sentAt);
  const today = new Date();
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  const sameDay = (a: Date, b: Date) =>
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate();
  if (sameDay(d, today)) return "Today";
  if (sameDay(d, yesterday)) return "Yesterday";
  return d.toLocaleDateString();
}
