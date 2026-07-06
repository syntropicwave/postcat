import { create } from "zustand";
import { cancelRequest, sendRequest } from "../ipc/commands";
import type {
  BodySpec,
  KeyValue,
  RequestSpec,
  SendResult,
  SendSettings,
} from "../types";
import { DEFAULT_SETTINGS } from "../types";

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
  sending: boolean;
  response: SendResult | null;
  responseError: string | null;
  /// Set when the tab was opened from a collection: variables resolve in
  /// this collection's scope and Ctrl+S saves back into the item.
  collectionId: number | null;
  itemId: number | null;
  itemName: string | null;
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
  closeTab: (id: string) => void;
  setActive: (id: string) => void;
  updateTab: (id: string, patch: Partial<Tab>) => void;
  setUrl: (id: string, url: string) => void;
  setParams: (id: string, params: KeyValue[]) => void;
  send: (id: string) => Promise<void>;
  cancel: (id: string) => void;
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
    settings: { ...DEFAULT_SETTINGS },
    sending: false,
    response: null,
    responseError: null,
    collectionId: null,
    itemId: null,
    itemName: null,
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
    url: normalizeUrl(tab.url),
    headers: tab.headers,
    body: tab.body,
    settings: tab.settings,
  };
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
    get().updateTab(id, { sending: true, responseError: null });
    try {
      const result = await sendRequest(id, specFromTab(tab), tab.collectionId);
      get().updateTab(id, { sending: false, response: result });
    } catch (e) {
      get().updateTab(id, {
        sending: false,
        response: null,
        responseError: String(e),
      });
    } finally {
      set((s) => ({ historyVersion: s.historyVersion + 1 }));
    }
  },

  cancel: (id) => {
    void cancelRequest(id);
  },
}));

// The first tab is created before the store exists; activate it now.
useTabs.setState((s) => ({ activeTabId: s.tabs[0].id }));
