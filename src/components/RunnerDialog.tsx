import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { runCollection, runnerCancel } from "../ipc/commands";
import { useTabs } from "../state/tabs";
import type { Collection, RequestRunResult, RunReport } from "../types";
import { formatDuration } from "./ResponseViewer";

interface Props {
  collection: Collection;
  onClose: () => void;
}

export function RunnerDialog({ collection, onClose }: Props) {
  const bump = useTabs((s) => s.bumpCollections);
  const [iterations, setIterations] = useState(1);
  const [delay, setDelay] = useState(0);
  const [dataRows, setDataRows] = useState<unknown[] | null>(null);
  const [dataName, setDataName] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<RequestRunResult[]>([]);
  const [report, setReport] = useState<RunReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!running) return;
    const unlisten = listen<RequestRunResult>("runner:progress", (event) => {
      setProgress((prev) => [...prev, event.payload]);
      requestAnimationFrame(() => {
        listRef.current?.scrollTo({ top: listRef.current.scrollHeight });
      });
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [running]);

  const pickData = async () => {
    const path = await open({
      multiple: false,
      filters: [{ name: "Data", extensions: ["json", "csv"] }],
    });
    if (typeof path !== "string") return;
    try {
      const text = await readTextFile(path);
      const rows = path.toLowerCase().endsWith(".csv")
        ? parseCsv(text)
        : (JSON.parse(text) as unknown[]);
      if (!Array.isArray(rows) || rows.length === 0)
        throw new Error("expected a non-empty array of rows");
      setDataRows(rows);
      setDataName(`${path.split(/[\\/]/).pop()} (${rows.length} rows)`);
      if (iterations < rows.length) setIterations(rows.length);
    } catch (e) {
      setError(`Data file: ${e}`);
    }
  };

  const start = async () => {
    setRunning(true);
    setProgress([]);
    setReport(null);
    setError(null);
    try {
      const result = await runCollection({
        collection_id: collection.id,
        iterations,
        delay_ms: delay,
        data: dataRows,
      });
      setReport(result);
      bump(); // history changed
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={running ? undefined : onClose}>
      <div className="modal modal-wide" onClick={(e) => e.stopPropagation()}>
        <div className="code-toolbar">
          <span className="retention-title">Run — {collection.name}</span>
          <span style={{ flex: 1 }} />
          {running ? (
            <button onClick={() => void runnerCancel(collection.id)}>
              Cancel run
            </button>
          ) : (
            <button onClick={onClose}>Close</button>
          )}
        </div>

        <div className="runner-options">
          <label>
            Iterations
            <input
              type="number"
              min={1}
              disabled={running}
              value={iterations}
              onChange={(e) =>
                setIterations(Math.max(1, Number(e.target.value)))
              }
            />
          </label>
          <label>
            Delay (ms)
            <input
              type="number"
              min={0}
              disabled={running}
              value={delay}
              onChange={(e) => setDelay(Math.max(0, Number(e.target.value)))}
            />
          </label>
          <button disabled={running} onClick={() => void pickData()}>
            {dataName ?? "Data file…"}
          </button>
          {dataRows && (
            <button
              disabled={running}
              title="Remove data file"
              onClick={() => {
                setDataRows(null);
                setDataName(null);
              }}
            >
              ×
            </button>
          )}
          <span style={{ flex: 1 }} />
          <button
            className="primary"
            disabled={running}
            onClick={() => void start()}
          >
            {running ? "Running…" : "Run"}
          </button>
        </div>

        {error && <div className="app-error">{error}</div>}

        <div className="runner-progress" ref={listRef}>
          {progress.map((r, i) => (
            <div key={i} className="runner-row">
              <span className="hist-time">#{r.iteration + 1}</span>
              <span className={`hist-method method-${r.method}`}>
                {r.method}
              </span>
              <span className="runner-name" title={r.url}>
                {r.name}
              </span>
              <span
                className={`hist-status ${
                  r.error
                    ? "status-error"
                    : (r.status ?? 0) < 400
                      ? "status-ok"
                      : "status-client-error"
                }`}
              >
                {r.skipped ? "SKIP" : r.error ? "ERR" : r.status}
              </span>
              <span className="hist-time">{formatDuration(r.duration_ms)}</span>
              <span
                className={
                  r.tests.some((t) => !t.passed)
                    ? "tests-failed"
                    : "tests-passed"
                }
              >
                {r.tests.length > 0
                  ? `${r.tests.filter((t) => t.passed).length}/${r.tests.length}`
                  : ""}
              </span>
            </div>
          ))}
          {progress
            .flatMap((r) => r.tests.filter((t) => !t.passed))
            .map((t, i) => (
              <div key={`f${i}`} className="test-row fail">
                <span className="test-mark">✗</span>
                <span className="test-name">{t.name}</span>
                <span className="test-error">{t.error}</span>
              </div>
            ))}
        </div>

        {report && (
          <div
            className={`runner-summary ${
              report.failed_tests > 0 || report.errors > 0 ? "fail" : "pass"
            }`}
          >
            {report.total_requests} requests · {report.passed_tests} tests
            passed · {report.failed_tests} failed · {report.errors} errors ·{" "}
            {(report.duration_ms / 1000).toFixed(1)}s
            {report.cancelled ? " · cancelled" : ""}
          </div>
        )}
      </div>
    </div>
  );
}

/** Read a text file through the backend (no fs plugin needed). */
async function readTextFile(path: string): Promise<string> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<string>("read_text_file", { path });
}

/** Tiny CSV parser: header row + comma-separated values, quotes supported. */
function parseCsv(text: string): Record<string, string>[] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let inQuotes = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (inQuotes) {
      if (c === '"' && text[i + 1] === '"') {
        field += '"';
        i++;
      } else if (c === '"') {
        inQuotes = false;
      } else {
        field += c;
      }
    } else if (c === '"') {
      inQuotes = true;
    } else if (c === ",") {
      row.push(field);
      field = "";
    } else if (c === "\n" || c === "\r") {
      if (c === "\r" && text[i + 1] === "\n") i++;
      row.push(field);
      field = "";
      if (row.some((f) => f !== "")) rows.push(row);
      row = [];
    } else {
      field += c;
    }
  }
  if (field !== "" || row.length > 0) {
    row.push(field);
    if (row.some((f) => f !== "")) rows.push(row);
  }
  const [header, ...body] = rows;
  if (!header) return [];
  return body.map((r) =>
    Object.fromEntries(header.map((h, i) => [h.trim(), r[i] ?? ""])),
  );
}
