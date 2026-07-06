import type { ReactElement } from "react";

/**
 * One consistent line-icon set (16px, currentColor, 1.6 stroke) replacing the
 * mixed emoji/glyph buttons. Paths are hand-drawn in a 24×24 grid in the
 * common line-icon idiom.
 */
export type IconName =
  | "key"
  | "script"
  | "play"
  | "cookie"
  | "settings"
  | "sync"
  | "braces"
  | "code"
  | "star"
  | "star-filled"
  | "pencil"
  | "copy"
  | "diff"
  | "download"
  | "upload"
  | "folder-plus"
  | "filter"
  | "check"
  | "x"
  | "plus"
  | "trash"
  | "chevron-right"
  | "chevron-down"
  | "clock"
  | "search"
  | "arrow-in"
  | "arrow-out"
  | "extract"
  | "wrap"
  | "save"
  | "refresh";

const PATHS: Record<IconName, ReactElement> = {
  key: (
    <>
      <circle cx="7.5" cy="15.5" r="4" />
      <path d="M10.5 12.5 20 3M17 6l2 2M14 9l2 2" />
    </>
  ),
  script: (
    <>
      <path d="M8 3h9a2 2 0 0 1 2 2v12a2 2 0 0 0 2 2H8a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z" />
      <path d="M5 19a2 2 0 0 0 2 2M10 8h6M10 12h6M10 16h3" />
    </>
  ),
  play: <path d="M7 4.5v15l12-7.5-12-7.5Z" />,
  cookie: (
    <>
      <path d="M12 3a9 9 0 1 0 9 9 4 4 0 0 1-4-4 4 4 0 0 1-4-4 .9.9 0 0 0-1-1Z" />
      <path d="M8.5 10h.01M12 14h.01M15.5 11h.01M9 15h.01" />
    </>
  ),
  settings: (
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M19 12a7 7 0 0 0-.1-1l2-1.5-2-3.4-2.3 1a7 7 0 0 0-1.7-1l-.4-2.6h-4l-.4 2.6a7 7 0 0 0-1.7 1l-2.3-1-2 3.4 2 1.5a7 7 0 0 0 0 2l-2 1.5 2 3.4 2.3-1a7 7 0 0 0 1.7 1l.4 2.6h4l.4-2.6a7 7 0 0 0 1.7-1l2.3 1 2-3.4-2-1.5A7 7 0 0 0 19 12Z" />
    </>
  ),
  sync: (
    <>
      <path d="M21 12a9 9 0 0 1-15 6.7L3 16" />
      <path d="M3 12a9 9 0 0 1 15-6.7L21 8" />
      <path d="M21 3v5h-5M3 21v-5h5" />
    </>
  ),
  refresh: (
    <>
      <path d="M21 12a9 9 0 1 1-2.6-6.4L21 8" />
      <path d="M21 3v5h-5" />
    </>
  ),
  braces: (
    <path d="M8 3c-2 0-2 2-2 3.5S6 10 4 10c2 0 2 2 2 3.5S6 17 8 17M16 3c2 0 2 2 2 3.5S18 10 20 10c-2 0-2 2-2 3.5S18 17 16 17" />
  ),
  code: <path d="m9 8-5 4 5 4M15 8l5 4-5 4M13 5l-2 14" />,
  star: (
    <path d="m12 3 2.6 5.6L20 9.3l-4 4 1 5.7-5-2.8-5 2.8 1-5.7-4-4 5.4-.7L12 3Z" />
  ),
  "star-filled": (
    <path
      d="m12 3 2.6 5.6L20 9.3l-4 4 1 5.7-5-2.8-5 2.8 1-5.7-4-4 5.4-.7L12 3Z"
      fill="currentColor"
      stroke="none"
    />
  ),
  pencil: <path d="M4 20h4L19 9a2 2 0 0 0-3-3L5 17v3ZM14 7l3 3" />,
  copy: (
    <>
      <rect x="9" y="9" width="11" height="11" rx="2" />
      <path d="M6 15H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v1" />
    </>
  ),
  diff: <path d="M4 8h11M4 8l3-3M4 8l3 3M20 16H9m11 0-3-3m3 3-3 3" />,
  download: <path d="M12 3v12m0 0 4-4m-4 4-4-4M4 19h16" />,
  upload: <path d="M12 21V9m0 0 4 4m-4-4-4 4M4 5h16" />,
  "folder-plus": (
    <>
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z" />
      <path d="M12 11v4M10 13h4" />
    </>
  ),
  filter: <path d="M3 5h18l-7 8v6l-4-2v-4L3 5Z" />,
  check: <path d="m4 12 5 5L20 6" />,
  x: <path d="M6 6l12 12M18 6 6 18" />,
  plus: <path d="M12 5v14M5 12h14" />,
  trash: <path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v6M14 11v6" />,
  "chevron-right": <path d="m9 6 6 6-6 6" />,
  "chevron-down": <path d="m6 9 6 6 6-6" />,
  clock: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 2" />
    </>
  ),
  search: (
    <>
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.5-3.5" />
    </>
  ),
  "arrow-in": <path d="M20 12H8m0 0 4-4m-4 4 4 4M4 5v14" />,
  "arrow-out": <path d="M4 12h12m0 0-4-4m4 4-4 4M20 5v14" />,
  extract: <path d="M4 7h10M4 12h7M4 17h10M20 8v8m0 0 3-3m-3 3-3-3" />,
  wrap: <path d="M4 6h16M4 12h13a3 3 0 1 1 0 6h-3m0 0 2-2m-2 2 2 2M4 18h5" />,
  save: (
    <>
      <path d="M5 3h11l3 3v13a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z" />
      <path d="M8 3v5h7M8 21v-6h8v6" />
    </>
  ),
};

interface Props {
  name: IconName;
  size?: number;
  className?: string;
}

export function Icon({ name, size = 16, className }: Props) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {PATHS[name]}
    </svg>
  );
}
