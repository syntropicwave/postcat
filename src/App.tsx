import { useEffect, useState } from "react";
import { HistorySidebar } from "./components/HistorySidebar";
import { CollectionsPanel } from "./components/CollectionsPanel";
import { TabBar } from "./components/TabBar";
import { EnvBar } from "./components/EnvBar";
import { RequestEditor, saveBoundTab } from "./components/RequestEditor";
import { ResponseViewer } from "./components/ResponseViewer";
import { SaveDialog } from "./components/SaveDialog";
import { CookieManager } from "./components/CookieManager";
import { SettingsDialog } from "./components/SettingsDialog";
import { SyncDialog } from "./components/SyncDialog";
import { WsPanel } from "./components/WsPanel";
import { CommandPalette } from "./components/CommandPalette";
import { DiffView } from "./components/DiffView";
import { useTabs, isWsUrl } from "./state/tabs";
import { listen } from "@tauri-apps/api/event";
import { historySearch } from "./ipc/commands";
import type { WsEvent } from "./types";
import "./App.css";

function App() {
  const { tabs, activeTabId, send, newTab, closeTab } = useTabs();
  const [sidebarTab, setSidebarTab] = useState<"history" | "collections">(
    "history",
  );
  const [saveFor, setSaveFor] = useState<string | null>(null);
  const [cookiesOpen, setCookiesOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [syncOpen, setSyncOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [diffPair, setDiffPair] = useState<[number, number] | null>(null);
  const active = tabs.find((t) => t.id === activeTabId) ?? tabs[0];

  // "Diff vs previous": find the prior response for this endpoint.
  const diffPrevious = active?.response
    ? async () => {
        const method = active.method;
        const urlBase = active.url.split("?")[0];
        const prior = await historySearch(
          { endpoint: { method, url_base: urlBase } },
          { limit: 5 },
        );
        const currentId = active.response?.history_id;
        const previous = prior.find((e) => e.id !== currentId);
        if (currentId && previous) setDiffPair([previous.id, currentId]);
      }
    : undefined;

  // Global WebSocket event stream → per-tab message logs.
  useEffect(() => {
    const unlisten = listen<WsEvent>("ws:event", (event) => {
      useTabs.getState().wsApplyEvent(event.payload);
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

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
      } else if (e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((v) => !v);
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
          <div className="top-actions">
            <button
              className="icon-btn"
              title="Sync (end-to-end encrypted)"
              onClick={() => setSyncOpen(true)}
            >
              ⟳
            </button>
            <button
              className="icon-btn"
              title="Cookies"
              onClick={() => setCookiesOpen(true)}
            >
              🍪
            </button>
            <button
              className="icon-btn"
              title="Settings (proxy, certificates)"
              onClick={() => setSettingsOpen(true)}
            >
              ⚙
            </button>
          </div>
        </div>
        {active && (
          <div className="workspace">
            <RequestEditor tab={active} />
            {isWsUrl(active.url) ? (
              <WsPanel tab={active} />
            ) : (
              <ResponseViewer
                response={active.response}
                error={active.responseError}
                sending={active.sending}
                streamText={active.streamText}
                collectionId={active.collectionId}
                onDiffPrevious={diffPrevious}
              />
            )}
          </div>
        )}
      </main>
      {saveTab && <SaveDialog tab={saveTab} onClose={() => setSaveFor(null)} />}
      {cookiesOpen && <CookieManager onClose={() => setCookiesOpen(false)} />}
      {settingsOpen && (
        <SettingsDialog onClose={() => setSettingsOpen(false)} />
      )}
      {syncOpen && <SyncDialog onClose={() => setSyncOpen(false)} />}
      {paletteOpen && <CommandPalette onClose={() => setPaletteOpen(false)} />}
      {diffPair && (
        <DiffView ids={diffPair} onClose={() => setDiffPair(null)} />
      )}
    </div>
  );
}

export default App;
