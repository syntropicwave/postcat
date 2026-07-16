import { useEffect, useMemo, useState } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import {
  historyClear,
  historyDelete,
  historyEndpoints,
  historyGet,
  historySearch,
  historySetPinned,
} from "../ipc/commands";
import { useTabs, tabFromHistory } from "../state/tabs";
import type { EndpointGroup, HistorySummary, SearchFilters } from "../types";
import { formatDuration } from "./ResponseViewer";
import { RetentionPopover } from "./RetentionPopover";
import { Icon } from "./Icon";
import { UrlDisplay } from "./UrlDisplay";

const PAGE = 100;

interface UiFilters {
  method: string;
  status: string; // "" | "2" | "3" | "4" | "5" | "errors"
  host: string;
  from: string; // yyyy-mm-dd
  to: string;
  pinnedOnly: boolean;
}

const EMPTY_FILTERS: UiFilters = {
  method: "",
  status: "",
  host: "",
  from: "",
  to: "",
  pinnedOnly: false,
};

export function HistorySidebar() {
  const historyVersion = useTabs((s) => s.historyVersion);
  const [entries, setEntries] = useState<HistorySummary[]>([]);
  const [rawQuery, setRawQuery] = useState("");
  const [query, setQuery] = useState("");
  const [ui, setUi] = useState<UiFilters>(EMPTY_FILTERS);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [view, setView] = useState<"timeline" | "endpoints">("timeline");
  const [hasMore, setHasMore] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);

  // Instant-feel search: debounce keystrokes slightly.
  useEffect(() => {
    const t = setTimeout(() => setQuery(rawQuery), 120);
    return () => clearTimeout(t);
  }, [rawQuery]);

  const filters: SearchFilters = useMemo(() => {
    const f: SearchFilters = {};
    if (query.trim()) f.query = query.trim();
    if (ui.method) f.method = ui.method;
    if (ui.status === "errors") f.errors_only = true;
    else if (ui.status) f.status_class = Number(ui.status);
    if (ui.host.trim()) f.host = ui.host.trim();
    if (ui.pinnedOnly) f.pinned_only = true;
    if (ui.from) f.date_from = `${ui.from}T00:00:00`;
    if (ui.to) f.date_to = `${nextDay(ui.to)}T00:00:00`;
    return f;
  }, [query, ui]);

  const activeFilterCount =
    (ui.method ? 1 : 0) +
    (ui.status ? 1 : 0) +
    (ui.host.trim() ? 1 : 0) +
    (ui.from || ui.to ? 1 : 0) +
    (ui.pinnedOnly ? 1 : 0);

  useEffect(() => {
    let stale = false;
    historySearch(filters, { limit: PAGE, offset: 0 }).then((page) => {
      if (stale) return;
      setHasMore(page.length === PAGE);
      setEntries(page);
    });
    return () => {
      stale = true;
    };
  }, [filters, historyVersion]);

  const loadMore = async () => {
    const page = await historySearch(filters, {
      limit: PAGE,
      offset: entries.length,
    });
    setHasMore(page.length === PAGE);
    setEntries((prev) => [...prev, ...page]);
  };

  const patchEntry = (id: number, patch: Partial<HistorySummary>) =>
    setEntries((prev) =>
      prev.map((e) => (e.id === id ? { ...e, ...patch } : e)),
    );

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <div className="history-search-wrap">
          <input
            className="history-search"
            placeholder="Search history — URL, headers, bodies…"
            value={rawQuery}
            onChange={(e) => setRawQuery(e.target.value)}
          />
          {rawQuery && (
            <button
              className="history-search-clear"
              title="Clear search"
              onClick={() => setRawQuery("")}
            >
              <Icon name="x" size={13} />
            </button>
          )}
        </div>
        <button
          className={`icon-btn${filtersOpen || activeFilterCount ? " active" : ""}`}
          title="Filters"
          onClick={() => setFiltersOpen((v) => !v)}
        >
          <Icon name="filter" />
          {activeFilterCount ? (
            <span className="icon-badge">{activeFilterCount}</span>
          ) : null}
        </button>
        <button
          className={`icon-btn${settingsOpen ? " active" : ""}`}
          title="History settings"
          onClick={() => setSettingsOpen((v) => !v)}
        >
          <Icon name="settings" />
        </button>
      </div>

      {settingsOpen && (
        <RetentionPopover onClose={() => setSettingsOpen(false)} />
      )}

      {filtersOpen && (
        <div className="filter-panel">
          <div className="filter-row">
            <select
              value={ui.method}
              onChange={(e) => setUi({ ...ui, method: e.target.value })}
            >
              <option value="">method</option>
              {["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].map(
                (m) => (
                  <option key={m}>{m}</option>
                ),
              )}
            </select>
            <select
              value={ui.status}
              onChange={(e) => setUi({ ...ui, status: e.target.value })}
            >
              <option value="">status</option>
              <option value="2">2xx</option>
              <option value="3">3xx</option>
              <option value="4">4xx</option>
              <option value="5">5xx</option>
              <option value="errors">errors</option>
            </select>
            <input
              className="filter-host"
              placeholder="host"
              value={ui.host}
              onChange={(e) => setUi({ ...ui, host: e.target.value })}
            />
          </div>
          <div className="filter-row">
            <input
              type="date"
              value={ui.from}
              onChange={(e) => setUi({ ...ui, from: e.target.value })}
            />
            <span className="filter-dash">–</span>
            <input
              type="date"
              value={ui.to}
              onChange={(e) => setUi({ ...ui, to: e.target.value })}
            />
            <label className="filter-pinned" title="Pinned only">
              <input
                type="checkbox"
                checked={ui.pinnedOnly}
                onChange={(e) => setUi({ ...ui, pinnedOnly: e.target.checked })}
              />
              <Icon name="star" size={14} />
            </label>
            {activeFilterCount > 0 && (
              <button
                className="filter-reset"
                onClick={() => setUi(EMPTY_FILTERS)}
              >
                reset
              </button>
            )}
          </div>
        </div>
      )}

      <div className="view-toggle">
        <button
          className={view === "timeline" ? "active" : ""}
          onClick={() => setView("timeline")}
        >
          Timeline
        </button>
        <button
          className={view === "endpoints" ? "active" : ""}
          onClick={() => setView("endpoints")}
        >
          Endpoints
        </button>
        <span className="view-spacer" />
        <button
          className="history-clear"
          title="Clear history (pinned entries are kept)"
          onClick={async () => {
            if (
              await confirm("Delete all history entries? Pinned are kept.", {
                title: "Clear history",
                kind: "warning",
              })
            ) {
              await historyClear();
              setEntries((prev) => prev.filter((e) => e.pinned));
            }
          }}
        >
          Clear
        </button>
      </div>

      {view === "timeline" ? (
        <TimelineList
          entries={entries}
          hasMore={hasMore}
          onLoadMore={loadMore}
          onPatch={patchEntry}
          onDelete={(id) =>
            setEntries((prev) => prev.filter((x) => x.id !== id))
          }
          searching={Boolean(filters.query)}
        />
      ) : (
        <EndpointList historyVersion={historyVersion} />
      )}
    </aside>
  );
}

