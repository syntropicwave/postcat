import { splitUrl, useAliasForHost } from "../state/hostAliases";
import { HostChip } from "./HostChip";

interface Props {
  url: string;
  /**
   * How to treat a non-aliased URL: "hide"/"dim" drop the "https://" prefix
   * for compactness, "show" keeps the full URL. When aliased, the whole
   * origin (scheme + host) collapses into the chip regardless.
   */
  scheme?: "hide" | "dim" | "show";
  className?: string;
}

/**
 * Renders a URL with its origin (scheme + host) collapsed to a coloured chip
 * when an alias exists. Read-only — used in history, tabs and the (blurred)
 * address bar.
 */
export function UrlDisplay({ url, scheme = "dim", className }: Props) {
  const { host, post } = splitUrl(url);
  const alias = useAliasForHost(host);

  if (!host) return <span className={className}>{url || "New request"}</span>;

  if (!alias) {
    const text =
      scheme === "show" ? url : host.replace(/^[a-z0-9+.-]+:\/\//i, "") + post;
    return <span className={className}>{text}</span>;
  }

  return (
    <span className={className}>
      <HostChip alias={alias.alias} color={alias.color} host={host} />
      {post}
    </span>
  );
}
