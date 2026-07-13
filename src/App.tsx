import { useEffect, useRef, useState } from "react";
import { HistorySidebar } from "./components/HistorySidebar";
import { CollectionsPanel } from "./components/CollectionsPanel";
import { TabBar } from "./components/TabBar";
import { EnvBar } from "./components/EnvBar";
import { RequestEditor, saveBoundTab } from "./components/RequestEditor";
import { ResponseViewer } from "./components/ResponseViewer";
import { SaveDialog } from "./components/SaveDialog";
import { CookieManager } from "./components/CookieManager";
import { SettingsDialog } from "./components/SettingsDialog";
import { HostsDialog } from "./components/HostsDialog";
import { SyncDialog } from "./components/SyncDialog";
import { WsPanel } from "./components/WsPanel";
import { CommandPalette } from "./components/CommandPalette";
import { DiffView } from "./components/DiffView";
import { Icon } from "./components/Icon";
import { ResizeHandle } from "./components/ResizeHandle";
import { UpdateBanner } from "./components/UpdateBanner";
import { useUpdater, autoUpdateEnabled } from "./state/updater";
import { usePersistentState } from "./hooks/usePersistentState";
import { useAppSettings } from "./state/appSettings";
import { useHostAliases } from "./state/hostAliases";
import { useTabs, isWsUrl } from "./state/tabs";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
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
  const [hostsOpen, setHostsOpen] = useState(false);
  const [syncOpen, setSyncOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [diffPair, setDiffPair] = useState<[number, number] | null>(null);
  const active = tabs.find((t) => t.id === activeTabId) ?? tabs[0];

  const layout = useAppSettings((s) => s.settings?.response_layout ?? "bottom");
  const loadSettings = useAppSettings((s) => s.load);
  const loadHostAliases = useHostAliases((s) => s.load);
  const horizontal = layout === "right";

  // Resizable panels (persisted). Double-click a divider to reset.
  const [sidebarWidth, setSidebarWidth] = usePersistentState(
    "ui.sidebarWidth",
    320,
  );
  const [reqHeight, setReqHeight] = usePersistentState<number | null>(
    "ui.reqHeight",
    null,
  );
  const [reqWidth, setReqWidth] = usePersistentState<number | null>(
    "ui.reqWidth",
    null,
  );
  // "Focus response": collapse the request params to just the address bar and
  // give the response the freed vertical space. Only actually collapses when
  // there's response content, so it can never hide the params with no way back.
  const [responseFocus, setResponseFocus] = usePersistentState(
    "ui.responseFocus",
    false,
  );
  const workspaceRef = useRef<HTMLDivElement>(null);

  const clamp = (v: number, lo: number, hi: number) =>
    Math.min(Math.max(v, lo), hi);

  useEffect(() => {
    void loadSettings();
    void loadHostAliases();
  }, [loadSettings, loadHostAliases]);

  // Auto-check for updates on launch (opt-out in Settings).
  useEffect(() => {
    if (autoUpdateEnabled()) void useUpdater.getState().runCheck();
  }, []);

  // Mirror the active request's address into the OS window title (taskbar,
  // alt-tab), the way Postman does.
  useEffect(() => {
    const addr = active?.url.trim();
    const title = addr
      ? `${active.method} ${addr}`
      : active?.itemName || "postcat";
    void getCurrentWindow().setTitle(title);
  }, [active?.method, active?.url, active?.itemName]);

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
      } else if (e.key.toLowerCase() === "d") {
        e.preventDefault();
        useTabs.getState().duplicateTab(id);
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

  const onSidebarDelta = (dx: number) =>
    setSidebarWidth((w) => clamp(w + dx, 220, 640));

  const onReqDelta = (delta: number) => {
    const ws = workspaceRef.current;
    if (!ws) return;
    const reqEl = ws.querySelector<HTMLElement>(".request-editor");
    const rect = reqEl?.getBoundingClientRect();
    if (horizontal) {
      const base = reqWidth ?? rect?.width ?? 480;
      const maxW = ws.getBoundingClientRect().width - 360;
      setReqWidth(clamp(base + delta, 320, Math.max(320, maxW)));
    } else {
      const base = reqHeight ?? rect?.height ?? 320;
      const maxH = ws.getBoundingClientRect().height - 160;
      setReqHeight(clamp(base + delta, 160, Math.max(160, maxH)));
    }
  };

  const saveTab = saveFor ? tabs.find((t) => t.id === saveFor) : undefined;

  return (
    <div className="app">
      <div className="titlebar">
        <TabBar />
        <div className="top-actions">
          <button
            className="icon-btn"
            title="Sync (end-to-end encrypted)"
            onClick={() => setSyncOpen(true)}
          >
            <Icon name="sync" />
          </button>
          <button
            className="icon-btn"
            title="Host aliases"
            onClick={() => setHostsOpen(true)}
          >
            <Icon name="tag" />
          </button>
          <button
            className="icon-btn"
            title="Cookies"
            onClick={() => setCookiesOpen(true)}
          >
            <Icon name="cookie" />
          </button>
          <button
            className="icon-btn"
            title="Settings"
            onClick={() => setSettingsOpen(true)}
          >
            <Icon name="settings" />
          </button>
        </div>
      </div>
      <div className="app-body">
        <div className="sidebar-wrap" style={{ width: sidebarWidth }}>
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
          <EnvBar />
        </div>
        <ResizeHandle
          axis="x"
          onDelta={onSidebarDelta}
          onReset={() => setSidebarWidth(320)}
        />
        <main className="main">
          {active &&
            (() => {
              const hasResponseContent =
                !isWsUrl(active.url) &&
                !!(
                  active.response ||
                  active.responseError ||
                  active.sending ||
                  active.streamText
                );
              return (
                <div
                  className={`workspace ${horizontal ? "horizontal" : ""}${
                    responseFocus && hasResponseContent ? " response-focus" : ""
                  }`}
                  ref={workspaceRef}
                  style={
                    {
                      ...(reqHeight != null
                        ? { "--req-h": `${reqHeight}px` }
                        : {}),
                      ...(reqWidth != null
                        ? { "--req-w": `${reqWidth}px` }
                        : {}),
                    } as React.CSSProperties
                  }
                >
                  <RequestEditor tab={active} />
                  <ResizeHandle
                    axis={horizontal ? "x" : "y"}
                    onDelta={onReqDelta}
                    onReset={() =>
                      horizontal ? setReqWidth(null) : setReqHeight(null)
                    }
                  />
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
                      focus={responseFocus}
                      onToggleFocus={() => setResponseFocus((v) => !v)}
                    />
                  )}
                </div>
              );
            })()}
        </main>
      </div>
      {saveTab && <SaveDialog tab={saveTab} onClose={() => setSaveFor(null)} />}
      {cookiesOpen && <CookieManager onClose={() => setCookiesOpen(false)} />}
      {hostsOpen && <HostsDialog onClose={() => setHostsOpen(false)} />}
      {settingsOpen && (
        <SettingsDialog onClose={() => setSettingsOpen(false)} />
      )}
      {syncOpen && <SyncDialog onClose={() => setSyncOpen(false)} />}
      {paletteOpen && <CommandPalette onClose={() => setPaletteOpen(false)} />}
      {diffPair && (
        <DiffView ids={diffPair} onClose={() => setDiffPair(null)} />
      )}
      <UpdateBanner />
    </div>
  );
}

export default App;
