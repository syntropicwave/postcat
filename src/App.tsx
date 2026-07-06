import { useEffect } from "react";
import { HistorySidebar } from "./components/HistorySidebar";
import { TabBar } from "./components/TabBar";
import { RequestEditor } from "./components/RequestEditor";
import { ResponseViewer } from "./components/ResponseViewer";
import { useTabs } from "./state/tabs";
import "./App.css";

function App() {
  const { tabs, activeTabId, send, newTab, closeTab } = useTabs();
  const active = tabs.find((t) => t.id === activeTabId) ?? tabs[0];

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      const { activeTabId: id } = useTabs.getState();
      if (e.key === "Enter") {
        e.preventDefault();
        void send(id);
      } else if (e.key.toLowerCase() === "t") {
        e.preventDefault();
        newTab();
      } else if (e.key.toLowerCase() === "w") {
        e.preventDefault();
        closeTab(id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [send, newTab, closeTab]);

  return (
    <div className="app">
      <HistorySidebar />
      <main className="main">
        <TabBar />
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
    </div>
  );
}

export default App;
