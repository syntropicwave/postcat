import { useEffect, useState } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import { cookieDelete, cookiesClear, cookiesList } from "../ipc/commands";
import type { CookieInfo } from "../types";

export function CookieManager({ onClose }: { onClose: () => void }) {
  const [cookies, setCookies] = useState<CookieInfo[]>([]);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    cookiesList().then(setCookies);
  }, [version]);

  const groups = new Map<string, CookieInfo[]>();
  for (const c of cookies) {
    const list = groups.get(c.domain) ?? [];
    list.push(c);
    groups.set(c.domain, list);
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal modal-wide" onClick={(e) => e.stopPropagation()}>
        <div className="code-toolbar">
          <span className="retention-title">Cookies</span>
          <span style={{ flex: 1 }} />
          <button
            onClick={async () => {
              if (
                await confirm("Delete all cookies?", {
                  title: "Clear cookies",
                  kind: "warning",
                })
              ) {
                await cookiesClear();
                setVersion((v) => v + 1);
              }
            }}
          >
            Clear all
          </button>
          <button className="primary" onClick={onClose}>
            Close
          </button>
        </div>
        <div className="cookie-list">
          {[...groups.entries()].map(([domain, list]) => (
            <div key={domain}>
              <div className="cookie-domain">{domain}</div>
              <table className="cookie-table">
                <tbody>
                  {list.map((c, i) => (
                    <tr key={i}>
                      <td className="header-name">{c.name}</td>
                      <td className="header-value">{c.value}</td>
                      <td className="cookie-meta">
                        {c.path}
                        {c.secure ? " · secure" : ""}
                        {c.expires ? ` · ${c.expires}` : " · session"}
                      </td>
                      <td>
                        <button
                          className="kv-remove"
                          title="Delete cookie"
                          onClick={async () => {
                            await cookieDelete(c.domain, c.path, c.name);
                            setVersion((v) => v + 1);
                          }}
                        >
                          ×
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ))}
          {cookies.length === 0 && (
            <div className="history-empty">
              No cookies yet — they are captured automatically from responses
              and persist across restarts.
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
