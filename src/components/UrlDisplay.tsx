import { useUrlMatch } from "../state/hostAliases";
import { HostChip } from "./HostChip";

interface Props {
  url: string;
  /**
   * How to treat text before the chip / a non-aliased URL: "hide"/"dim" drop
   * the "https://" prefix for compactness, "show" keeps it. When an alias
   * matches, the matched prefix collapses into the chip regardless.
   */
  scheme?: "hide" | "dim" | "show";
  className?: string;
}

const SCHEME_RE = /^[a-z0-9+.-]+:\/\//i;

/**
 * Renders a URL with its matched alias prefix (origin, or origin + a path
 * chunk) collapsed to a coloured chip. Read-only — used in history, tabs and
 * the (blurred) address bar.
 */
export function UrlDisplay({ url, scheme = "dim", className }: Props) {
  const m = useUrlMatch(url);

  if (!m) {
    if (!url) return <span className={className}>New request</span>;
    const text = scheme === "show" ? url : url.replace(SCHEME_RE, "");
    return <span className={className}>{text}</span>;
  }

  const pre = url.slice(0, m.start);
  const post = url.slice(m.end);
  const preText = scheme === "hide" ? pre.replace(SCHEME_RE, "") : pre;

  return (
    <span className={className}>
      {preText && <span className="url-scheme">{preText}</span>}
      <HostChip
        alias={m.alias.alias}
        color={m.alias.color}
        host={m.alias.host}
      />
      {post}
    </span>
  );
}
