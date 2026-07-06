import { useEffect, useState } from "react";
import { HistorySidebar } from "./components/HistorySidebar";
import { CollectionsPanel } from "./components/CollectionsPanel";
import { TabBar } from "./components/TabBar";
import { EnvBar } from "./components/EnvBar";
import { RequestEditor, saveBoundTab } from "./components/RequestEditor";
import { ResponseViewer } from "./components/ResponseViewer";
import { SaveDialog } from "./components/SaveDialog";
import { useTabs } from "./state/tabs";
import "./App.css";

function App() {
  const { tabs, activeTabId, send, newTab, closeTab } = useTabs();
  const [sidebarTab, setSidebarTab] = useState<"history" | "collections">(
    "history",
  );
  const [saveFor, setSaveFor] = useState<string | null>(null);
  const active = tabs.find((t) => t.id === activeTabId) ?? tabs[0];

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      const { activeTabId: id, tabs } = useTabs.getState();
      const tab = tabs.find((t) => t.id === id);
      if (e.key === "Enter") {
        e.preventDefault();
        void send(id);
      } else if (e.key.toLowerCase() === "t") {
        e.preventDefault();
        newTab();
      } else if (e.key.toLowerCase() === "w") {
        e.preventDefault();
        closeTab(id);
      } else if (e.key.toLowerCase() === "s") {
        e.preventDefault();
        if (tab) {
          void saveBoundTab(tab).then((saved) => {
            if (!saved) setSaveFor(tab.id);
          });
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [send, newTab, closeTab]);

  const saveTab = saveFor ? tabs.find((t) => t.id === saveFor) : undefined;

  return (
    <div className="app">
      <div className="sidebar-wrap">
        <div className="sidebar-tabs">
          <button
            className={sidebarTab === "history" ? "active" : ""}
            onClick={() => setSidebarTab("history")}
          >
            History
          </button>
          <button
            className={sidebarTab === "collections" ? "active" : ""}
            onClick={() => setSidebarTab("collections")}
          >
            Collections
          </button>
        </div>
        {sidebarTab === "history" ? <HistorySidebar /> : <CollectionsPanel />}
      </div>
      <main className="main">
        <div className="top-bar">
          <TabBar />
          <EnvBar />
        </div>
        {active && (
          <div className="workspace">
            <RequestEditor tab={active} />
            <ResponseViewer
              response={active.response}
              error={active.responseError}
              sending={active.sending}
            />
          </div>
        )}
      </main>
      {saveTab && <SaveDialog tab={saveTab} onClose={() => setSaveFor(null)} />}
    </div>
  );
}

export default App;
