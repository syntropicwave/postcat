import { useTabs } from "../state/tabs";
import { UrlDisplay } from "./UrlDisplay";

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
          <span className="tab-title">
            {tab.itemName ? (
              tab.itemName
            ) : (
              <UrlDisplay url={tab.url} scheme="hide" />
            )}
            {tab.dirty && tab.itemId ? " •" : ""}
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
