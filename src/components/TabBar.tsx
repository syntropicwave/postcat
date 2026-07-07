import { useEffect, useState } from "react";
import { useTabs } from "../state/tabs";
import type { Tab } from "../state/tabs";
import {
  matchAlias,
  useHostAliases,
  type AliasMatch,
} from "../state/hostAliases";
import { HostChip } from "./HostChip";
import { UrlDisplay } from "./UrlDisplay";

interface Cell {
  tab: Tab;
  m: AliasMatch | null;
}
interface Group {
  key: number | null;
  match: AliasMatch | null;
  cells: Cell[];
}

export function TabBar() {
  const { tabs, activeTabId, setActive, closeTab, newTab, duplicateTab } =
    useTabs();
  const aliases = useHostAliases((s) => s.aliases);
  const [menu, setMenu] = useState<{
    tabId: string;
    x: number;
    y: number;
  } | null>(null);
  // Only the hovered tab whose title is actually clipped gets a peek overlay.
  const [peekId, setPeekId] = useState<string | null>(null);
  const onTabEnter = (e: React.MouseEvent<HTMLDivElement>, id: string) => {
    const title = e.currentTarget.querySelector<HTMLElement>(".tab-title");
    setPeekId(title && title.scrollWidth > title.clientWidth + 1 ? id : null);
  };

  // Group runs of adjacent tabs that share the same host alias.
  const groups: Group[] = [];
  for (const tab of tabs) {
    const m = matchAlias(tab.url, aliases);
    const key = m?.alias.id ?? null;
    const last = groups[groups.length - 1];
    if (last && key !== null && last.key === key) last.cells.push({ tab, m });
    else groups.push({ key, match: m, cells: [{ tab, m }] });
  }

  const renderTab = ({ tab, m }: Cell, grouped: boolean) => {
    const title = tab.itemName ? (
      tab.itemName
    ) : grouped && m ? (
      tab.url.slice(m.end).replace(/^\//, "") || "/"
    ) : (
      <UrlDisplay url={tab.url} scheme="hide" dropLeadingSlash />
    );
    const dirty = tab.dirty && tab.itemId ? " •" : "";
    return (
      <div
        key={tab.id}
        className={`tab${tab.id === activeTabId ? " active" : ""}${
          m && !grouped ? " tab-aliased" : ""
        }`}
        style={
          m && !grouped
            ? ({
                "--tab-color": m.alias.color || "var(--accent)",
              } as React.CSSProperties)
            : undefined
        }
        onClick={() => setActive(tab.id)}
        onMouseEnter={(e) => onTabEnter(e, tab.id)}
        onMouseLeave={() => setPeekId((p) => (p === tab.id ? null : p))}
        onAuxClick={(e) => {
          if (e.button === 1) {
            e.preventDefault();
            closeTab(tab.id);
          }
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          setMenu({ tabId: tab.id, x: e.clientX, y: e.clientY });
        }}
      >
        <span className={`tab-method method-${tab.method}`}>{tab.method}</span>
        <span className="tab-title">
          {title}
          {dirty}
        </span>
        <button
          className="tab-close"
          title="Close tab"
          onClick={(e) => {
            e.stopPropagation();
            closeTab(tab.id);
          }}
        >
          ×
        </button>
        {/* When the title is clipped, reveal it in full as an overlay that
            grows to the right — no layout shift, only for this tab. */}
        {peekId === tab.id && (
          <div className="tab-peek" aria-hidden="true">
            <span className={`tab-method method-${tab.method}`}>
              {tab.method}
            </span>
            <span className="tab-peek-title">
              {title}
              {dirty}
            </span>
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="tab-bar">
      {groups.map((g, gi) =>
        g.key !== null && g.match && g.cells.length >= 2 ? (
          <div
            key={`g${gi}`}
            className="tab-group"
            style={
              {
                "--group-color": g.match.alias.color || "var(--accent)",
              } as React.CSSProperties
            }
          >
            <span className="tab-group-label" title={g.match.alias.host}>
              <HostChip
                alias={g.match.alias.alias}
                color={g.match.alias.color}
                host={g.match.alias.host}
              />
            </span>
            {g.cells.map((c) => renderTab(c, true))}
          </div>
        ) : (
          g.cells.map((c) => renderTab(c, false))
        ),
      )}
      <button
        className="tab-new"
        title="New request (Ctrl+T)"
        onClick={() => newTab()}
      >
        +
      </button>

      {menu && (
        <TabMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          onDuplicate={() => {
            duplicateTab(menu.tabId);
            setMenu(null);
          }}
          onCloseTab={() => {
            closeTab(menu.tabId);
            setMenu(null);
          }}
          onCloseOthers={() => {
            for (const t of tabs) if (t.id !== menu.tabId) closeTab(t.id);
            setMenu(null);
          }}
        />
      )}
    </div>
  );
}

function TabMenu({
  x,
  y,
  onClose,
  onDuplicate,
  onCloseTab,
  onCloseOthers,
}: {
  x: number;
  y: number;
  onClose: () => void;
  onDuplicate: () => void;
  onCloseTab: () => void;
  onCloseOthers: () => void;
}) {
  useEffect(() => {
    const off = () => onClose();
    document.addEventListener("mousedown", off);
    document.addEventListener("blur", off);
    return () => {
      document.removeEventListener("mousedown", off);
      document.removeEventListener("blur", off);
    };
  }, [onClose]);

  return (
    <div
      className="tab-menu"
      style={{ left: x, top: y }}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <button onClick={onDuplicate}>Duplicate (Ctrl+D)</button>
      <button onClick={onCloseTab}>Close</button>
      <button onClick={onCloseOthers}>Close others</button>
    </div>
  );
}
