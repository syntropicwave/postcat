import { useMemo } from "react";
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

/** Exact lookup for an alias by its key (the stored prefix), reactive. */
export function useAliasByKey(key: string): HostAlias | undefined {
  return useHostAliases((s) => (key ? s.byHost[key.toLowerCase()] : undefined));
}

export interface AliasMatch {
  alias: HostAlias;
  start: number;
  end: number;
}

/**
 * The alias to show for a URL: the saved prefix that appears earliest in the
 * URL, longest first. Aliases are arbitrary substrings (origin, or origin +
 * a path chunk), so this compacts "https://api.example.com/v1/…" down to a
 * single chip.
 */
export function matchAlias(
  url: string,
  aliases: HostAlias[],
): AliasMatch | null {
  const lower = url.toLowerCase();
  let best: AliasMatch | null = null;
  for (const a of aliases) {
    const key = a.host.toLowerCase();
    if (!key) continue;
    const idx = lower.indexOf(key);
    if (idx < 0) continue;
    const end = idx + key.length;
    if (!best || idx < best.start || (idx === best.start && end > best.end)) {
      best = { alias: a, start: idx, end };
    }
  }
  return best;
}

/** Reactive alias match for a URL. */
export function useUrlMatch(url: string): AliasMatch | null {
  const aliases = useHostAliases((s) => s.aliases);
  return useMemo(() => matchAlias(url, aliases), [url, aliases]);
}

/**
 * The origin of a URL — scheme + host (+ port) — or null if it has none yet.
 * Aliases are keyed on the origin so "https://x" and "http://x" are distinct.
 * Scheme-less input ("api.example.com/x"), which the address bar accepts, is
 * keyed on the bare host.
 */
export function originOf(url: string): string | null {
  const s = url.trim();
  if (!s) return null;
  try {
    if (s.includes("://")) {
      const u = new URL(s);
      return u.host ? `${u.protocol}//${u.host}`.toLowerCase() : null;
    }
    const u = new URL(`http://${s}`);
    return u.host ? u.host.toLowerCase() : null;
  } catch {
    return null;
  }
}

/** The character range of a URL's origin, for highlighting/aliasing it. */
export function originRange(
  url: string,
): { start: number; end: number } | null {
  const o = originOf(url);
  if (!o) return null;
  const i = url.toLowerCase().indexOf(o.toLowerCase());
  if (i < 0) return null;
  return { start: i, end: i + o.length };
}

/** Schemes offered at the end of the address-bar suggestions. */
export const URL_SCHEMES = ["https://", "http://", "wss://", "ws://"];

export interface UrlSuggestion {
  kind: "alias" | "scheme";
  /** Text inserted when chosen. */
  key: string;
  alias?: HostAlias;
}

/**
 * Address-bar autocomplete: saved alias prefixes first (shortest/base first),
 * then bare schemes — all filtered to those that begin with what's typed.
 */
export function buildUrlSuggestions(
  value: string,
  aliases: HostAlias[],
): UrlSuggestion[] {
  const v = value.toLowerCase();
  const out: UrlSuggestion[] = aliases
    .filter(
      (a) => a.host.toLowerCase().startsWith(v) && a.host.toLowerCase() !== v,
    )
    .sort((a, b) => a.host.length - b.host.length)
    .map((a) => ({ kind: "alias" as const, key: a.host, alias: a }));
  for (const s of URL_SCHEMES) {
    if (s.startsWith(v) && s !== v) out.push({ kind: "scheme", key: s });
  }
  return out;
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

/**
 * Preset palette offered when creating/editing an alias. Muted pastels —
 * calm but still distinguishable, and light enough to carry dark text on
 * either theme.
 */
export const ALIAS_COLORS = [
  "#b3a3e0", // violet
  "#8fc9a4", // green
  "#e8c07f", // amber
  "#e79a8f", // coral
  "#87b8e2", // blue
  "#e0a0cb", // pink
  "#83ccc1", // teal
  "#aeb8c7", // slate
];