/* ------------------------------------------------------------------ */

interface TimelineProps {
  entries: HistorySummary[];
  hasMore: boolean;
  searching: boolean;
  onLoadMore: () => void;
  onPatch: (id: number, patch: Partial<HistorySummary>) => void;
  onDelete: (id: number) => void;
}

function TimelineList({
  entries,
  hasMore,
  searching,
  onLoadMore,
  onPatch,
  onDelete,
}: TimelineProps) {
  const groups = searching ? null : groupByDay(entries);

  return (
    <div className="history-list">
      {groups
        ? groups.map(([day, items]) => (
            <div key={day}>
              <div className="history-day">{day}</div>
              {items.map((e) => (
                <HistoryItem
                  key={e.id}
                  entry={e}
                  onPatch={onPatch}
                  onDelete={onDelete}
                />
              ))}
            </div>
          ))
        : entries.map((e) => (
            <HistoryItem
              key={e.id}
              entry={e}
              onPatch={onPatch}
              onDelete={onDelete}
            />
          ))}
      {entries.length === 0 && (
        <div className="history-empty">
          {searching
            ? "Nothing found."
            : "Every request you send lands here — searchable forever."}
        </div>
      )}
      {hasMore && (
        <button className="history-more" onClick={onLoadMore}>
          Load more
        </button>
      )}
    </div>
  );
}

