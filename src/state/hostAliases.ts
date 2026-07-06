import { create } from "zustand";
import {
  hostAliasesList,
  hostAliasUpsert,
  hostAliasDelete,
} from "../ipc/commands";
import type { HostAlias } from "../types";

interface State {
  aliases: HostAlias[];
  /** host (lowercased) -> alias row */
  byHost: Record<string, HostAlias>;
  load: () => Promise<void>;
  upsert: (host: string, alias: string, color: string) => Promise<void>;
  remove: (id: number) => Promise<void>;
}

function index(aliases: HostAlias[]): Record<string, HostAlias> {
  const m: Record<string, HostAlias> = {};
  for (const a of aliases) m[a.host.toLowerCase()] = a;
  return m;
}

export const useHostAliases = create<State>((set, get) => ({
  aliases: [],
  byHost: {},
  load: async () => {
    const aliases = await hostAliasesList();
    set({ aliases, byHost: index(aliases) });
  },
  upsert: async (host, alias, color) => {
    await hostAliasUpsert(host, alias, color);
    await get().load();
  },
  remove: async (id) => {
    await hostAliasDelete(id);
    await get().load();
  },
}));

/** Look up an alias for a host string, case-insensitively. Non-reactive. */
export function aliasForHost(host: string | null): HostAlias | undefined {
  if (!host) return undefined;
  return useHostAliases.getState().byHost[host.toLowerCase()];
}

/** Reactive variant: re-renders the caller when the alias set changes. */
export function useAliasForHost(host: string | null): HostAlias | undefined {
  return useHostAliases((s) =>
    host ? s.byHost[host.toLowerCase()] : undefined,
  );
}

/**
 * The host (hostname + port) of a URL, or null if it has none yet. Tolerates
 * scheme-less input ("api.example.com/x") the way the address bar accepts it.
 */
export function hostOf(url: string): string | null {
  const s = url.trim();
  if (!s) return null;
  try {
    const u = new URL(s.includes("://") ? s : `http://${s}`);
    return u.host || null;
  } catch {
    return null;
  }
}

/**
 * Split a URL into the parts around its host so callers can render the host
 * distinctly. `host` is null when the URL has no recognisable host yet.
 */
export function splitUrl(url: string): {
  pre: string;
  host: string | null;
  post: string;
} {
  const host = hostOf(url);
  if (!host) return { pre: url, host: null, post: "" };
  const i = url.toLowerCase().indexOf(host.toLowerCase());
  if (i < 0) return { pre: url, host: null, post: "" };
  return {
    pre: url.slice(0, i),
    host: url.slice(i, i + host.length),
    post: url.slice(i + host.length),
  };
}

/** Readable text colour (black/white) for a given hex background. */
export function contrastText(hex: string): string {
  const h = hex.replace("#", "");
  if (h.length < 6) return "#ffffff";
  const r = parseInt(h.slice(0, 2), 16);
  const g = parseInt(h.slice(2, 4), 16);
  const b = parseInt(h.slice(4, 6), 16);
  // Perceived luminance (sRGB approximation).
  const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  return lum > 0.6 ? "#1b1b1f" : "#ffffff";
}

/** Preset palette offered when creating/editing an alias. */
export const ALIAS_COLORS = [
  "#7c5cff",
  "#1a9a55",
  "#d97706",
  "#d13438",
  "#0ea5e9",
  "#db2777",
  "#0d9488",
  "#6b7280",
];
