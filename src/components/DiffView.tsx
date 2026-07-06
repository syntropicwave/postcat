import { useEffect, useState } from "react";
import { diffLines, type Change } from "diff";
import { historyGet } from "../ipc/commands";
import type { HistoryDetail } from "../types";

interface Props {
  ids: [number, number];
  onClose: () => void;
}

/** Side-by-side comparison of two history entries, oldest on the left. */
export function DiffView({ ids, onClose }: Props) {
  const [pair, setPair] = useState<[HistoryDetail, HistoryDetail] | null>(null);

  useEffect(() => {
    const [a, b] = [...ids].sort((x, y) => x - y);
    Promise.all([historyGet(a), historyGet(b)]).then((r) =>
      setPair(r as [HistoryDetail, HistoryDetail]),
    );
  }, [ids]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  if (!pair) return null;
  const [a, b] = pair;

  const sections: { title: string; old: string; new: string }[] = [
    {
      title: "Request",
      old: `${a.method} ${a.url}`,
      new: `${b.method} ${b.url}`,
    },
    {
      title: "Request headers",
      old: headerText(a.req_headers),
      new: headerText(b.req_headers),
    },
    {
      title: "Request body",
      old: pretty(a.req_body_text),
      new: pretty(b.req_body_text),
    },
    {
      title: "Response status",
      old: statusLine(a),
      new: statusLine(b),
    },
    {
      title: "Response headers",
      old: headerText(a.resp_headers),
      new: headerText(b.resp_headers),
    },
    {
      title: "Response body",
      old: pretty(a.resp_body_text),
      new: pretty(b.resp_body_text),
    },
  ];

  return (
    <div className="diff-overlay">
      <div className="diff-header">
        <span>
          Diff: #{a.id} ({shortTime(a.sent_at)}) → #{b.id} (
          {shortTime(b.sent_at)})
        </span>
        <button className="diff-close" onClick={onClose} title="Close (Esc)">
          ×
        </button>
      </div>
      <div className="diff-body">
        {sections.map((s) => {
          if (!s.old && !s.new) return null;
          const changes = diffLines(
            s.old.endsWith("\n") || s.old === "" ? s.old : s.old + "\n",
            s.new.endsWith("\n") || s.new === "" ? s.new : s.new + "\n",
          );
          const changed = changes.some((c) => c.added || c.removed);
          return (
            <section key={s.title} className="diff-section">
              <h3>
                {s.title}
                {!changed && <span className="diff-same"> — identical</span>}
              </h3>
              {changed && <DiffText changes={changes} />}
            </section>
          );
        })}
      </div>
    </div>
  );
}

function DiffText({ changes }: { changes: Change[] }) {
  return (
    <pre className="diff-text">
      {changes.map((c, i) => (
        <span
          key={i}
          className={c.added ? "diff-added" : c.removed ? "diff-removed" : ""}
        >
          {c.value}
        </span>
      ))}
    </pre>
  );
}

function headerText(headers: unknown): string {
  if (!Array.isArray(headers)) return "";
  return (headers as [string, string][])
    .map(([k, v]) => `${k}: ${v}`)
    .sort()
    .join("\n");
}

function statusLine(d: HistoryDetail): string {
  if (d.error) return `ERROR: ${d.error}`;
  return `${d.status ?? ""} ${d.status_text ?? ""} (${d.http_version ?? ""})`;
}

function pretty(text: string | null): string {
  if (!text) return "";
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

function shortTime(iso: string): string {
  return new Date(iso).toLocaleString();
}