function HistoryItem({
  entry: e,
  onPatch,
  onDelete,
}: {
  entry: HistorySummary;
  onPatch: (id: number, patch: Partial<HistorySummary>) => void;
  onDelete: (id: number) => void;
}) {
  const newTab = useTabs((s) => s.newTab);

  const open = async () => {
    const detail = await historyGet(e.id);
    newTab(tabFromHistory(detail));
  };

  return (
    <div className="history-item">
      <div className="history-item-main" onClick={open} title={e.url}>
        <span className={`hist-method method-${e.method}`}>{e.method}</span>
        <span className="hist-url">
          {e.label ? <span className="hist-label">{e.label} </span> : null}
          {shortUrl(e)}
        </span>
        {/* Status + time by default; on hover the controls take their place. */}
        <span className="hist-end">
          <span className="hist-meta">
            <span className={`hist-status ${histStatusClass(e)}`}>
              {e.error ? "ERR" : (e.status ?? "")}
            </span>
            {e.duration_ms != null && (
              <span className="hist-time">{formatDuration(e.duration_ms)}</span>
            )}
          </span>
          <span className="hist-controls">
            <button
              className={e.pinned ? "pinned" : ""}
              title={
                e.pinned ? "Saved (click to unsave)" : "Save (kept forever)"
              }
              onClick={async (ev) => {
                ev.stopPropagation();
                await historySetPinned(e.id, !e.pinned);
                onPatch(e.id, { pinned: !e.pinned });
              }}
            >
              <Icon name={e.pinned ? "star-filled" : "star"} size={15} />
            </button>
            <button
              title="Delete entry"
              onClick={async (ev) => {
                ev.stopPropagation();
                await historyDelete(e.id);
                onDelete(e.id);
              }}
            >
              <Icon name="trash" size={15} />
            </button>
          </span>
        </span>
      </div>

      {e.snippet && <Snippet text={e.snippet} />}
    </div>
  );
}

/** Renders a search snippet, turning [[..]] markers into highlights. */
function Snippet({ text }: { text: string }) {
  const parts = text.split(/\[\[|\]\]/);
  return (
    <div className="hist-snippet">
      {parts.map((p, i) => (i % 2 === 1 ? <mark key={i}>{p}</mark> : p))}
    </div>
  );
}

/* ------------------------------------------------------------------ */

function EndpointList({ historyVersion }: { historyVersion: number }) {
  const [groups, setGroups] = useState<EndpointGroup[]>([]);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [expandedEntries, setExpandedEntries] = useState<HistorySummary[]>([]);
  const newTab = useTabs((s) => s.newTab);

  useEffect(() => {
    let stale = false;
    historyEndpoints().then((g) => {
      if (!stale) setGroups(g);
    });
    return () => {
      stale = true;
    };
  }, [historyVersion]);

  const toggle = async (g: EndpointGroup) => {
    const key = `${g.method} ${g.url_base}`;
    if (expanded === key) {
      setExpanded(null);
      return;
    }
    const entries = await historySearch(
      { endpoint: { method: g.method, url_base: g.url_base } },
      { limit: 50 },
    );
    setExpanded(key);
    setExpandedEntries(entries);
  };

  const openEntry = async (id: number) => {
    const detail = await historyGet(id);
    newTab(tabFromHistory(detail));
  };

  return (
    <div className="history-list">
      {groups.map((g) => {
        const key = `${g.method} ${g.url_base}`;
        const isOpen = expanded === key;
        return (
          <div key={key}>
            <div
              className="endpoint-item"
              onClick={() => void toggle(g)}
              title={g.url_base}
            >
              <span className={`hist-method method-${g.method}`}>
                {g.method}
              </span>
              <UrlDisplay url={g.url_base} scheme="hide" className="hist-url" />
              <span className="endpoint-count">{g.count}</span>
              <span
                className={`hist-status ${endpointStatusClass(g)}`}
                title="Last status"
              >
                {g.last_error ? "ERR" : (g.last_status ?? "")}
              </span>
            </div>
            {isOpen &&
              expandedEntries.map((e) => (
                <div
                  key={e.id}
                  className="history-item endpoint-child"
                  onClick={() => void openEntry(e.id)}
                >
                  <div className="history-item-main">
                    <span className="hist-time">{shortTime(e.sent_at)}</span>
                    <span className={`hist-status ${histStatusClass(e)}`}>
                      {e.error ? "ERR" : (e.status ?? "")}
                    </span>
                    {e.duration_ms != null && (
                      <span className="hist-time">
                        {formatDuration(e.duration_ms)}
                      </span>
                    )}
                    {e.resp_size != null && (
                      <span className="hist-time">{e.resp_size} B</span>
                    )}
                  </div>
                </div>
              ))}
          </div>
        );
      })}
      {groups.length === 0 && (
        <div className="history-empty">No endpoints yet.</div>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */

function nextDay(isoDate: string): string {
  const d = new Date(`${isoDate}T00:00:00`);
  d.setDate(d.getDate() + 1);
  return d.toISOString().slice(0, 10);
}

function shortUrl(e: HistorySummary): string {
  try {
    const u = new URL(e.url);
    return u.pathname + u.search || "/";
  } catch {
    return e.url;
  }
}

function shortTime(iso: string): string {
  return new Date(iso).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}

function histStatusClass(e: HistorySummary): string {
  if (e.error) return "status-error";
  return codeClass(e.status);
}

function endpointStatusClass(g: EndpointGroup): string {
  if (g.last_error) return "status-error";
  return codeClass(g.last_status);
}

function codeClass(status: number | null): string {
  const s = status ?? 0;
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
