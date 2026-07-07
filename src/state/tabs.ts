import { create } from "zustand";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  cancelRequest,
  sendRequest,
  wsClose,
  wsConnect,
} from "../ipc/commands";
import type {
  AuthSpec,
  BodySpec,
  KeyValue,
  RequestSpec,
  SendResult,
  SendSettings,
  WsEvent,
  WsMessage,
} from "../types";
import { DEFAULT_SETTINGS } from "../types";
import { requestDefaults } from "./appSettings";

export interface Tab {
  id: string;
  method: string;
  url: string;
  /// Query params as rows; kept in two-way sync with the url string.
  /// Disabled rows live only here — they are not part of the url.
  params: KeyValue[];
  headers: KeyValue[];
  body: BodySpec;
  settings: SendSettings;
  auth: AuthSpec;
  /// Values for `:name` path segments in the URL, filled on send.
  pathVars: Record<string, string>;
  preRequestScript: string;
  testScript: string;
  sending: boolean;
  response: SendResult | null;
  responseError: string | null;
  /// Live SSE chunks while the request is streaming.
  streamText: string;
  wsStatus: "closed" | "connecting" | "open";
  wsMessages: WsMessage[];
  /// Set when the tab was opened from a collection: variables resolve in
  /// this collection's scope and Ctrl+S saves back into the item.
  collectionId: number | null;
  itemId: number | null;
  itemName: string | null;
  /// Markdown description shown in the Docs tab (saved with the item).
  description: string;
  dirty: boolean;
}

interface TabsState {
  tabs: Tab[];
  activeTabId: string;
  /// Bumped after every send so the history sidebar knows to refetch.
  historyVersion: number;
  /// Bumped after collection mutations so the collections panel refetches.
  collectionsVersion: number;
  bumpCollections: () => void;
  newTab: (partial?: Partial<Tab>) => string;
  duplicateTab: (id: string) => void;
  closeTab: (id: string) => void;
  setActive: (id: string) => void;
  updateTab: (id: string, patch: Partial<Tab>) => void;
  setUrl: (id: string, url: string) => void;
  setParams: (id: string, params: KeyValue[]) => void;
  send: (id: string) => Promise<void>;
  cancel: (id: string) => void;
  wsToggle: (id: string) => Promise<void>;
  wsApplyEvent: (event: WsEvent) => void;
}

export function isWsUrl(url: string): boolean {
  return /^wss?:\/\//i.test(url.trim());
}

let tabCounter = 0;

function makeTab(partial?: Partial<Tab>): Tab {
  tabCounter += 1;
  return {
    id: `tab-${Date.now()}-${tabCounter}`,
    method: "GET",
    url: "",
    params: [],
    headers: [],
    body: { kind: "none" },
    settings: requestDefaults(DEFAULT_SETTINGS),
    auth: { kind: "none" },
    pathVars: {},
    preRequestScript: "",
    testScript: "",
    sending: false,
    response: null,
    responseError: null,
    streamText: "",
    wsStatus: "closed",
    wsMessages: [],
    collectionId: null,
    itemId: null,
    itemName: null,
    description: "",
    dirty: false,
    ...partial,
  };
}

/** Parse the query string of a (possibly partial) URL into rows. */
export function parseParams(url: string): KeyValue[] {
  const qIndex = url.indexOf("?");
  if (qIndex === -1) return [];
  const query = url.slice(qIndex + 1);
  if (!query) return [];
  return query.split("&").map((pair) => {
    const eq = pair.indexOf("=");
    if (eq === -1) return { key: decodeSafe(pair), value: "", enabled: true };
    return {
      key: decodeSafe(pair.slice(0, eq)),
      value: decodeSafe(pair.slice(eq + 1)),
      enabled: true,
    };
  });
}

/** Rebuild a URL from its base and enabled param rows. */
export function buildUrl(url: string, params: KeyValue[]): string {
  const qIndex = url.indexOf("?");
  const base = qIndex === -1 ? url : url.slice(0, qIndex);
  const query = params
    .filter((p) => p.enabled && p.key !== "")
    .map((p) =>
      p.value === ""
        ? encodeSafe(p.key)
        : `${encodeSafe(p.key)}=${encodeSafe(p.value)}`,
    )
    .join("&");
  return query ? `${base}?${query}` : base;
}

function decodeSafe(s: string): string {
  try {
    return decodeURIComponent(s.replace(/\+/g, " "));
  } catch {
    return s;
  }
}

