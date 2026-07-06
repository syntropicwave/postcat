import { useTabs } from "../state/tabs";

export function TabBar() {
  const { tabs, activeTabId, setActive, closeTab, newTab } = useTabs();

  return (
    <div className="tab-bar">
      {tabs.map((tab) => (
        <div
          key={tab.id}
          className={`tab${tab.id === activeTabId ? " active" : ""}`}
          onClick={() => setActive(tab.id)}
        >
          <span className={`tab-method method-${tab.method}`}>
            {tab.method}
          </span>
          <span className="tab-title">{tabTitle(tab.url)}</span>
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
        </div>
      ))}
      <button
        className="tab-new"
        title="New request (Ctrl+T)"
        onClick={() => newTab()}
      >
        +
      </button>
    </div>
  );
}

function tabTitle(url: string): string {
  if (!url.trim()) return "New request";
  try {
    const u = new URL(url.includes("://") ? url : `http://${url}`);
    const path = u.pathname === "/" ? "" : u.pathname;
    return `${u.host}${path}` || url;
  } catch {
    return url;
  }
}
