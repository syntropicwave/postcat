import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { appSettingsGet, appSettingsSet } from "../ipc/commands";
import type { AppSettings } from "../types";

export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const [s, setS] = useState<AppSettings | null>(null);

  useEffect(() => {
    appSettingsGet().then(setS);
  }, []);

  if (!s) return null;

  const save = async () => {
    await appSettingsSet(s);
    onClose();
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="retention-title">Settings</div>

        <label className="modal-field">
          Proxy
          <select
            value={s.proxy_mode}
            onChange={(e) => setS({ ...s, proxy_mode: e.target.value })}
          >
            <option value="system">System / environment</option>
            <option value="none">No proxy</option>
            <option value="custom">Custom</option>
          </select>
        </label>
        {s.proxy_mode === "custom" && (
          <label className="modal-field">
            Proxy URL
            <input
              value={s.proxy_url}
              placeholder="http://127.0.0.1:8888"
              onChange={(e) => setS({ ...s, proxy_url: e.target.value })}
            />
          </label>
        )}

        <label className="modal-field">
          Extra CA certificates (PEM)
          <div className="cert-row">
            <input
              readOnly
              value={s.ca_cert_paths.join("; ")}
              placeholder="none"
            />
            <button
              onClick={async () => {
                const path = await open({
                  multiple: false,
                  filters: [{ name: "PEM", extensions: ["pem", "crt", "cer"] }],
                });
                if (typeof path === "string")
                  setS({ ...s, ca_cert_paths: [...s.ca_cert_paths, path] });
              }}
            >
              Add…
            </button>
            {s.ca_cert_paths.length > 0 && (
              <button onClick={() => setS({ ...s, ca_cert_paths: [] })}>
                Clear
              </button>
            )}
          </div>
        </label>

        <label className="modal-field">
          Client certificate (PKCS#12 / .pfx)
          <div className="cert-row">
            <input readOnly value={s.client_cert_path} placeholder="none" />
            <button
              onClick={async () => {
                const path = await open({
                  multiple: false,
                  filters: [{ name: "PKCS#12", extensions: ["pfx", "p12"] }],
                });
                if (typeof path === "string")
                  setS({ ...s, client_cert_path: path });
              }}
            >
              Pick…
            </button>
            {s.client_cert_path && (
              <button onClick={() => setS({ ...s, client_cert_path: "" })}>
                Clear
              </button>
            )}
          </div>
        </label>
        {s.client_cert_path && (
          <label className="modal-field">
            Certificate passphrase
            <input
              type="password"
              value={s.client_cert_password}
              onChange={(e) =>
                setS({ ...s, client_cert_password: e.target.value })
              }
            />
          </label>
        )}

        <label className="modal-field">
          Response capture limit (KB)
          <input
            type="number"
            min={64}
            value={s.max_captured_body_kb}
            onChange={(e) =>
              setS({ ...s, max_captured_body_kb: Number(e.target.value) })
            }
          />
        </label>

        <div className="retention-actions">
          <button onClick={onClose}>Cancel</button>
          <button className="primary" onClick={() => void save()}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
