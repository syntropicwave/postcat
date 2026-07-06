import { splitUrl, useAliasForHost } from "../state/hostAliases";
import { HostChip } from "./HostChip";

interface Props {
  url: string;
  /** How to treat the "https://" prefix: hide it, dim it, or show plainly. */
  scheme?: "hide" | "dim" | "show";
  className?: string;
}

/**
 * Renders a URL with its host collapsed to a coloured chip when an alias
 * exists. Read-only — used in history, tabs and the (blurred) address bar.
 */
export function UrlDisplay({ url, scheme = "dim", className }: Props) {
  const { pre, host, post } = splitUrl(url);
  const alias = useAliasForHost(host);

  if (!host) return <span className={className}>{url || "New request"}</span>;

  if (!alias) {
    const text = scheme === "show" ? url : host + post;
    return <span className={className}>{text}</span>;
  }

  return (
    <span className={className}>
      {scheme === "dim" && pre && <span className="url-scheme">{pre}</span>}
      {scheme === "show" && pre}
      <HostChip alias={alias.alias} color={alias.color} host={host} />
      {post}
    </span>
  );
}
