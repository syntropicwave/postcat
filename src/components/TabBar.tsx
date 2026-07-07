import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
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
  method: string;
  title: ReactNode;
  dirty: string;
  left: number;
  top: number;
  width: number;
  height: number;
  background: string;
  boxShadow: string;
}

// Minimum shrunk tab width; also the unit for the overflow fit calculation.
const MIN_TAB = 90;

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
  // The right edge is a real border-right on .tab-peek; here we only carry the
  // decoration bars (group top bar / active underline).
  return { background, boxShadow: shadows.join(", ") || "none" };
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

  // Overflow: tabs shrink via CSS; when even shrunk they don't fit, the oldest
  // collapse and a chevron on the left opens a searchable list of all tabs.
  const barRef = useRef<HTMLDivElement>(null);
  const [capacity, setCapacity] = useState(999);
  const [list, setList] = useState<{ x: number; y: number } | null>(null);

  const count = tabs.length;
  const activeIndex = tabs.findIndex((t) => t.id === activeTabId);
  const shown = Math.max(1, Math.min(capacity, count));
  // Show a contiguous window ending at the newest tab, shifted left if needed
  // to keep the active tab visible.
  let start = Math.max(0, count - shown);
  if (activeIndex >= 0 && activeIndex < start) start = activeIndex;
  start = Math.min(start, Math.max(0, count - shown));
  const visibleTabs = tabs.slice(start, start + shown);
  const overflowed = shown < count;

  // Fit as many tabs as possible. Runs synchronously before paint.
  //   shrink: the tabs, crammed to their min, still overflow the bar.
  //   grow:   the visible tabs (which fill the bar via flex-grow) carry more
  //           than a whole extra tab's width above their minimum, so another
  //           hidden tab would fit. A margin avoids flip-flopping at the edge.
  useLayoutEffect(() => {
    const bar = barRef.current;
    if (!bar) return;
    const over = bar.scrollWidth - bar.clientWidth;
    if (over > 1) {
      if (shown > 1)
        setCapacity(
          Math.max(1, shown - Math.max(1, Math.ceil(over / MIN_TAB))),
        );
    } else if (shown < count) {
      let tabsWidth = 0;
      const els = bar.querySelectorAll<HTMLElement>(".tab");
      els.forEach((t) => {
        tabsWidth += t.offsetWidth;
      });
      const slack = tabsWidth - els.length * MIN_TAB;
      // Reserve a min tab PLUS a possible new group label per added tab, so a
      // grow can never overshoot into an overflow that the shrink branch would
      // then undo — that fight is an infinite render loop. Add only as many as
      // definitely fit even in the worst (all-new-labels) case.
      const GROW_UNIT = MIN_TAB + 50;
      if (slack >= GROW_UNIT)
        setCapacity(shown + Math.floor(slack / GROW_UNIT));
    }
  }, [shown, count]);

  // A window resize changes the available width; re-expand and let the layout
  // effect settle to the new fit.
  useEffect(() => {
    const onResize = () => setCapacity(999);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  // Group runs of adjacent VISIBLE tabs that share the same host alias.
  const groups: Group[] = [];
  for (const tab of visibleTabs) {
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
      method: tab.method,
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
    <div className="tab-bar" ref={barRef}>
      {overflowed && (
        <button
          className={`tab-overflow${list ? " active" : ""}`}
          title="All tabs"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            setList(list ? null : { x: r.left, y: r.bottom });
          }}
        >
          <Icon name="chevron-down" size={16} />
          <span className="tab-overflow-count">{count}</span>
        </button>
      )}
      {groups.map((g) => {
        const groupColor = g.match?.alias.color || "var(--accent)";
        const gkey = g.cells[0].tab.id;
        return g.key !== null && g.match && g.cells.length >= 2 ? (
          <div
            key={gkey}
            className="tab-group"
            style={
              {
                "--group-color": groupColor,
                // Grow proportionally to how many tabs it holds so grouped and
                // standalone tabs end up roughly the same width.
                flex: `${g.cells.length} 1 0`,
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

      {list && (
        <TabList
          x={list.x}
          y={list.y}
          tabs={tabs}
          activeTabId={activeTabId}
          onPick={(id) => {
            setActive(id);
            setList(null);
          }}
          onCloseTab={(id) => closeTab(id)}
          onClose={() => setList(null)}
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
            {/* Mirror the tab's lead exactly (method badge reserves the width,
                × sits over it) so the title lands in the same place and doesn't
                jump when the overlay appears. The overlay body is click-through;
                only this × is interactive (highlights + closes). */}
            <span className="tab-lead">
              <span className={`tab-method method-${peek.method}`}>
                {peek.method}
              </span>
              <button
                className="tab-x"
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
            </span>
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

/** Searchable list of ALL tabs (opened from the overflow chevron). */
function TabList({
  x,
  y,
  tabs,
  activeTabId,
  onPick,
  onCloseTab,
  onClose,
}: {
  x: number;
  y: number;
  tabs: Tab[];
  activeTabId: string;
  onPick: (id: string) => void;
  onCloseTab: (id: string) => void;
  onClose: () => void;
}) {
  const [q, setQ] = useState("");
  useEffect(() => {
    const off = () => onClose();
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("mousedown", off);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", off);
      document.removeEventListener("keydown", esc);
    };
  }, [onClose]);

  const needle = q.trim().toLowerCase();
  const rows = tabs.filter(
    (t) =>
      !needle ||
      (t.itemName || t.url).toLowerCase().includes(needle) ||
      t.method.toLowerCase().includes(needle),
  );

  return createPortal(
    <div
      className="tab-list"
      style={{ left: x, top: y }}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <input
        className="tab-list-search"
        autoFocus
        spellCheck={false}
        placeholder="Search tabs"
        value={q}
        onChange={(e) => setQ(e.target.value)}
      />
      <div className="tab-list-items">
        {rows.length === 0 && <div className="tab-list-empty">No tabs</div>}
        {rows.map((t) => (
          <div
            key={t.id}
            className={`tab-list-item${t.id === activeTabId ? " active" : ""}`}
            onClick={() => onPick(t.id)}
          >
            <span className={`tab-method method-${t.method}`}>{t.method}</span>
            <span className="tab-list-title">
              {t.itemName || (
                <UrlDisplay url={t.url} scheme="hide" dropLeadingSlash />
              )}
            </span>
            <button
              className="tab-list-x"
              title="Close tab"
              onClick={(e) => {
                e.stopPropagation();
                onCloseTab(t.id);
              }}
            >
              <Icon name="x" size={14} />
            </button>
          </div>
        ))}
      </div>
    </div>,
    document.body,
  );
}
