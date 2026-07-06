import { useState } from "react";
import type { Timings } from "../types";

/** Colored stacked waterfall of the request phases with an expandable legend. */
export function TimingBar({ timings }: { timings: Timings }) {
  const [open, setOpen] = useState(false);

  const phases: { key: string; label: string; ms: number | null }[] = [
    { key: "dns", label: "DNS", ms: timings.dns_ms },
    { key: "connect", label: "TCP", ms: timings.connect_ms },
    { key: "tls", label: "TLS", ms: timings.tls_ms },
    { key: "server", label: "Server", ms: timings.server_ms },
    { key: "download", label: "Download", ms: timings.download_ms },
  ];
  const present = phases.filter((p) => p.ms != null && p.ms > 0);
  const total = present.reduce((sum, p) => sum + (p.ms ?? 0), 0) || 1;
  // Connection phases are None on reqwest fallback → partial breakdown.
  const partial = timings.dns_ms == null && timings.connect_ms == null;

  return (
    <span className="timing-bar-wrap">
      <button
        className="timing-toggle"
        title="Timing breakdown"
        onClick={() => setOpen((v) => !v)}
      >
        <span className="timing-bar">
          {present.map((p) => (
            <span
              key={p.key}
              className={`timing-seg timing-${p.key}`}
              style={{ width: `${(((p.ms ?? 0) / total) * 100).toFixed(1)}%` }}
            />
          ))}
        </span>
      </button>
      {open && (
        <div className="timing-legend">
          {partial && (
            <div className="timing-note">
              Connection phases merged (proxy/fallback path).
            </div>
          )}
          {timings.redirects > 0 && (
            <div className="timing-note">
              {timings.redirects} redirect
              {timings.redirects > 1 ? "s" : ""} — phases summed across hops.
            </div>
          )}
          {present.map((p) => (
            <div key={p.key} className="timing-legend-row">
              <span className={`timing-dot timing-${p.key}`} />
              <span className="timing-legend-label">{p.label}</span>
              <span className="timing-legend-ms">{fmt(p.ms ?? 0)}</span>
            </div>
          ))}
          <div className="timing-legend-row timing-total">
            <span className="timing-legend-label">Total</span>
            <span className="timing-legend-ms">{fmt(timings.total_ms)}</span>
          </div>
        </div>
      )}
    </span>
  );
}

function fmt(ms: number): string {
  if (ms < 1) return `${(ms * 1000).toFixed(0)} µs`;
  if (ms < 1000) return `${ms.toFixed(1)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}