function encodeSafe(s: string): string {
  // Encode only what breaks query parsing; keep URLs human-readable.
  return s.replace(/[&=#\s]/g, (c) => encodeURIComponent(c));
}

export function specFromTab(tab: Tab): RequestSpec {
  return {
    method: tab.method,
    url: applyPathVars(normalizeUrl(tab.url), tab.pathVars),
    headers: tab.headers,
    body: tab.body,
    settings: tab.settings,
    auth: tab.auth,
  };
}

/** Replace `:name` segments with their filled values (unfilled ones stay). */
function applyPathVars(url: string, pathVars: Record<string, string>): string {
  return url.replace(/\/:([A-Za-z0-9_]+)/g, (match, name: string) => {
    const value = pathVars[name];
    return value ? `/${encodeURIComponent(value)}` : match;
  });
}

function normalizeUrl(url: string): string {
  const trimmed = url.trim();
  if (trimmed === "") return trimmed;
  if (!/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(trimmed)) {
    return `http://${trimmed}`;
  }
  return trimmed;
}

export const useTabs = create<TabsState>((set, get) => ({
  tabs: [makeTab()],
  activeTabId: "",
  historyVersion: 0,
  collectionsVersion: 0,
  bumpCollections: () =>
    set((s) => ({ collectionsVersion: s.collectionsVersion + 1 })),

  newTab: (partial) => {
    const tab = makeTab(partial);
    set((s) => ({ tabs: [...s.tabs, tab], activeTabId: tab.id }));
    return tab.id;
  },

  duplicateTab: (id) => {
    set((s) => {
      const idx = s.tabs.findIndex((t) => t.id === id);
      const src = s.tabs[idx];
      if (!src) return s;
      // Clone the request, not the response/session state. Unbind from the
      // saved item so it's a fresh draft; keep the collection scope.
      const copy = makeTab({
        method: src.method,
        url: src.url,
        params: src.params.map((p) => ({ ...p })),
        headers: src.headers.map((h) => ({ ...h })),
        body: structuredClone(src.body),
        settings: { ...src.settings },
        auth: structuredClone(src.auth),
        pathVars: { ...src.pathVars },
        preRequestScript: src.preRequestScript,
        testScript: src.testScript,
        description: src.description,
        collectionId: src.collectionId,
        itemId: null,
        itemName: src.itemName ? `${src.itemName} copy` : null,
        dirty: true,
      });
      // Insert right after the source so same-alias tabs stay adjacent.
      const tabs = [...s.tabs];
      tabs.splice(idx + 1, 0, copy);
      return { tabs, activeTabId: copy.id };
    });
  },

  closeTab: (id) => {
    set((s) => {
      const idx = s.tabs.findIndex((t) => t.id === id);
      const tabs = s.tabs.filter((t) => t.id !== id);
      if (tabs.length === 0) {
        const tab = makeTab();
        return { tabs: [tab], activeTabId: tab.id };
      }
      const activeTabId =
        s.activeTabId === id
          ? tabs[Math.min(idx, tabs.length - 1)].id
          : s.activeTabId;
      return { tabs, activeTabId };
    });
  },

  setActive: (id) => set({ activeTabId: id }),

  updateTab: (id, patch) =>
    set((s) => ({
      tabs: s.tabs.map((t) => (t.id === id ? { ...t, ...patch } : t)),
    })),

  setUrl: (id, url) => {
    const disabled = (get().tabs.find((t) => t.id === id)?.params ?? []).filter(
      (p) => !p.enabled,
    );
    get().updateTab(id, {
      url,
      params: [...parseParams(url), ...disabled],
      dirty: true,
    });
  },

  setParams: (id, params) => {
    const tab = get().tabs.find((t) => t.id === id);
    if (!tab) return;
    get().updateTab(id, {
      params,
      url: buildUrl(tab.url, params),
      dirty: true,
    });
  },

  send: async (id) => {
    const tab = get().tabs.find((t) => t.id === id);
    if (!tab || tab.sending || tab.url.trim() === "") return;
    if (isWsUrl(tab.url)) {
      return get().wsToggle(id);
    }
    get().updateTab(id, { sending: true, responseError: null, streamText: "" });

    // Live SSE chunks stream in while the request is running.
    let unlisten: UnlistenFn | null = null;
    try {
      unlisten = await listen<string>(`stream:${id}`, (event) => {
        set((s) => ({
          tabs: s.tabs.map((t) =>
            t.id === id
              ? { ...t, streamText: t.streamText + event.payload }
              : t,
          ),
        }));
      });
      const result = await sendRequest(
        id,
        specFromTab(tab),
        tab.collectionId,
        tab.itemId,
        tab.preRequestScript || null,
        tab.testScript || null,
      );
      get().updateTab(id, { sending: false, response: result });
    } catch (e) {
      get().updateTab(id, {
        sending: false,
        response: null,
        responseError: String(e),
      });
    } finally {
      unlisten?.();
      set((s) => ({ historyVersion: s.historyVersion + 1 }));
    }
  },

  cancel: (id) => {
    void cancelRequest(id);
  },

  wsToggle: async (id) => {
    const tab = get().tabs.find((t) => t.id === id);
    if (!tab) return;
    if (tab.wsStatus !== "closed") {
      await wsClose(id);
      return;
    }
    get().updateTab(id, { wsStatus: "connecting", wsMessages: [] });
    try {
      await wsConnect(id, tab.url.trim(), tab.headers, tab.collectionId);
    } catch (e) {
      get().updateTab(id, {
        wsStatus: "closed",
        wsMessages: [
          ...(get().tabs.find((t) => t.id === id)?.wsMessages ?? []),
          { kind: "error", text: String(e), ts: Date.now() },
        ],
      });
    }
  },

  wsApplyEvent: (event) => {
    set((s) => ({
      tabs: s.tabs.map((t) => {
        if (t.id !== event.conn_id) return t;
        const message: WsMessage = {
          kind: event.kind,
          text: event.text,
          ts: Date.now(),
        };
        return {
          ...t,
          wsStatus:
            event.kind === "open"
              ? "open"
              : event.kind === "closed"
                ? "closed"
                : t.wsStatus,
          wsMessages: [...t.wsMessages, message],
        };
      }),
      historyVersion:
        event.kind === "closed" ? s.historyVersion + 1 : s.historyVersion,
    }));
  },
}));

// The first tab is created before the store exists; activate it now.
useTabs.setState((s) => ({ activeTabId: s.tabs[0].id }));
