import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useTabs } from "../state/tabs";
import type { Tab } from "../state/tabs";
import {
  matchAlias,
  useHostAliases,
  type AliasMatch,
} from "../state/hostAliases";
import { HostChip } from "./HostChip";
import { UrlDisplay } from "./UrlDisplay";
import { Icon } from "./Icon";

interface Cell {
  tab: Tab;
  m: AliasMatch | null;
}
interface Group {
  key: number | null;
  match: AliasMatch | null;
  cells: Cell[];
}

// The hover overlay for a clipped tab. Rendered in a portal (fixed, on the
// body) so it escapes the tab-bar's overflow clip and can grow past it, and so
// it never redraws the tabs' own separators.
interface Peek {
  id: string;
  title: ReactNode;
  dirty: string;
  left: number;
  top: number;
  width: number;
  height: number;
  background: string;
  boxShadow: string;
}

// Reproduce the tab's own look (background + decoration bars) as inline styles,
// since the portal is detached from the .tab-group / .active ancestors that the
// CSS would otherwise match. CSS vars still resolve against :root. A thin border
// (not a drop shadow) delimits the overlay from the tabs underneath.
function tabDeco(
  active: boolean,
  grouped: boolean,
  groupColor: string,
  aliasColor: string | null,
): { background: string; boxShadow: string } {
  const background = grouped
    ? active
      ? `color-mix(in srgb, ${groupColor} 8%, var(--bg))`
      : `color-mix(in srgb, ${groupColor} 10%, var(--bg-panel))`
    : active
      ? "var(--bg)"
      : "var(--bg-panel)";
  const topColor = grouped ? groupColor : aliasColor;
  const shadows: string[] = [];
  if (topColor) shadows.push(`inset 0 2px 0 ${topColor}`);
  if (active) shadows.push("inset 0 -2px 0 var(--accent)");
  // Just a right edge to cap the overlay — no drop shadow. Top/bottom align
  // with the bar and the left edge sits on the real tab separator.
  shadows.push("inset -1px 0 0 var(--border)");
  return { background, boxShadow: shadows.join(", ") };
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
  const [peek, setPeek] = useState<Peek | null>(null);
  // Hover-bridge: the overlay body is click-through, but its × is interactive.
  // Moving from the tab onto the × briefly "leaves" the tab, so defer clearing
  // the overlay and cancel that if the × (or the tab again) is entered.
  const clearTimer = useRef<number | null>(null);
  const cancelClear = () => {
    if (clearTimer.current !== null) {
      clearTimeout(clearTimer.current);
      clearTimer.current = null;
    }
  };
  const scheduleClear = () => {
    cancelClear();
    clearTimer.current = window.setTimeout(() => setPeek(null), 90);
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

  const tabTitle = (
    tab: Tab,
    m: AliasMatch | null,
    grouped: boolean,
  ): ReactNode =>
    tab.itemName ? (
      tab.itemName
    ) : grouped && m ? (
      tab.url.slice(m.end).replace(/^\//, "") || "/"
    ) : (
      <UrlDisplay url={tab.url} scheme="hide" dropLeadingSlash />
    );

  const onTabEnter = (
    e: React.MouseEvent<HTMLDivElement>,
    tab: Tab,
    m: AliasMatch | null,
    grouped: boolean,
    groupColor: string,
  ) => {
    const el = e.currentTarget;
    const title = el.querySelector<HTMLElement>(".tab-title");
    if (!title || title.scrollWidth <= title.clientWidth + 1) {
      setPeek(null);
      return;
    }
    const r = el.getBoundingClientRect();
    const active = tab.id === activeTabId;
    const aliasColor = !grouped && m ? m.alias.color || "var(--accent)" : null;
    setPeek({
      id: tab.id,
      title: tabTitle(tab, m, grouped),
      dirty: tab.dirty && tab.itemId ? " •" : "",
      left: r.left,
      top: r.top,
      width: r.width,
      height: r.height,
      ...tabDeco(active, grouped, groupColor, aliasColor),
    });
  };

  const renderTab = (
    { tab, m }: Cell,
    grouped: boolean,
    groupColor: string,
  ) => {
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
        onMouseEnter={(e) => {
          cancelClear();
          onTabEnter(e, tab, m, grouped, groupColor);
        }}
        onMouseLeave={scheduleClear}
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
        {/* The method badge reserves the lead slot; on hover the close button
            fills it (no layout shift). Removing the always-on × frees space. */}
        <span className="tab-lead">
          <span className={`tab-method method-${tab.method}`}>
            {tab.method}
          </span>
          <button
            className="tab-x"
            title="Close tab"
            onClick={(e) => {
              e.stopPropagation();
              closeTab(tab.id);
            }}
          >
            <Icon name="x" size={20} />
          </button>
        </span>
        <span className="tab-title">
          {tabTitle(tab, m, grouped)}
          {dirty}
        </span>
      </div>
    );
  };

  return (
    <div
      className="tab-bar"
      onScroll={() => {
        cancelClear();
        setPeek(null);
      }}
    >
      {groups.map((g, gi) => {
        const groupColor = g.match?.alias.color || "var(--accent)";
        return g.key !== null && g.match && g.cells.length >= 2 ? (
          <div
            key={`g${gi}`}
            className="tab-group"
            style={{ "--group-color": groupColor } as React.CSSProperties}
          >
            <span className="tab-group-label" title={g.match.alias.host}>
              <HostChip
                alias={g.match.alias.alias}
                color={g.match.alias.color}
                host={g.match.alias.host}
              />
            </span>
            {g.cells.map((c) => renderTab(c, true, groupColor))}
          </div>
        ) : (
          g.cells.map((c) => renderTab(c, false, groupColor))
        );
      })}
      <button
        className="tab-new"
        title="New request (Ctrl+T)"
        onClick={() => newTab()}
      >
        <Icon name="plus" size={17} />
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

      {peek &&
        createPortal(
          <div
            className="tab-peek"
            style={{
              left: peek.left,
              top: peek.top,
              height: peek.height,
              minWidth: peek.width,
              background: peek.background,
              boxShadow: peek.boxShadow,
            }}
          >
            {/* The overlay body is click-through (navigation passes to the tabs
                underneath); only this × is interactive, so it highlights and
                closes directly. The hover-bridge keeps the overlay alive while
                the pointer is on it. */}
            <button
              className="tab-peek-x"
              title="Close tab"
              onMouseEnter={cancelClear}
              onMouseLeave={scheduleClear}
              onClick={(e) => {
                e.stopPropagation();
                cancelClear();
                closeTab(peek.id);
                setPeek(null);
              }}
            >
              <Icon name="x" size={20} />
            </button>
            <span className="tab-peek-title">
              {peek.title}
              {peek.dirty}
            </span>
          </div>,
          document.body,
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
