import { contrastText } from "../state/hostAliases";

interface Props {
  alias: string;
  color: string;
  /** Real host, shown as a tooltip. */
  host: string;
}

/** A compact coloured label standing in for a host. */
export function HostChip({ alias, color, host }: Props) {
  const style = color
    ? { background: color, color: contrastText(color) }
    : undefined;
  return (
    <span
      className={`host-chip${color ? "" : " host-chip-default"}`}
      style={style}
      title={host}
    >
      {alias}
    </span>
  );
}
