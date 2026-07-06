import { useEffect, useRef, useState } from "react";
import { wsSend } from "../ipc/commands";
import type { Tab } from "../state/tabs";

/** WebSocket session panel: message timeline + composer. */
export function WsPanel({ tab }: { tab: Tab }) {
  const [draft, setDraft] = useState("");
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    listRef.current?.scrollTo({ top: listRef.current.scrollHeight });
  }, [tab.wsMessages.length]);

  const send = async () => {
    if (!draft.trim() || tab.wsStatus !== "open") return;
    await wsSend(tab.id, draft);
    setDraft("");
  };

  return (
    <div className="ws-panel">
      <div className="response-status">
        <span
          className={`status-badge ${
            tab.wsStatus === "open" ? "status-ok" : "status-server-error"
          }`}
        >
          {tab.wsStatus === "open"
            ? "CONNECTED"
            : tab.wsStatus === "connecting"
              ? "CONNECTING"
              : "CLOSED"}
        </span>
        <span className="response-meta">
          {tab.wsMessages.filter((m) => m.kind === "in").length} in ·{" "}
          {tab.wsMessages.filter((m) => m.kind === "out").length} out — the
          session is saved to history on disconnect
        </span>
      </div>

      <div className="ws-messages" ref={listRef}>
        {tab.wsMessages.map((m, i) => (
          <div key={i} className={`ws-msg ws-${m.kind}`}>
            <span className="ws-dir">
              {m.kind === "in"
                ? "◀"
                : m.kind === "out"
                  ? "▶"
                  : m.kind === "open"
                    ? "●"
                    : m.kind === "closed"
                      ? "○"
                      : "!"}
            </span>
            <span className="ws-text">
              {m.kind === "open"
                ? `connected to ${m.text}`
                : m.kind === "closed"
                  ? "connection closed"
                  : m.text}
            </span>
            <span className="hist-time">
              {new Date(m.ts).toLocaleTimeString()}
            </span>
          </div>
        ))}
        {tab.wsMessages.length === 0 && (
          <div className="history-empty">
            Connect and exchange messages — everything lands in history,
            searchable like any request.
          </div>
        )}
      </div>

      <form
        className="ws-composer"
        onSubmit={(e) => {
          e.preventDefault();
          void send();
        }}
      >
        <textarea
          value={draft}
          placeholder={
            tab.wsStatus === "open"
              ? "Message…"
              : "Connect first (button above)"
          }
          disabled={tab.wsStatus !== "open"}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
              e.preventDefault();
              void send();
            }
          }}
        />
        <button
          type="submit"
          className="send-btn"
          disabled={tab.wsStatus !== "open" || !draft.trim()}
          title="Send (Ctrl+Enter)"
        >
          Send
        </button>
      </form>
    </div>
  );
}
